# Devices Matrix — everything-imu

Hardware and protocol reference for supported devices.

---

## Joy-Con 1 / Switch Pro Controller

**Crate**: `crates/device-joycon/`

### Hardware

| Component | Part |
|-----------|------|
| IMU (real Nintendo) | LSM6DS3-TR-C |
| IMU (clones) | ICM-20600 (auto-detect, fall back to VQF warm-up) |

### Transport

- USB: `hidapi` directly
- Bluetooth: HID over Bluetooth Classic, paired via OS

### Calibration

- Factory cal: SPI region 0x6020 (12 bytes accel + gyro offsets/scales)
- User cal: SPI region 0x8026 (override gated by magic 0xB2 0xA1)
- Read via subcommand 0x10 (SPI read)

### Multi-sample decoding

- Input report 0x30 contains 3 IMU samples per 15 ms packet
- Must decode all 3 to maximize tracker rate (effective 200 Hz)

### Subcommands

| ID | Purpose |
|----|---------|
| 0x40 | IMU enable |
| 0x30 | Player LED control |
| 0x38 | Home LED control |
| 0x48 | HD Rumble (basic) |
| 0x10 | SPI read |

---

## Joy-Con 2 (Switch 2) / Pro Controller 2 / NSO GameCube 2

**Crate**: `crates/device-joycon/` (module `jc2.rs`)

### Hardware

| Component | Part | Scale |
|-----------|------|-------|
| IMU 6-axis | TDK InvenSense ICM-42670-P | gyro ±2000 dps (`gyro_coeff = 34.8 rad/s / INT16_MAX = 0.001062 rad/s/raw`), accel ±8 G |
| Magnetometer | Asahi Kasei AK09919 | 3-axis ±4900 µT, 16-bit |

### Transport

- **BLE only** (NOT Bluetooth Classic).
- Connect via `btleplug` to advertising peripheral, NO Windows pairing dialog
- Reconnect cooldown: rapid reconnects lock chip for several minutes — throttle

### GATT services

Service UUID: `ab7de9be-89fe-49ad-828f-118f09df7fd0` (handle `0x0008`)

| Role | UUID | Handle |
|------|------|--------|
| Common input report (0x05) | `ab7de9be-89fe-49ad-828f-118f09df7fd2` | `0x000A` |
| Joy-Con L input (0x07) | `cc1bbbb5-7354-4d32-a716-a81cb241a32a` | `0x000E` |
| Joy-Con R input (0x08) | `d5a9e01e-2ffc-4cca-b20c-8b67142bf442` | `0x000E` |
| Pro Controller 2 input (0x09) | `7492866c-ec3e-4619-8258-32755ffcc0f8` | `0x000E` |
| NSO GC2 input (0x0A) | `8261cba1-9435-420c-84d6-f0c75a2c8e4d` | `0x000E` |
| Write commands | `649d4ac9-8eb7-4e6c-af44-1ea54fe5f005` | — |
| Output/rumble (Joy-Con 2) | `289326cb-a471-485d-a8f4-240c14f18241` | `0x0012` |
| Command response notify | `c765a961-d9d8-4d36-a20a-5315b111836a` | — |

### Input Report 0x05 layout (62 bytes plaintext)

| Offset | Size | Field |
|--------|------|-------|
| 0x00 | 4 | Counter (uint32 LE, +1/packet) |
| 0x04 | 4 | Buttons |
| 0x0A | 3 | Left stick (12-bit packed) |
| 0x0D | 3 | Right stick |
| 0x10 | 8 | Mouse |
| **0x19** | **6** | **Magnetometer X/Y/Z** (int16 LE, feature bit 7) |
| **0x1F** | **2** | **Battery voltage mV** (uint16 LE) |
| 0x21 | 1 | Charging state |
| 0x22 | 2 | Battery current (feature bit 5) |
| **0x2A** | **18** | **Motion block** (feature bit 2) |
| 0x3C | 1 | Left trigger (NSO GC2) |
| 0x3D | 1 | Right trigger (NSO GC2) |

