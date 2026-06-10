// everything-imu — Wii Remote IMU forwarder (homebrew companion)
//
// Runs on a Wii / Wii U via the Homebrew Channel. Reads up to four Wii Remotes
// through wiiuse (WPAD), including the MotionPlus 3-axis gyroscope, and forwards
// each controller's IMU data to the everything-imu desktop app over a plain TCP
// socket (default port 9909). The PC replies with per-slot rumble flags and a
// requested frame interval.
//
// Protocol: see docs/reference/wii_protocol.md. 17 bytes per controller record,
// little-endian; 5-byte reply (4 rumble + 1 interval).
//
// Build: devkitPPC + libogc/wiiuse. See companions/wii/Makefile.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>

#include <gccore.h>
#include <ogcsys.h>
#include <wiiuse/wpad.h>
#include <network.h>
#include <ogc/lwp_watchdog.h>

#ifndef INADDR_NONE
#define INADDR_NONE 0xFFFFFFFF
#endif

#define MAX_WIIMOTES        4
#define RECORD_BYTES        17
#define REPLY_BYTES         5
#define DEFAULT_SERVER_IP   "10.0.0.21"
#define DEFAULT_SERVER_PORT 9909
#define MP_SETTLE_US        500000  // MotionPlus needs ~500 ms after enable.

static char server_ip[32] = DEFAULT_SERVER_IP;
static int  server_port   = DEFAULT_SERVER_PORT;

static int  sock = -1;
static u32  frame_interval_ms = 16;  // host-controlled; clamped 8..100.

static bool format_set[MAX_WIIMOTES]    = { false, false, false, false };
static bool mp_enabled[MAX_WIIMOTES]    = { false, false, false, false };
static bool mp_unsupported[MAX_WIIMOTES]= { false, false, false, false };
static bool was_vibrating[MAX_WIIMOTES] = { false, false, false, false };

static void *xfb = NULL;
static GXRModeObj *rmode = NULL;

// ---- little-endian helpers -------------------------------------------------

static void put_s16_le(s16 value, u8 *dst) {
    dst[0] = (u8)(value & 0xFF);
    dst[1] = (u8)((value >> 8) & 0xFF);
}

// ---- MotionPlus detection --------------------------------------------------

static bool has_motionplus(int chan) {
    expansion_t exp;
    WPAD_Expansion(chan, &exp);
    return exp.type == EXP_MOTION_PLUS;
}

static bool has_nunchuk(int chan) {
    expansion_t exp;
    WPAD_Expansion(chan, &exp);
    return exp.type == EXP_NUNCHUK;
}

// ---- networking ------------------------------------------------------------

static int socket_open(void) {
    if (sock >= 0) {
        net_close(sock);
        sock = -1;
    }
    int fd = net_socket(AF_INET, SOCK_STREAM, IPPROTO_IP);
    if (fd < 0) {
        printf("socket() failed: %d\n", fd);
        return -1;
    }
    int one = 1;
    net_setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &one, sizeof(one));

    struct sockaddr_in dst;
    memset(&dst, 0, sizeof(dst));
    dst.sin_family = AF_INET;
    dst.sin_port = htons(server_port);
    if (inet_aton(server_ip, &dst.sin_addr) == 0) {
        printf("bad server ip: %s\n", server_ip);
        net_close(fd);
        return -1;
    }
    if (net_connect(fd, (struct sockaddr *)&dst, sizeof(dst)) < 0) {
        printf("connect failed: errno=%d\n", errno);
        net_close(fd);
        return -1;
    }
    sock = fd;
    return 0;
}

