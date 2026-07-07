# Architecture: everything-imu

## Workspace layout

```
everything-imu/
├── Cargo.toml                           # workspace root
├── Cargo.lock
├── pnpm-workspace.yaml
├── package.json                         # root scripts (tauri orchestration)
├── rust-toolchain.toml                  # pin Rust stable
├── biome.json                           # lint + format config
├── .github/workflows/
│   ├── ci.yml                           # build + test multi-OS
│   └── release.yml                      # tauri bundle on tag push
├── crates/
│   ├── slime-tracker/                   # SlimeVR UDP protocol
│   ├── imu-math/                        # quat ops, coord transforms
│   ├── imu-fusion/                      # VQF, Madgwick, BasicVQF
│   ├── device-traits/                   # interfaces shared across devices
│   ├── device-joycon/                   # JC1, JC2, Pro Controller
│   ├── device-dualsense/                # DS5, DS4
│   ├── device-dualshock3/               # DS3 / SIXAXIS (experimental)
│   ├── device-psmove/                   # ZCM1, ZCM2
│   ├── device-wii/                      # Wii Remote via homebrew Wi-Fi forwarder
│   ├── device-3ds/                      # 3DS / 2DS via homebrew UDP forwarder
│   ├── device-vita/                     # PS Vita via homebrew UDP forwarder
│   ├── device-steam-deck/               # Steam Deck integrated IMU (USB)
│   ├── device-steam-controller/         # Steam Controller (USB wired + dongle)
│   ├── device-tesla/                    # Tesla vehicle heading/speed → synth IMU
│   ├── device-hopx/                     # HOPX / Triki BLE IMU (Nordic UART)
│   ├── device-remote/                   # eimu remote-hub UDP ingest (phone/watch)
│   ├── osc-haptics/                     # VRChat OSC → rumble bridge
│   ├── persistence/                     # rusqlite settings store
│   └── core/                            # AppState, orchestrator
├── companions/                          # console-side forwarders
│   ├── wii/                             # Wii Remote (devkitPPC)
│   ├── 3ds/                             # 3DS / 2DS (devkitARM / libctru)
│   └── vita/                            # PS Vita (VitaSDK)
├── apps/                                # executables (binaries + frontend)
│   ├── everything-imu-app/              # Tauri 2 binary (custom title bar)
│   │   ├── Cargo.toml
│   │   ├── tauri.conf.json
│   │   └── src/
│   ├── headless-cli/                    # daemon binary (no GUI)
│   └── ui/                              # React 19 + Vite 6 + TS
│       ├── package.json
│       ├── vite.config.ts
│       └── src/
│           ├── api/                     # specta-generated bindings + client
│           ├── components/
│           │   ├── layout/              # app shell (title bar, status bar, palette)
│           │   ├── widgets/             # domain widgets (tracker cards, viz, config)
│           │   └── ui/                  # presentational primitives
│           ├── pages/
│           └── stores/                  # Zustand
└── docs/                                # private knowledge base (gitignored)
```

## Crate dependency graph

```
                    ┌──────────────┐
                    │  tauri-app   │
                    └──────┬───────┘
                           │
                    ┌──────▼───────┐
                    │     core     │
                    └──┬─────┬─────┘
                       │     │
        ┌──────────────┼─────┴──────────┬───────────────┐
        │              │                │               │
   ┌────▼────┐  ┌──────▼──────┐  ┌──────▼──────┐  ┌─────▼──────┐
   │device-* │  │slime-tracker│  │ imu-fusion  │  │persistence │
   └────┬────┘  └─────────────┘  └──────┬──────┘  └────────────┘
        │                               │
   ┌────▼────────┐                ┌──────▼──────┐
   │device-traits│                │  imu-math   │
   └─────────────┘                └─────────────┘
```

## Crate responsibilities

### `slime-tracker`

- UDP wire-protocol encoder/decoder (handshake, sensor info, rotation, accel, BUNDLE, FEATURE_FLAGS)
- BUNDLE auto-fallback gating
- Wire-compat tests against reference fixtures

**Public API**:

```rust
pub struct SlimeClient { /* ... */ }
impl SlimeClient {
    pub async fn connect(addr: SocketAddr, info: &HandshakeInfo) -> Result<Self>;
    pub async fn send_sensor_info(&self, desc: &SensorDescriptor) -> Result<()>;
    pub async fn send_rotation_and_accel(&self, id: u8, q: SlimeQuaternion, accel: (f32, f32, f32)) -> Result<()>;
    pub async fn send_battery(&self, voltage: f32, level: f32) -> Result<()>;
    pub async fn send_user_action(&self, action: ActionType) -> Result<()>;
}
```

### `imu-math`

- Quaternion operations (compose, conjugate)
- Coordinate transforms (axis swaps per device convention)
- Calibration application (factory bias + scale, user offsets)

