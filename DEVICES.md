# Devices Matrix — everything-imu

Hardware reference for every controller everything-imu can bridge. Each section
covers the IMU part, transport, sample rate, calibration source, axis
convention, on-device reset gestures, and known quirks.

## Summary

| Device                  |            VID:PID | IMU part                                 | Transport            |    Rate | Mag | Battery | Rumble |
| ----------------------- | -----------------: | ---------------------------------------- | -------------------- | ------: | :-: | :-----: | :----: |
| Joy-Con L               |        `057E:2006` | LSM6DS3-TR-C¹                            | USB · BT Classic     |  200 Hz |  ✗  |    ✓    |   ✓    |
| Joy-Con R               |        `057E:2007` | LSM6DS3-TR-C¹                            | USB · BT Classic     |  200 Hz |  ✗  |    ✓    |   ✓    |
| Switch Pro Controller   |        `057E:2009` | LSM6DS3-TR-C                             | USB · BT Classic     |  200 Hz |  ✗  |    ✓    |   ✓    |
| Joy-Con 2 (L/R)         |           BLE only | ICM-42670-P + AK09919                    | BLE                  |   62 Hz |  ✓  |    ✓    |   ✓    |
| Switch 2 Pro Controller |           BLE only | ICM-42670-P + AK09919                    | BLE                  |   62 Hz |  ✓  |    ✓    |   ✓    |
| NSO GameCube 2          |           BLE only | ICM-42670-P + AK09919                    | BLE                  |   62 Hz |  ✓  |    ✓    |   ✓    |
| DualSense               |        `054C:0CE6` | BMI270                                   | USB · BT             |  250 Hz |  ✗  |    ✓    |   ✓    |
| DualSense Edge          |        `054C:0DF2` | BMI270                                   | USB · BT             |  250 Hz |  ✗  |    ✓    |   ✓    |
| DualShock 4 v1          |        `054C:05C4` | BMI055                                   | USB                  |  250 Hz |  ✗  |    ✓    |   ✓    |
| DualShock 4 v2          |        `054C:09CC` | BMI055                                   | USB                  |  250 Hz |  ✗  |    ✓    |   ✓    |
| PS Move ZCM1            |        `054C:03D5` | MPU-6050 + AK8975                        | USB · BT             |  175 Hz |  ✓  |    ✓    |   ✓    |
| PS Move ZCM2            |        `054C:0C5E` | MPU-6500                                 | USB · BT             |  175 Hz |  ✗  |    ✓    |   ✓    |
| Wii Remote              |      TCP forwarder | ADXL345 + IDG-600 / ADXL330²             | TCP `127.0.0.1:9909` |  100 Hz |  ✗  |    ✓    |   ✓    |
| 3DS / 2DS (XL)          |      UDP forwarder | ST accel + InvenSense gyro               | UDP `:9305`          |  100 Hz |  ✗  |    ✗    |   ✗    |
| PS Vita                 |      UDP forwarder | 3-axis accel + 3-axis gyro (`sceMotion`) | UDP `:9306`          |  100 Hz |  ✗  |    ✗    |   ✗    |
| DualShock 3             |        `054C:0268` | Kionix accel + 1-axis gyro³              | USB                  | ~100 Hz |  ✗  |    ✗    |   ✗    |
| HOPX / Triki            | BLE (name `Triki`) | LSM6DS (nRF52810)                        | BLE                  |   52 Hz |  ✗  |    ✗    |   ✗    |

¹ Genuine Nintendo. Clones ship with ICM-20600 — auto-detected via SPI ID, fall
back to longer VQF warm-up.
² Wii Remote IMUs vary by revision; values forwarded by the companion process.
³ DualShock 3 has only a **single-axis (yaw) gyroscope** + 3-axis accel, no
mag — experimental/not-recommended tracker (accel-dominant, unconstrained yaw
drift). See `docs/reference/dualshock3_protocol.md`.

Charging Grip (`057E:200E`) enumerates as USB but is not directly driven — it
proxies its docked Joy-Cons. Connect them via Bluetooth instead.

---