// Send the whole payload (net_write may be partial) then read the 5-byte reply
// and apply rumble + frame interval. Drops the socket on any error so the next
// frame reconnects cleanly.
static void send_frame(u8 *payload, int len) {
    if (sock < 0 && socket_open() != 0) {
        usleep(250000);
        return;
    }

    int sent = 0;
    while (sent < len) {
        int w = net_write(sock, payload + sent, len - sent);
        if (w <= 0) {
            net_close(sock);
            sock = -1;
            return;
        }
        sent += w;
    }

    u8 reply[REPLY_BYTES];
    int got = 0;
    while (got < REPLY_BYTES) {
        int r = net_read(sock, reply + got, REPLY_BYTES - got);
        if (r <= 0) {
            net_close(sock);
            sock = -1;
            return;
        }
        got += r;
    }

    for (int i = 0; i < MAX_WIIMOTES; i++) {
        u32 type;
        if (WPAD_Probe(i, &type) == WPAD_ERR_NONE) {
            bool on = reply[i] == 1;
            if (on || was_vibrating[i]) {
                WPAD_Rumble(i, on);
                was_vibrating[i] = on;
            }
        }
    }

    u8 raw = reply[4];
    if (raw < 8)   raw = 8;
    if (raw > 100) raw = 100;
    frame_interval_ms = raw;
}

// ---- per-controller record -------------------------------------------------

// Writes one 17-byte record for slot `i` into `out`. Returns the controller's
// MotionPlus gyro/Nunchuk-accel handling inline.
static void build_record(int i, u8 *out) {
    u32 type;
    if (WPAD_Probe(i, &type) != WPAD_ERR_NONE) {
        memset(out, 0, RECORD_BYTES);
        out[0] = 0xFF;  // slot empty
        return;
    }

    WPADData *d = WPAD_Data(i);
    if (!d) {
        memset(out, 0, RECORD_BYTES);
        out[0] = 0xFF;
        return;
    }

    // First sighting: select the IMU data format and enable MotionPlus.
    if (!format_set[i]) {
        WPAD_SetDataFormat(i, WPAD_FMT_BTNS_ACC_IR);
        if (!mp_unsupported[i]) {
            WPAD_SetMotionPlus(i, 1);
            mp_enabled[i] = true;
            usleep(MP_SETTLE_US);
        }
        format_set[i] = true;
    }

    bool nunchuk = has_nunchuk(i);
    bool motionplus = has_motionplus(i);

    // Arbitrate: a Nunchuk and MotionPlus cannot both stream raw, so if a
    // Nunchuk is attached, drop MotionPlus and forward the Nunchuk accel.
    if (nunchuk && mp_enabled[i]) {
        WPAD_SetMotionPlus(i, 0);
        usleep(MP_SETTLE_US);
        WPAD_SetDataFormat(i, WPAD_FMT_BTNS_ACC_IR);
        mp_enabled[i] = false;
        motionplus = false;
    } else if (!nunchuk && !mp_enabled[i] && !mp_unsupported[i]) {
        // Nunchuk removed: bring MotionPlus back.
        WPAD_SetMotionPlus(i, 1);
        usleep(MP_SETTLE_US);
        mp_enabled[i] = true;
    }

    s16 ax = d->accel.x;
    s16 ay = d->accel.y;
    s16 az = d->accel.z;

    s16 dx = 0, dy = 0, dz = 0;
    u8 nunchuk_flag = 0;
    u8 mp_flag = 0;

    if (nunchuk) {
        nunchuk_flag = 1;
        dx = d->exp.nunchuk.accel.x;
        dy = d->exp.nunchuk.accel.y;
        dz = d->exp.nunchuk.accel.z;
    } else if (motionplus) {
        mp_flag = 1;
        dx = d->exp.mp.rx;
        dy = d->exp.mp.ry;
        dz = d->exp.mp.rz;
        // A controller that reports all-zero gyro after enable does not really
        // have MotionPlus — flag it so we stop re-enabling and fall back to
        // accel-only.
        if (dx == 0 && dy == 0 && dz == 0) {
            mp_unsupported[i] = true;
            mp_enabled[i] = false;
            mp_flag = 0;
            WPAD_SetMotionPlus(i, 0);
        }
    }

    u32 pressed = WPAD_ButtonsHeld(i);
    u8 button = (pressed & (WPAD_BUTTON_1 | WPAD_BUTTON_2)) ? 1 : 0;
    u8 battery = WPAD_BatteryLevel(i);

    out[0] = (u8)i;
    put_s16_le(ax, out + 1);
    put_s16_le(ay, out + 3);
    put_s16_le(az, out + 5);
    put_s16_le(dx, out + 7);
    put_s16_le(dy, out + 9);
    put_s16_le(dz, out + 11);
    out[13] = nunchuk_flag;
    out[14] = mp_flag;
    out[15] = battery;
    out[16] = button;
}