Motion block (0x2A):
- 0x2A + 0 (4): Motion timestamp
- 0x2E (2): Temperature
- 0x30 (2): Accel X (int16 LE, **4096 = 1G**)
- 0x32 (2): Accel Y
- 0x34 (2): Accel Z
- 0x36 (2): Gyro X (int16 LE)
- 0x38 (2): Gyro Y
- 0x3A (2): Gyro Z

### Commands (write characteristic, WriteWithoutResponse)

Header: `CMD | 0x91 | 0x01 | SUBCMD | 0x00 | LEN | 0x00 | 0x00 | DATA...`

| CMD | Purpose |
|-----|---------|
| 0x02 | Flash read/write (factory cal) |
| 0x03 | Init (input report select) |
| 0x09 | Player LED |
| 0x0A | Vibration / sound |
| 0x0B | Battery query |
| **0x0C** | Feature select (enable IMU/mag) |
| 0x15 | **DO NOT USE** — pairing persistence, can brick |

Canonical IMU enable:
1. `0C 91 01 02 00 04 00 00 <mask> 00 00 00` (set feature mask)
2. wait 500 ms
3. `0C 91 01 04 00 04 00 00 <mask> 00 00 00` (enable features)

Mask `0x04` = IMU, `0x80` = mag, `0xFF` = all.

### Memory map (CMD 0x02 flash read)

- `0x13000-0x14FFF`: factory data
- `0x13002`: serial
- `0x13012`: USB product ID (variant detection authoritative)
- `0x13040 + 4/8/12`: 3× float32 gyro bias
- `0x13100 + 12/16/20`: 3× float32 accel bias
- `0x1FA000`: BLE pairing info (host + LTK)

### Axis convention (SDL authoritative)

`output = (raw_x, raw_z, -raw_y)` for body frame (gravity +Z when face-up).

Standalone variant flips:
- JC2 L: `(x, y, z) → (z, y, -x)` (+90° about Y)
- JC2 R: `(x, y, z) → (-z, y, x)` (-90° about Y)

Mag chip co-mounted on same PCB → same base remap as IMU.

### Reset buttons

- Home short = RESET_YAW
- Home long (≥1 s) = RESET_FULL
- Capture = RESET_YAW
- Wired to byte 0x05 bits 0x10 and 0x20 of input report

---

## DualSense (PS5)

**Crate**: `crates/device-dualsense/`

### Hardware

| Component | Part | Scale |
|-----------|------|-------|
| IMU | BMI270 | ±2000 dps, 8192 LSB/g |

### Transport

- USB: `hidapi` (250 Hz, Edge 1000 Hz)
- BT: `btleplug`

### Factory calibration

| Feature report | Purpose |
|----------------|---------|
| 0x05 | Calibration data (gyro + accel offsets/scales) |

---

## DualShock 4 (PS4)

**Crate**: `crates/device-dualsense/` (shared module)

### Hardware

| Component | Part | Scale |
|-----------|------|-------|
| IMU | BMI055 | ±2000 dps, 8192 LSB/g |

### Transport

- USB: `hidapi`

---

## PS Move (ZCM1 / ZCM2)

**Crate**: `crates/device-psmove/`

### Transport

- USB: `hidapi`, feature report 0x05 for pairing
- BT: HID report 0x01

### Layout

| Aspect | Value |
|--------|-------|
| Axis swap | X-Z-Y |
| Accel scale | ±3g |
| Gyro scale | ±2000 dps |

---

## Wii Remote

**Crate**: `crates/device-wii/`

### Transport

- TCP listener (`127.0.0.1:9909`) for forwarded companion packets (17-byte legacy Wii packet format)
- Bi-directional response path returns per-controller rumble state + polling interval
- No HID/BLE — passive packet receiver

---