## Joy-Con 1 / Switch Pro Controller

**Crate**: `crates/device-joycon/` (`jc1.rs`)
**Status**: hardware-validated (Joy-Con L/R + Pro Controller).

### Hardware

| Component            | Part                            | Notes                              |
| -------------------- | ------------------------------- | ---------------------------------- |
| IMU 6-axis (genuine) | STMicroelectronics LSM6DS3-TR-C | ±2000 dps, ±8 g                    |
| IMU 6-axis (clones)  | InvenSense ICM-20600            | Auto-detected, slower bias warm-up |

### Transport

- **USB**: `hidapi` direct.
- **Bluetooth Classic**: HID over Bluetooth, paired via OS. Windows uses the
  Settings → Devices pairing flow; Linux uses `bluetoothctl`.

### Sample rate

- Input report `0x30` ships **3 IMU samples per 15 ms packet**.
- All three are decoded → effective **200 Hz** per controller.
- Reading only the first sample halves the effective rate; do not regress this.

### Calibration

| Source  | SPI region | Bytes | Notes                                 |
| ------- | ---------- | ----: | ------------------------------------- |
| Factory | `0x6020`   |    12 | Accel + gyro offsets/scales           |
| User    | `0x8026`   |    12 | Override gated by magic bytes `B2 A1` |

Read via subcommand `0x10` (SPI read). Bias is also persisted client-side after
30 s of stillness, see VQF _rest bias estimation_.

### Subcommands

| ID     | Purpose           |
| ------ | ----------------- |
| `0x40` | Enable IMU        |
| `0x30` | Player LEDs       |
| `0x38` | Home LED          |
| `0x48` | HD rumble (basic) |
| `0x10` | SPI read          |

### Axis convention

Body frame is `(x, z, -y)` of the raw IMU output (gravity = +Z when face-up).

### Reset gestures

| Gesture            | Action       |
| ------------------ | ------------ |
| Capture button     | `RESET_YAW`  |
| Home (short press) | `RESET_YAW`  |
| Home (≥1 s hold)   | `RESET_FULL` |

### Quirks

- After bonding via OS, the controller may go to sleep — first input report can
  take ~1.5 s. The driver re-issues `0x40` IMU-enable on first timeout.
- Pro Controller's USB mode requires a `0x80 0x02` handshake before HID; the
  driver handles this on init.

---

## Joy-Con 2 / Pro Controller 2 / NSO GameCube 2

**Crate**: `crates/device-joycon/` (`jc2.rs`)
**Status**: hardware-validated (Joy-Con 2 L); other SKUs share the protocol.

### Hardware

| Component    | Part                           | Scale                                                                                     |
| ------------ | ------------------------------ | ----------------------------------------------------------------------------------------- |
| IMU 6-axis   | TDK InvenSense **ICM-42670-P** | gyro ±2000 dps (coeff `34.8 / INT16_MAX ≈ 0.001062 rad/s/raw`), accel ±8 g (`4096 = 1 g`) |
| Magnetometer | Asahi Kasei **AK09919**        | 3-axis ±4900 µT, 16-bit                                                                   |

### Transport

- **BLE only.** Joy-Con 2 does **not** advertise classic Bluetooth.
- Connect via `btleplug` directly to the advertising peripheral — no Windows
  pairing dialog needed.
- **Reconnect cooldown**: rapid reconnects (within ~30 s) lock the radio for
  several minutes. Always honour the chip's backoff.

### GATT topology

Service UUID `ab7de9be-89fe-49ad-828f-118f09df7fd0` (handle `0x0008`):