// ---- config (optional SD/USB override) -------------------------------------

static void load_config(void) {
    const char *paths[] = {
        "sd:/apps/eimu-wii/config.txt",
        "usb:/apps/eimu-wii/config.txt",
    };
    for (unsigned p = 0; p < sizeof(paths) / sizeof(paths[0]); p++) {
        FILE *f = fopen(paths[p], "r");
        if (!f) continue;
        char line[128];
        while (fgets(line, sizeof(line), f)) {
            char *s = line;
            while (*s == ' ' || *s == '\t') s++;
            if (*s == '#' || *s == ';' || *s == '\0') continue;
            char *nl = strpbrk(s, "\r\n");
            if (nl) *nl = '\0';
            if (strncasecmp(s, "server_ip=", 10) == 0) {
                strncpy(server_ip, s + 10, sizeof(server_ip) - 1);
                server_ip[sizeof(server_ip) - 1] = '\0';
            } else if (strncasecmp(s, "server_port=", 12) == 0) {
                int port = atoi(s + 12);
                if (port > 0 && port < 65536) server_port = port;
            }
        }
        fclose(f);
        printf("config: %s:%d\n", server_ip, server_port);
        return;
    }
    printf("no config file; using %s:%d\n", server_ip, server_port);
}

// ---- main ------------------------------------------------------------------

int main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    VIDEO_Init();
    WPAD_Init();
    rmode = VIDEO_GetPreferredMode(NULL);
    xfb = MEM_K0_TO_K1(SYS_AllocateFramebuffer(rmode));
    console_init(xfb, 20, 20, rmode->fbWidth, rmode->xfbHeight,
                 rmode->fbWidth * VI_DISPLAY_PIX_SZ);
    VIDEO_Configure(rmode);
    VIDEO_SetNextFramebuffer(xfb);
    VIDEO_SetBlack(false);
    VIDEO_Flush();
    VIDEO_WaitVSync();
    if (rmode->viTVMode & VI_NON_INTERLACE) VIDEO_WaitVSync();

    printf("\x1b[2;0H");
    printf("everything-imu Wii forwarder\n");
    printf("Press HOME to exit.\n\n");

    if (net_init() < 0) {
        printf("net_init failed\n");
    }
    if (fatInitDefault()) {
        load_config();
    } else {
        printf("no SD/USB filesystem; using %s:%d\n", server_ip, server_port);
    }

    char localip[16] = {0}, gateway[16] = {0}, netmask[16] = {0};
    if (if_config(localip, gateway, netmask, true, 20) >= 0) {
        printf("IP: %s\n", localip);
    } else {
        printf("DHCP failed\n");
    }

    WPAD_SetIdleTimeout(36000);

    u8 payload[MAX_WIIMOTES * RECORD_BYTES];

    while (1) {
        u64 start = gettime();
        WPAD_ScanPads();

        // HOME on any connected remote quits.
        for (int i = 0; i < MAX_WIIMOTES; i++) {
            u32 type;
            if (WPAD_Probe(i, &type) == WPAD_ERR_NONE &&
                (WPAD_ButtonsDown(i) & WPAD_BUTTON_HOME)) {
                if (sock >= 0) net_close(sock);
                exit(0);
            }
        }

        for (int i = 0; i < MAX_WIIMOTES; i++) {
            build_record(i, payload + i * RECORD_BYTES);
        }
        send_frame(payload, MAX_WIIMOTES * RECORD_BYTES);

        u32 elapsed_ms = (u32)ticks_to_millisecs(gettime() - start);
        if (elapsed_ms < frame_interval_ms) {
            usleep((frame_interval_ms - elapsed_ms) * 1000);
        }
    }

    return 0;
}
