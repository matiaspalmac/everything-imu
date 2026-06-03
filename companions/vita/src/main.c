// everything-imu — PlayStation Vita IMU forwarder (homebrew, clean-room).
//
// Streams the Vita's 6-axis IMU (accelerometer + gyroscope) to the
// everything-imu desktop app over UDP. The PC side (crates/device-vita) binds
// port 9306 and treats each Vita IP as one tracker.
//
// Wire format (must match crates/device-vita): 24 bytes, little-endian
//   float ax, ay, az   (accelerometer, g)
//   float gx, gy, gz   (gyroscope, rad/s)
// sceMotion already returns calibrated SI-ish floats, so they go on the wire
// verbatim (Vita is little-endian).

#include <psp2/ctrl.h>
#include <psp2/kernel/processmgr.h>
#include <psp2/motion.h>
#include <psp2/net/net.h>
#include <psp2/net/netctl.h>
#include <psp2/sysmodule.h>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define SERVER_PORT 9306
#define CONFIG_PATH "ux0:data/eimu/server.cfg"
#define DEFAULT_SERVER_IP "192.168.1.50"
#define NET_POOL_SIZE (1 * 1024 * 1024)

typedef struct {
	float ax, ay, az;
	float gx, gy, gz;
} ImuPacket;

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

static int net_start(void *pool) {
	sceSysmoduleLoadModule(SCE_SYSMODULE_NET);
	SceNetInitParam param;
	param.memory = pool;
	param.size = NET_POOL_SIZE;
	param.flags = 0;
	int r = sceNetInit(&param);
	if (r < 0 && r != (int)0x80410101 /* already initialised */) {
		return r;
	}
	sceNetCtlInit();
	return 0;
}

int main(void) {
	void *net_pool = malloc(NET_POOL_SIZE);
	if (!net_pool || net_start(net_pool) < 0) {
		sceKernelExitProcess(0);
		return 0;
	}

	char ip[64];
	load_server_ip(ip, sizeof(ip));

	int sock = sceNetSocket("eimu_imu", SCE_NET_AF_INET, SCE_NET_SOCK_DGRAM, 0);
	if (sock < 0) {
		sceKernelExitProcess(0);
		return 0;
	}

	SceNetSockaddrIn dst;
	memset(&dst, 0, sizeof(dst));
	dst.sin_family = SCE_NET_AF_INET;
	dst.sin_port = sceNetHtons(SERVER_PORT);
	sceNetInetPton(SCE_NET_AF_INET, ip, &dst.sin_addr);

	sceMotionReset();
	sceMotionStartSampling();
	sceCtrlSetSamplingMode(SCE_CTRL_MODE_ANALOG);

	for (;;) {
		SceCtrlData ctrl;
		sceCtrlPeekBufferPositive(0, &ctrl, 1);
		if (ctrl.buttons & SCE_CTRL_START) break;

		SceMotionSensorState state;
		if (sceMotionGetSensorState(&state, 1) == 0) {
			ImuPacket pkt = {
				.ax = state.accelerometer.x,
				.ay = state.accelerometer.y,
				.az = state.accelerometer.z,
				.gx = state.gyro.x,
				.gy = state.gyro.y,
				.gz = state.gyro.z,
			};
			sceNetSendto(sock, &pkt, sizeof(pkt), 0,
			             (SceNetSockaddr *)&dst, sizeof(dst));
		}

		// ~100 Hz.
		sceKernelDelayThread(10 * 1000);
	}

	sceMotionStopSampling();
	sceNetSocketClose(sock);
	sceNetCtlTerm();
	sceNetTerm();
	free(net_pool);
	sceKernelExitProcess(0);
	return 0;
}