| Role                            | UUID                                   | Handle   |
| ------------------------------- | -------------------------------------- | -------- |
| Common input report (`0x05`)    | `ab7de9be-89fe-49ad-828f-118f09df7fd2` | `0x000A` |
| Joy-Con L input (`0x07`)        | `cc1bbbb5-7354-4d32-a716-a81cb241a32a` | `0x000E` |
| Joy-Con R input (`0x08`)        | `d5a9e01e-2ffc-4cca-b20c-8b67142bf442` | `0x000E` |
| Pro Controller 2 input (`0x09`) | `7492866c-ec3e-4619-8258-32755ffcc0f8` | `0x000E` |
| NSO GC2 input (`0x0A`)          | `8261cba1-9435-420c-84d6-f0c75a2c8e4d` | `0x000E` |
| Write commands                  | `649d4ac9-8eb7-4e6c-af44-1ea54fe5f005` | —        |
| Output / rumble                 | `289326cb-a471-485d-a8f4-240c14f18241` | `0x0012` |
| Command response (notify)       | `c765a961-d9d8-4d36-a20a-5315b111836a` | —        |

### Input report `0x05` (62 bytes plaintext)

|     Offset |   Size | Field                                              |
| ---------: | -----: | -------------------------------------------------- |
|     `0x00` |      4 | Counter (`uint32 LE`, +1 per packet)               |
|     `0x04` |      4 | Buttons bitfield                                   |
|     `0x0A` |      3 | Left stick (12-bit packed)                         |
|     `0x0D` |      3 | Right stick                                        |
|     `0x10` |      8 | Mouse data                                         |
| **`0x19`** |  **6** | **Magnetometer X/Y/Z** (`int16 LE`, feature bit 7) |
| **`0x1F`** |  **2** | **Battery voltage mV** (`uint16 LE`)               |
|     `0x21` |      1 | Charging state                                     |
|     `0x22` |      2 | Battery current (feature bit 5)                    |
| **`0x2A`** | **18** | **Motion block** (feature bit 2)                   |
|     `0x3C` |      1 | Left trigger (NSO GC2 only)                        |
|     `0x3D` |      1 | Right trigger (NSO GC2 only)                       |

Motion block (`0x2A`):

| Offset | Size | Field                              |
| -----: | ---: | ---------------------------------- |
|     +0 |    4 | Motion timestamp                   |
|     +4 |    2 | Temperature                        |
|     +6 |    2 | Accel X (`int16 LE`, `4096 = 1 g`) |
|     +8 |    2 | Accel Y                            |
|    +10 |    2 | Accel Z                            |
|    +12 |    2 | Gyro X (`int16 LE`)                |
|    +14 |    2 | Gyro Y                             |
|    +16 |    2 | Gyro Z                             |

Effective IMU rate ≈ **62 Hz** (16 ms packet interval).

### Commands (write characteristic, `WriteWithoutResponse`)

Header: `CMD | 0x91 | 0x01 | SUBCMD | 0x00 | LEN | 0x00 | 0x00 | DATA…`

| CMD        | Purpose                                                                |
| ---------- | ---------------------------------------------------------------------- |
| `0x02`     | Flash read/write (factory cal)                                         |
| `0x03`     | Init (input report select)                                             |
| `0x09`     | Player LED                                                             |
| `0x0A`     | Vibration / sound                                                      |
| `0x0B`     | Battery query                                                          |
| **`0x0C`** | Feature select (enable IMU/mag)                                        |
| ⚠ `0x15`   | **DO NOT USE** — pairing persistence write, _can brick the controller_ |

Canonical IMU enable sequence (validated against real hardware):

1. `0C 91 01 02 00 04 00 00 <mask> 00 00 00` — set feature mask
2. wait **500 ms**
3. `0C 91 01 04 00 04 00 00 <mask> 00 00 00` — activate features

Masks: `0x04` IMU, `0x80` mag, `0xFF` all.

### Memory map (CMD `0x02` flash read)

| Address              | Content                                   |
| -------------------- | ----------------------------------------- |
| `0x13000–0x14FFF`    | Factory data block                        |
| `0x13002`            | Serial number                             |
| `0x13012`            | USB product ID (authoritative variant id) |
| `0x13040 + 4/8/12`   | 3 × `float32` gyro bias                   |
| `0x13100 + 12/16/20` | 3 × `float32` accel bias                  |
| `0x1FA000`           | BLE pairing info (host + LTK)             |

### Axis convention (SDL2 authoritative)

```
output = (raw_x, raw_z, -raw_y)         # body frame, gravity +Z face-up
```

Variant-specific remaps applied on top:

| Variant                  | Remap                                    |
| ------------------------ | ---------------------------------------- |
| Joy-Con 2 L (standalone) | `(x, y, z) → ( z, y, -x)` (+90° about Y) |
| Joy-Con 2 R (standalone) | `(x, y, z) → (-z, y,  x)` (−90° about Y) |

The magnetometer is co-mounted on the same PCB → same base remap as the IMU.

### Reset gestures

| Gesture          | Bit in report                   | Action       |
| ---------------- | ------------------------------- | ------------ |
| Home (short)     | byte `0x05`, bit `0x10`         | `RESET_YAW`  |
| Home (≥1 s hold) | byte `0x05`, bit `0x10` (timer) | `RESET_FULL` |
| Capture          | byte `0x05`, bit `0x20`         | `RESET_YAW`  |

### Auto-calibration

Motion-based auto-cal: when the device is stationary (gyro < 0.05 rad/s, accel
gravity vector stable for 3 s), VQF re-estimates gyro bias automatically. Bias
is persisted to SQLite per MAC.

### Quirks

- The peripheral disappears from BLE scan after pairing — restart the adapter
  if not seen within 30 s.
- Switching variants (e.g. Joy-Con 2 → Pro Controller 2) over the same MAC is
  not supported in a single session.

---

## DualSense / DualSense Edge

**Crate**: `crates/device-dualsense/`
**Status**: hardware-validated (standard DualSense via USB).

### Hardware

| Component  | Part             | Scale                                             |
| ---------- | ---------------- | ------------------------------------------------- |
| IMU 6-axis | Bosch **BMI270** | gyro ±2000 dps, accel `8192 LSB/g` (≈ ±4 g range) |

### Transport

| Mode | Library    |                                        Rate |
| ---- | ---------- | ------------------------------------------: |
| USB  | `hidapi`   | 250 Hz (standard), 1000 Hz reported by Edge |
| BT   | `btleplug` |                                      250 Hz |

### Calibration

Read via **feature report `0x05`** — gyro + accel offsets and scales. Applied
before the sample reaches fusion.

### Reports

| Report ID | Purpose                                  |
| --------- | ---------------------------------------- |
| `0x01`    | Input (USB)                              |
| `0x31`    | Input (BT, 78 bytes incl. CRC32 trailer) |
| `0x02`    | Output (rumble, LEDs)                    |
| `0x05`    | Feature: calibration                     |
| `0x09`    | Feature: pairing info                    |

### Quirks

- DualSense Edge is detected as a distinct kind but uses the same protocol —
  USB-only at present.
- BT requires the CRC32 trailer on output reports; missing it → silent reject.

---

## DualShock 4 (PS4)

**Crate**: `crates/device-dualsense/` (shared module)
**Status**: hardware-validated (v2 via USB).

### Hardware

| Component  | Part             | Scale                              |
| ---------- | ---------------- | ---------------------------------- |
| IMU 6-axis | Bosch **BMI055** | gyro ±2000 dps, accel `8192 LSB/g` |

### Transport

- **USB only** in this bridge. BT works at the OS layer but the bridge does
  not currently negotiate the Bluetooth input mode (queued).

### Calibration

Same feature-report `0x05` scheme as DualSense.

### Quirks

- v1 (`05C4`) and v2 (`09CC`) hardware revisions share the protocol — both
  enumerate as `DualShock4` in the device kind enum.

---

## PS Move (ZCM1 / ZCM2)

**Crate**: `crates/device-psmove/`
**Status**: hardware-validated.

### Hardware

| Variant            | IMU                 | Magnetometer       | Notes                    |
| ------------------ | ------------------- | ------------------ | ------------------------ |
| ZCM1 (PS3 era)     | InvenSense MPU-6050 | Asahi Kasei AK8975 | Mag is **only** on ZCM1  |
| ZCM2 (PS4 refresh) | InvenSense MPU-6500 | —                  | Mag removed, smaller PCB |

### Transport

