# SlimeVR Wire Protocol Reference

Authoritative reference for the UDP tracker protocol implemented by `crates/slime-tracker/`.

---

## Wire format

UDP packets, big-endian byte order. **Every datagram begins with a 12-byte header**: `[u32 BE tag][u64 BE seq]`. The tag is the packet ID; the sequence number is per-tracker monotonically increasing and used by the server to drop out-of-order packets.

### Packet IDs (tracker → server)

| ID | Name | Notes |
|----|------|-------|
| 0 | Heartbeat | Trailing 1-byte tracker id (always 0) |
| 1 | Rotation | **Deprecated** — use ID 17 (`ROTATION_DATA`) for modern servers |
| 2 | Gyro | Raw gyro (rad/s) |
| 3 | Handshake | Initial connect |
| 4 | Acceleration | Linear acceleration vector (m/s², NOT g) |
| 5 | Magnetometer | Mag readings (µT) when 9D fusion is active |
| 10 | Ping/Pong | Server-issued challenge, tracker echoes |
| 12 | Battery level | `[f32 voltage_volts][f32 level_0_to_1]` |
| 15 | Sensor info | Tracker description (multiple per controller for multi-IMU devices) |
| 17 | Rotation data | **Modern rotation** — quaternion in `(X, Y, Z, W)` byte order |
| 21 | Calibration action | User-button reset events (yaw/full/mounting) |
| 22 | Feature flags | **Bidirectional** — see § Feature flags below |
| 23 | Rotation+Accel compact | Q15/Q7 packed combined frame for `BUNDLE_COMPACT` |
| 24 | Ack config change | Tracker → server ack of inbound `SET_CONFIG_FLAG` |
| 25 | Set config flag | **Server → tracker** request (mag enable, etc.) |
| 100 | BUNDLE | Multi-packet batched datagram (gated by server FEATURE_FLAGS) |
| 101 | BUNDLE_COMPACT | Halved bandwidth bundle of `ROTATION_AND_ACCELERATION_COMPACT` frames |

### Handshake (packet 3)

Client → server. Establishes session. Server replies with a packet whose first 4 bytes are `\x03Hey` (0x03486579). Protocol version 19.

```
[4 bytes] u32 BE tag = 3
[8 bytes] u64 BE packet seq
[4 bytes] u32 BE board type
[4 bytes] u32 BE IMU type        (sent as i32 here; SensorInfo uses u8 instead)
[4 bytes] u32 BE MCU type
[12 bytes] 3× i32 BE IMU info    (slot 0 = MagnetometerStatus; slots 1+2 reserved)
[4 bytes] i32 BE protocol version (19)
[1 byte]  u8 firmware string length N (≤ 255)
[N bytes] firmware string (e.g. "EverythingIMU 1.0.0")
[6 bytes] MAC address
```

### Rotation data (packet 17)

```
[4 bytes] u32 BE tag = 17
[8 bytes] u64 BE packet seq
[1 byte]  u8 sensor ID
[1 byte]  u8 data type (1 = Normal)
[16 bytes] 4× f32 BE quaternion in (X, Y, Z, W) byte order
[1 byte]  u8 calibration info (0 normally)
```

**Quaternion byte order**: `(X, Y, Z, W)` — maps to `(i, j, k, w)` in nalgebra notation.

### Accel (packet 4)

```
[4 bytes] u32 BE tag = 4
[8 bytes] u64 BE packet seq
[12 bytes] 3× f32 BE acceleration (X, Y, Z) in m/s²
[1 byte]  u8 sensor ID                                  ← trailing, not leading
```

**Critical**: SlimeVR-Server expects m/s², NOT g. Multiply g by 9.80665. Note tracker id is the LAST byte (vs FIRST in rotation).

### Magnetometer (packet 5)

```
[4 bytes] u32 BE tag = 5
[8 bytes] u64 BE packet seq
[1 byte]  u8 sensor ID
[1 byte]  u8 data type (1 = Normal)
[12 bytes] 3× f32 BE magnetic field (X, Y, Z) in µT
[1 byte]  u8 calibration info
```

### Sensor info (packet 15)

```
[4 bytes] u32 BE tag = 15
[8 bytes] u64 BE packet seq
[1 byte]  u8 sensor ID
[1 byte]  u8 sensor status (0 = Offline, 1 = OK — send 1 to flip dashboard green)
[1 byte]  u8 IMU type
[2 bytes] u16 BE sensor config bitmask
[1 byte]  u8 has completed rest calibration (0 normally)
[1 byte]  u8 tracker position
[1 byte]  u8 tracker data type (0 = Rotation)
```

