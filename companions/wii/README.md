# everything-imu — Wii Remote IMU forwarder (homebrew)

Runs on a Wii or Wii U and streams Wii Remote (Plus) IMU data — core
accelerometer **and** the MotionPlus 3-axis gyroscope — to the everything-imu
desktop app over TCP. Up to four remotes at once.

This is the **console-side** half of the Wii path. The desktop app provides the
PC-side TCP listener (`crates/device-wii`). Protocol details: see
`docs/ref_wii_protocol.md`.

## Why a homebrew forwarder?

The PC never talks Bluetooth to the Wiimote. The console reads the remotes
through `wiiuse` (which already handles the MotionPlus extension handshake) and
forwards plain binary frames over a socket. That keeps the PC side identical on
Windows and Linux — no BlueZ, no Windows BT stack, no `hid-wiimote` contention.

## Build

Requires the devkitPro toolchain:

- `devkitPPC`
- `libogc` (provides `wiiuse`, `libfat`, networking)

Install via devkitPro pacman (`wii-dev` group), then:

```sh
export DEVKITPRO=/opt/devkitpro
export DEVKITPPC=$DEVKITPRO/devkitPPC
cd companions/wii
make
```

Output: `eimu-wii.dol`.

## Install & run

1. Copy to your SD/USB card under `apps/eimu-wii/`:
   - `eimu-wii.dol` → rename to `boot.dol`
   - (optional) `meta.xml`, `icon.png` for the Homebrew Channel
2. (Optional) create `apps/eimu-wii/config.txt`:
   ```
   server_ip=192.168.1.50
   server_port=9909
   ```
   Without it the compiled-in default IP/port is used.
3. Pair your Wii Remote(s) to the **console** as usual.
4. Launch from the Homebrew Channel. It connects to the PC, which auto-detects
   each remote as a tracker.
5. Press **HOME** to exit.

## Notes

- A Wii Remote **Plus** (or original Wiimote + MotionPlus add-on) is required
  for gyroscope data. Without MotionPlus the forwarder still sends accelerometer
  data (orientation only, no fast-rotation tracking).
- A Nunchuk and MotionPlus cannot both stream raw at once. When a Nunchuk is
  attached the forwarder switches that remote to Nunchuk-accel mode; remove the
  Nunchuk to restore gyro.
