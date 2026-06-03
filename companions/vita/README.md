# everything-imu — PS Vita IMU forwarder (homebrew)

Runs on a PlayStation Vita (or PS TV) and streams its 6-axis IMU (accelerometer
+ gyroscope) to the everything-imu desktop app over UDP.

This is the **console-side** half of the Vita path. The desktop app provides the
PC-side UDP listener (`crates/device-vita`, port 9306). Protocol details:
`docs/ref_vita_protocol.md`.

## Why a homebrew forwarder?

The Vita can't be driven as a host peripheral, so it reads its own IMU via
`sceMotion` and forwards the values over Wi-Fi. The SDK already returns
calibrated floats (accelerometer in g, gyroscope in rad/s), so they go straight
on the wire — no raw-count scaling.

## Build

Requires [VitaSDK](https://vitasdk.org) and CMake:

```sh
export VITASDK=/usr/local/vitasdk
cd companions/vita
cmake -B build .
cmake --build build
```

Output: `build/eimu-vita.vpk`.

## Install & run

1. Install `eimu-vita.vpk` with **VitaShell** (or any homebrew installer). Needs
   a homebrew-enabled Vita (HENkaku / h-encore / Enso).
2. (Optional) set the PC IP in `ux0:data/eimu/server.cfg` — a single line:
   ```
   192.168.1.50
   ```
   Without it the compiled-in default is used. Port is fixed at 9306.
3. Connect the Vita to the same Wi-Fi network as the PC.
4. Launch **everything-imu Vita** from the LiveArea. It starts sampling and
   streaming; the desktop app auto-detects it as a tracker.
5. Press **START** to exit.

## Notes

- Full 6-axis (accel + gyro), no magnetometer — gyro-integrated yaw with
  motion-autocal, Joy-Con tier. Use the app's recenter to reset yaw.
- One Vita = one tracker. Uses a different port (9306) from the 3DS (9305), so
  both can stream to the same PC at once.
- Runs without on-screen output (the LiveArea/black screen is normal while
  streaming). START exits cleanly.
- Axis mapping should be reconfirmed on hardware; report drift or inverted axes.