`sensor_config` bitmask for magnetometer: bit 0 = enabled, bit 1 = supported. So:
- `0x0003` — magnetometer enabled
- `0x0002` — magnetometer supported but disabled
- `0x0000` — magnetometer not supported

For multi-IMU devices, send one `SENSOR_INFO` per logical tracker.

### Feature flags (packet 22, bidirectional)

Tracker advertises capabilities to the server (and vice versa) using the same packet id. Layout:

```
[4 bytes] u32 BE tag = 22
[8 bytes] u64 BE packet seq
[N bytes] flag bytes (LSB0 bit order, byte 0 first)
```

Bit semantics differ per direction:

| Direction | Namespace | Bit 0 | Bit 1 | Bit 2 |
|-----------|-----------|-------|-------|-------|
| Tracker → Server | `FirmwareFeatureFlagBits` | REMOTE_COMMAND | B64_WIFI_SCANNING | SENSOR_CONFIG |
| Server → Tracker | `ServerFeatureFlagBits` | PROTOCOL_BUNDLE_SUPPORT | PROTOCOL_BUNDLE_COMPACT_SUPPORT | — |

The tracker advertises `SENSOR_CONFIG` outbound so the server can issue `SET_CONFIG_FLAG` requests, and reads `PROTOCOL_BUNDLE_SUPPORT` from the inbound reply to drive the BUNDLE auto-fallback gate.

### BUNDLE (packet 100)

Multi-packet batched datagram. ONLY usable if the server's FEATURE_FLAGS reply advertises `PROTOCOL_BUNDLE_SUPPORT` (bit 0). On servers without it, type-100 datagrams are silently dropped.

```
[4 bytes] u32 BE tag = 100
[8 bytes] u64 BE outer packet seq
[ inner ... ]:
    [2 bytes] u16 BE inner length L (covers inner_type + inner_payload, NO inner seq)
    [4 bytes] u32 BE inner type
    [L-4 bytes] inner payload (no per-inner sequence number)
```

Inner packets do **not** carry their own 8-byte sequence number — only the outer one does. The encoder strips bytes `[4..12]` of each pre-built inner packet before copying.

**BUNDLE auto-fallback** (implemented in `slime-tracker/src/client.rs`):

1. On startup, default `server_supports_bundle = false`.
2. After handshake, the server's FEATURE_FLAGS reply lands; parse it and set `server_supports_bundle = (flag_bytes[0] & (1 << PROTOCOL_BUNDLE_SUPPORT)) != 0`.
3. When emitting per-tick rotation+accel:
   - If `server_supports_bundle` → send single BUNDLE (packet 100).
   - Else → send rotation (packet 17) THEN acceleration (packet 4) as separate datagrams. **Rotation first** — accel-first produced visible 1-frame jitter on servers without bundle support.
4. The ~1–2 samples that land before the FEATURE_FLAGS reply go via the fallback path automatically; no data loss.

---

## Wire-compat invariant (CRITICAL)

Any change to the encoder MUST pass reference tests against known-good captures.

### Capture procedure (Windows)

```powershell
# 1. Run everything-imu with 1 controller connected, SlimeVR-Server NOT running
# 2. Capture loopback UDP traffic on port 6969
& "$env:ProgramFiles\Wireshark\dumpcap.exe" -i Loopback -f "udp port 6969" -w fixtures/reference.pcapng
# 3. Extract packet bytes from pcapng
& "$env:ProgramFiles\Wireshark\tshark.exe" -r fixtures/reference.pcapng -T fields -e data > fixtures/reference.txt
# 4. Convert hex to binary fixtures
```

### Test pattern

```rust
#[test]
fn handshake_byte_compat() {
    let golden = include_bytes!("../fixtures/handshake.bin");
    let pkt = Packet::new(0, SbPacket::Handshake {
        board: BoardType::Custom,
        imu: ImuType::Bmi270,
        mcu: McuType::Esp32,
        imu_info: (0, 0, 0),
        protocol_version: 19,
        firmware: SlimeString::from("EverythingIMU 1.0.0"),
        mac_address: [0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE],
    });
    let bytes = pkt.to_bytes().expect("encodes");
    assert_eq!(&bytes[..], &golden[..]);
}
```

Without these, SlimeVR-Server silently rejects packets — silent failures are extremely costly to debug.
