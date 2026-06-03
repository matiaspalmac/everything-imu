// everything-imu — Nintendo 3DS / 2DS IMU forwarder (homebrew, clean-room).
//
// Streams the console's 6-axis IMU (accelerometer + gyroscope) to the
// everything-imu desktop app over UDP. The PC side (crates/device-3ds) binds
// port 9305 and treats each console IP as one tracker.
//
// Wire format (must match crates/device-3ds): 12 bytes, little-endian
//   int16 ax, ay, az, gx, gy, gz
// (3DS is little-endian, so the struct is sent verbatim.)

#include <3ds.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <unistd.h>

#define SERVER_PORT 9305
#define CONFIG_PATH "sdmc:/3ds/eimu/server.cfg"
#define DEFAULT_SERVER_IP "192.168.1.50"
#define SOC_BUFFER_SIZE 0x100000
#define LOOP_SLEEP_NS 10000000ULL // 10 ms -> ~100 Hz

typedef struct __attribute__((packed)) {
	s16 ax, ay, az;
	s16 gx, gy, gz;
} ImuPacket;

// Read the server IP from the config file, else fall back to the default.
static void load_server_ip(char *out, size_t cap) {
	FILE *f = fopen(CONFIG_PATH, "r");
	if (f) {
		if (fgets(out, (int)cap, f)) {
			out[strcspn(out, "\r\n")] = '\0';
		} else {
			strncpy(out, DEFAULT_SERVER_IP, cap - 1);
			out[cap - 1] = '\0';
		}
		fclose(f);
	} else {
		strncpy(out, DEFAULT_SERVER_IP, cap - 1);
		out[cap - 1] = '\0';
	}
}

int main(void) {
	gfxInitDefault();
	consoleInit(GFX_TOP, NULL);
	aptSetSleepAllowed(false);

	printf("everything-imu 3DS IMU forwarder\n");
	printf("--------------------------------\n");

	u32 *soc_buf = (u32 *)memalign(0x1000, SOC_BUFFER_SIZE);
	if (!soc_buf || socInit(soc_buf, SOC_BUFFER_SIZE) != 0) {
		printf("network init failed\n");
		goto wait_exit;
	}

	char ip[64];
	load_server_ip(ip, sizeof(ip));
	printf("server: %s:%d\n", ip, SERVER_PORT);

	int sock = socket(AF_INET, SOCK_DGRAM, 0);
	if (sock < 0) {
		printf("socket() failed\n");
		goto net_exit;
	}

	struct sockaddr_in dst;
	memset(&dst, 0, sizeof(dst));
	dst.sin_family = AF_INET;
	dst.sin_port = htons(SERVER_PORT);
	inet_pton(AF_INET, ip, &dst.sin_addr);

	HIDUSER_EnableAccelerometer();
	HIDUSER_EnableGyroscope();
	printf("streaming... press START to exit\n\n");

	u32 frame = 0;
	while (aptMainLoop()) {
		hidScanInput();
		if (hidKeysDown() & KEY_START) break;

		accelVector accel;
		angularRate gyro;
		hidAccelRead(&accel);
		hidGyroRead(&gyro);

		ImuPacket pkt = {
			.ax = accel.x, .ay = accel.y, .az = accel.z,
			.gx = gyro.x,  .gy = gyro.y,  .gz = gyro.z,
		};
		sendto(sock, &pkt, sizeof(pkt), 0,
		       (struct sockaddr *)&dst, sizeof(dst));

		if ((++frame % 50) == 0) {
			printf("\x1b[6;1H");
			printf("accel %6d %6d %6d\n", accel.x, accel.y, accel.z);
			printf("gyro  %6d %6d %6d\n", gyro.x, gyro.y, gyro.z);
		}
		svcSleepThread(LOOP_SLEEP_NS);
	}

	close(sock);
net_exit:
	socExit();
wait_exit:
	printf("\nexiting...\n");
	gfxExit();
	return 0;
}