| Mode | Library           | Notes                                           |
| ---- | ----------------- | ----------------------------------------------- |
| USB  | `hidapi`          | Feature report `0x05` carries pairing handshake |
| BT   | HID report `0x01` | Standard L2CAP after pairing                    |

### Layout

| Aspect      | Value                |
| ----------- | -------------------- |
| Axis remap  | X → X, Y → Z, Z → −Y |
| Accel scale | ±3 g                 |
| Gyro scale  | ±2000 dps            |
| IMU rate    | 175 Hz               |

### Quirks

- ZCM2's `has_magnetometer` flag is auto-set to false; the UI hides the mag
  toggle. Don't try to enable it — there's no sensor there.
- The PS button on the controller cycles tracker LED colors but is not wired
  to a reset action.

---

## Wii Remote

**Crate**: `crates/device-wii/`
**Status**: hardware-validated via TCP forwarder.

### Transport

The Wii Remote is **not** driven directly. A companion process (Bluetooth HID
master) forwards 17-byte legacy Wii input packets over TCP to
`127.0.0.1:9909`. The bridge listens, parses, and emits IMU samples like any
other device.

| Direction          | Protocol                                             |
| ------------------ | ---------------------------------------------------- |
| Companion → bridge | 17-byte Wii packet (button + accel + extension slot) |
| Bridge → companion | Per-controller rumble state + polling interval hint  |

No HID / BLE code path; this crate is a passive packet receiver + back-channel.

### Quirks

- The IMU is accelerometer-only on the bare Wii Remote (no gyro). Fusion falls
  back to gravity-only orientation — yaw will drift unconstrained without
  a MotionPlus or external reference.