Pure functions, no I/O, deterministic. Uses `nalgebra`.

### `imu-fusion`

- VQF (rest-bias estimation, gyro+accel fusion)
- Madgwick alternative
- BasicVQF lightweight variant

**Test gate**: deterministic input → output match reference within ε < 1e-6 rad.

### `device-traits`

```rust
pub trait Device: Send + Sync {
    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>>;
}

pub trait DeviceFactory: Send + Sync {
    async fn enumerate_loop(&self, tx: mpsc::Sender<(DeviceMetadata, Box<dyn Device>)>) -> Result<()>;
}
```

Implementations in `device-*` crates.

### `device-joycon`

- JC1 + Pro: HID via `hidapi`, SPI calibration read
- JC2: BLE via `btleplug`, GATT services + commands
- Multi-sample input report 0x30 decoding (3 samples per 15 ms)
- Subcommands: LED, rumble, IMU enable
- Variant detection (advert byte 4 mapping)

### `device-dualsense`

- DS5 + DS5 Edge + DS4: USB and Bluetooth, both via `hidapi` (Bluetooth is HID
  over Bluetooth Classic, not BLE)
- USB report `0x01`; BT report `0x31` (DS5) / `0x11` (DS4), told apart by report
  length; BT output rumble carries a trailing CRC32
- Factory calibration read (feature report `0x05`)
- Gyro / accel / battery, PS-button reset, RGB lightbar, DS5 hardware sensor
  timestamp

### `device-psmove`

- ZCM1 + ZCM2: USB via `hidapi`
- Pairing via USB feature 0x05
- Bluetooth report 0x01 layout
- Axis X-Z-Y swap convention

### `device-wii`

- TCP listener for the homebrew Wi-Fi forwarder (Wii Remote + MotionPlus)
- No HID/BLE; passive packet receiver

### `device-hopx`

- HOPX / Triki BLE IMU via `btleplug`
- Nordic UART Service: notifications carry packed gyro + accel
- Discovery by advertised name prefix (`Triki`)
- 6-axis only (no magnetometer, battery, or rumble in the stream)

### `device-remote`

- UDP ingest for the eimu remote protocol (see PROTOCOL.md)
- The phone/watch app forwards its own IMU plus any BLE controllers it owns;
  each announced handle registers as one device through the normal supervisor
- Handles HELLO/ACK discovery, per-handle announce/remove, IMU bursts, battery,
  button resets, and a rumble backchannel

### `persistence`

- `rusqlite` SQLite store
- Schema: settings, calibration cache, user preferences
- Migrations via SQL files
- Cross-platform path resolution: `directories` crate

### `core`

- `AppState`: central registry of devices, fusion engines, SlimeVR connections
- Device discovery orchestration via `Supervisor`
- Per-device `Pipeline`: IMU events → fusion → SlimeVR UDP
- Configuration loading (`persistence` integration)

### `apps/everything-imu-app`

- Tauri 2 binary entrypoint
- Tauri commands: device management, settings, calibration
- Tauri events: `tracker_update`, `device_discovered`
- IPC types via `tauri-specta`
- Window: `decorations: false` (custom title bar)

### `apps/ui` (`@everything-imu/ui`)

- React 19 + Vite 6 + TypeScript 5.6+
- Tailwind 4 via `@tailwindcss/vite`
- Radix UI primitives + shadcn/ui (style `new-york`)
- Phosphor Icons
- `cmdk` command palette (Ctrl+K)
- `uPlot` live charts (tracker freq / latency)
- `@tanstack/react-virtual` for log viewer
- Zustand 5 stores mirror Rust core state via Tauri events

## Data flow (typical)

```
[BLE/HID device] → device-* crate → ChannelInfo stream
                                          │
                                          ▼
                                  imu-fusion (VQF)
                                          │
                                          ▼ Quaternion + accel
                                  slime-tracker
                                          │
                                          ▼ UDP packet
                                  SlimeVR-Server (port 6969)

Parallel:
                                  core::AppState
                                          │
                                          ▼ watch channels
                                  Tauri event "tracker_update"
                                          │
                                          ▼ throttled 30 Hz
                                  React store (Zustand)
                                          │
                                          ▼
                                  Dashboard UI
```

## IPC (UI ↔ core)

`tauri-specta` generates TypeScript types from Rust. Single source of truth.

High-frequency data (tracker pose 60-200 Hz) goes via `emit("tracker_update", payload)`; the UI subscribes and throttles render loop to 30-60 Hz.

## Distribution

| Target  | Bundle                                |
| ------- | ------------------------------------- |
| Windows | NSIS `.exe` installer (Tauri bundler) |
| Linux   | `.deb`, `.AppImage` (Tauri bundler)   |
