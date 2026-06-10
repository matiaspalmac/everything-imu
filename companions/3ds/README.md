# everything-imu — 3DS / 2DS IMU forwarder (homebrew)

Runs on any 3DS/2DS family console and streams its 6-axis IMU (accelerometer +
gyroscope) to the everything-imu desktop app over UDP.

This is the **console-side** half of the 3DS path. The desktop app provides the
PC-side UDP listener (`crates/device-3ds`, port 9305). Protocol details:
`docs/reference/3ds_protocol.md`.

## Why a homebrew forwarder?

The 3DS is not a host peripheral — it can't be driven over USB/BT like a
controller. So the console reads its own IMU via `libctru` and forwards plain
binary frames over Wi-Fi. Same model as the Wii/Vita companions.

## Build

Requires the devkitPro toolchain:

- `devkitARM`
- `libctru` (devkitPro pacman group `3ds-dev`)

```sh
export DEVKITPRO=/opt/devkitpro
export DEVKITARM=$DEVKITPRO/devkitARM
cd companions/3ds
make
```

Output: `eimu-3ds.3dsx` (+ `.smdh`).

## Install & run

1. Copy `eimu-3ds.3dsx` to your SD card under `sdmc:/3ds/`.
2. (Optional) set the PC IP in `sdmc:/3ds/eimu/server.cfg` — a single line:
    ```
    192.168.1.50
    ```
    Without it the compiled-in default IP is used. Port is fixed at 9305.
3. Connect the console to the same Wi-Fi network as the PC.
4. Launch from the Homebrew Launcher. It enables the sensors and starts
   streaming; the desktop app auto-detects it as a tracker.
5. Press **START** to exit.

## Notes

- Full 6-axis (accel + gyro), no magnetometer — gyro-integrated yaw with
  motion-autocal, same quality tier as a Joy-Con. Use the app's recenter to
  reset yaw.
- One console = one tracker.
- 2.4 GHz Wi-Fi only on older 3DS units — occasional UDP loss is harmless (the
  fusion filter coasts).
- Axis mapping / accel scale are validated against a working reference but should
  be reconfirmed on your unit; report drift or inverted axes.