- A future iteration will read MotionPlus gyro data from the extension slot
  (already exposed in the 17-byte packet's extension bytes).

---

## HOPX / Triki

**Status**: hardware-validated (live tester).

The HOPX controller (shipped as "Triki") streams its IMU only over Bluetooth LE
through the Nordic UART Service - the host never talks to the sensor directly.

### Hardware

| Component  | Part                             | Notes                                     |
| ---------- | -------------------------------- | ----------------------------------------- |
| BLE SoC    | Nordic nRF52810                  | packs IMU samples into UART notifications |
| IMU 6-axis | STMicroelectronics LSM6DS family | 2000 dps, 16 g                            |

### Transport

- BLE only. Nordic UART Service (`6e400001-...`).
- TX notify `6e400003`, RX write `6e400002`.
- Vendor START command begins the stream; STOP ends it.
- Discovery by advertised **name prefix** `Triki`. The MAC OUI varies per unit, so it is not a usable filter.

### Sample format

- 14-byte record `[0x22][seq][6 x i16 LE]`.
- Gyro X/Y/Z first (offsets 2/4/6), accel X/Y/Z second (8/10/12), little-endian.

### Scale

- Gyro: 70 mdps/LSB (2000 dps).
- Accel: 1/2048 g (~0.488 mg/LSB).
- Native rate: 52 Hz.

### Notes

- 6-axis only - no magnetometer, battery, or rumble exposed in the stream.

## Nintendo 3DS / 2DS (XL)

**Crate**: `crates/device-3ds/`
**Status**: implemented (forwarder + parser + tests); axis remap + accel scale
pending live-console validation. Protocol recovered from a known-working
forwarder — see `docs/reference/3ds_protocol.md`.

### Hardware

Full 6-axis: 3-axis ST accelerometer + 3-axis InvenSense gyroscope. No mag, no
battery/rumble on the wire. Good tracker hardware — quality tier comparable to
Joy-Con 1.

### Transport

The console is not host-drivable, so a **homebrew app runs on the 3DS** and
streams raw IMU over **UDP** to the bridge (Wii-style companion model, but UDP).

| Aspect   | Value                                                |
| -------- | ---------------------------------------------------- |
| Protocol | UDP, bridge binds `0.0.0.0:9305` (`--three-ds-bind`) |
| Packet   | 12 bytes, little-endian: `i16 ax ay az gx gy gz`     |
| Rate     | ~100 Hz                                              |
| Identity | sender IP (one console = one tracker)                |

### Scale

- Gyro: `rad/s = raw * 0.00125`.
- Accel: gravity auto-scale — `division = 9.80665 / mean(|ay|)` over the first
  100 samples, then `m/s² = raw * division`. No magic LSB/g constant.

### Axis convention (provisional)

`accel = (ax, az, ay) * division`, `gyro = (-gx, -gy, -gz) * 0.00125`. Ported
from the working forwarder; confirm on hardware before treating as canonical.

### Quirks

- No buttons in the packet → no on-device reset gesture (UI/software recenter
  only) until the homebrew is extended.
- Old 3DS Wi-Fi is 2.4 GHz b/g — UDP loss tolerated (VQF coasts).

---

## PlayStation Vita

**Crate**: `crates/device-vita/`
**Status**: implemented (forwarder + parser + tests); axis convention pending
live-Vita validation. See `docs/reference/vita_protocol.md`.

### Hardware

Full 6-axis via `sceMotion` (3-axis accel + 3-axis gyro), no mag. Good tracker —
JC1 tier. Not host-drivable, so a VitaSDK homebrew streams over UDP.

### Transport

| Aspect   | Value                                                    |
| -------- | -------------------------------------------------------- |
| Protocol | UDP, bridge binds `0.0.0.0:9306` (`--vita-bind`)         |
| Packet   | 24 bytes, little-endian: 6 × `f32` (accel g, gyro rad/s) |
| Rate     | ~100 Hz                                                  |
| Identity | sender IP                                                |

The Vita SDK returns calibrated floats, so the wire carries SI `f32` values —
`accel_m_s2 = accel_g * 9.80665`, gyro passed through. No raw-count scaling.

### Notes

- eimu-defined protocol (no legacy reference); the companion `.vpk` must match.
- One Vita = one tracker. Distinct port from the 3DS so both can run at once.

---

## DualShock 3 / SIXAXIS (PS3)

**Crate**: `crates/device-dualshock3/`
**Status**: implemented — **experimental**. USB only. Single-axis gyro means
this is a tilt-dominant tracker; ship behind a UI warning. Scales + axis
convention are estimates pending hardware. See `docs/reference/dualshock3_protocol.md`.

### Hardware

3-axis Kionix accelerometer + **single-axis (yaw) gyroscope**, no magnetometer.
Hardware-limited: with one gyro axis there is no drift-free yaw reference and no
rate damping for pitch/roll. **Experimental / not recommended** — same fusion
tier as a bare Wii Remote (accel-gravity orientation, unconstrained yaw drift).

### Transport / enable

USB (HID) or BT. Must be told to start: SET_REPORT feature `0xF4` (`42 0C 00 00`
USB / `42 03 00 00` BT). Input report `0x01`. Motion words are 10-bit, MSB-first
(byte-swap): accel X/Y/Z then the single gyro Z, near offsets `0x29/0x2B/0x2D`
and `0x2F` in the 49-byte report. Treat scales as estimates pending hardware.

### Why it is experimental

The field consensus ("insane drift") and the Linux `hid-sony` driver (accel-only,
gyro inaccurate + revision-dependent) both confirm the hardware ceiling. The
driver only populates the yaw gyro axis (X/Y are zero — no sensor there). Ship
behind a clear UI warning; do not advertise as a serious tracker. Windows often
needs a filter/libusb driver to expose the raw HID interface — document, don't
solve here.

---

## Adding a new device

Minimum surface to land a new driver:

1. Implement the `device-traits::Device` trait in a new `crates/device-<name>/`
   crate. Required: `metadata()`, `caps()`, `start()`, `stop()`.
2. Emit `ChannelInfo::ImuSamples` with raw IMU triples in **m/s²** (accel) and
   **rad/s** (gyro). Wire byte-exactness matters — keep raw bytes intact until
   the fusion stage.
3. Add a factory in `crates/core/src/supervisor.rs` so the bridge knows when to
   spawn the driver.
4. Update this matrix + add a synthetic driver (`crate::synthetic`) so CI can
   exercise the pipeline without hardware.
5. Add at least one byte-fixture test and one end-to-end test in
   `crates/core/tests/`.
