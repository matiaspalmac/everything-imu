<div align="center">
  <h1>everything-imu</h1>
  <p><strong>Cross-platform SlimeVR IMU bridge for game controllers</strong></p>

  <p>
    <a href="https://github.com/matiaspalmac/everything-imu/actions/workflows/ci.yml"><img src="https://github.com/matiaspalmac/everything-imu/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
    <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License"></a>
    <img src="https://img.shields.io/badge/status-stable-brightgreen.svg" alt="Status: stable">
    <img src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20Android-lightgrey.svg" alt="Platform">
    <a href="https://github.com/matiaspalmac/everything-imu/releases/latest"><img src="https://img.shields.io/github/v/release/matiaspalmac/everything-imu?include_prereleases&label=release" alt="Latest release"></a>
  </p>
</div>

`everything-imu` is a native bridge that turns Nintendo and Sony game controllers
into full-body trackers for **SlimeVR-Server**. Plug a controller into your PC,
strap it on, and it shows up in SlimeVR as a regular tracker — no extra hardware,
no firmware flashing.

The bridge reads the controller's built-in IMU at its native sample rate, runs a
sensor fusion filter (VQF or Madgwick) in Rust, and forwards the resulting
quaternion to SlimeVR-Server over UDP using the official protocol.

> **Status:** 1.0.2. The core fusion + protocol pipeline is stable and
> hardware-validated across the supported controllers. If you hit a snag,
> please file an issue with reproduction steps and the logs from the **Logs**
> tab.

## Features

- **9 controller families** supported across USB · BT Classic · BLE · TCP.
- **VQF / Madgwick / BasicVQF** fusion, switchable per-device.
- **Magnetometer calibration wizard** for Joy-Con 2 and PS Move ZCM1 (sphere
  fit + coverage meter).
- **Reset Yaw / Reset Full / Reset Mounting** from the UI, system tray, global
  hotkey, or on-device gesture.
- **VRChat OSC → rumble bridge** with per-rule gain, threshold, pulse mode,
  and a per-device haptic calibration wizard (floor / gain mapping).
- **UDP-forwarded haptic targets** — register `host:port` endpoints as virtual
  rumble devices for remote setups.
- **Linux udev installer** — one-click hidraw access for Joy-Con / DualSense /
  PSMove without sudo.
- **Auto-update at startup** (GitHub Releases), with optional crash reporting
  via Sentry. Both opt-in, both off by default.
- **Live diagnostics** — per-tracker rate panels, bridge latency, raw IMU
  charts on the Debug page, circular battery rings, signal meter.

## Why this exists

SlimeVR-Server already supports full-body tracking with dedicated SlimeVR
hardware. This project plugs the *bring-your-own-IMU* gap: most homes have
controllers with high-quality IMUs sitting in a drawer, and they make great
secondary trackers (waist, knees, feet) when you're trying SlimeVR for the
first time and don't want to commit to buying nodes yet.

We **deliberately do not reimplement SlimeVR-Server features** (skeletal model,
SteamVR driver, calibration UI, etc.). everything-imu is the *bridge layer*
only.

## Supported devices

| Device | Transport | Native rate | Mag | Status |
|--------|-----------|------------:|:---:|--------|
| Joy-Con (L/R) | USB · BT Classic | 200 Hz | ✗ | hardware-validated |
| Switch Pro Controller | USB · BT Classic | 200 Hz | ✗ | hardware-validated |
| Joy-Con 2 / Pro 2 / NSO GC2 | BLE only | 62 Hz | ✓ | hardware-validated |
| DualSense (PS5) | USB · BT | 250 Hz | ✗ | hardware-validated |
| DualSense Edge | USB · BT | 250 Hz | ✗ | needs hardware |
| DualShock 4 (PS4) | USB | 250 Hz | ✗ | hardware-validated |
| PS Move ZCM1 | USB · BT | 175 Hz | ✓ | hardware-validated |
| PS Move ZCM2 | USB · BT | 175 Hz | ✗ | hardware-validated |
| Wii Remote | TCP forwarder (`127.0.0.1:9909`) | 100 Hz | ✗ | hardware-validated |

Deep-dive on each: [DEVICES.md](DEVICES.md).

## Mobile companion (Android + Wear OS)

`mobile/` ships a native Android phone tracker + Wear OS companion that
streams the phone IMU straight to SlimeVR-Server using the same UDP
protocol the desktop bridge speaks — no PC controller required.

- 4-tab UI (Home · Calibrate · Haptics · Settings) sharing the desktop
  sumi-ink theme, with a washi light palette and a follow-system mode.
- Configurable send rate, magnetometer toggle, tracker name, and shake-to-
  recenter — all live-applied without restart.
- Notification quick actions: **Recenter** and **Stop**.
- Battery level reported to SlimeVR-Server (packet 12) every 30 s.
- Auto-reconnect on Wi-Fi changes via `ConnectivityManager.NetworkCallback`.
- Figure-8 magnetometer wizard + automatic gyro-bias estimation.
- VRChat OSC → device vibration bridge on UDP `9001`.
- Foreground service with wake + Wi-Fi locks; OEM battery-optimization
  helper (Xiaomi / Huawei / Samsung / dontkillmyapp guides).
- EN / ES localisation, persisted in DataStore.
- Native VQF fusion via JNI (`crates/jni-android`), with a pure-Kotlin
  Madgwick fallback when the `.so` is unavailable.
- **Wear OS standalone setup** — the watch no longer needs the phone to know
  the server address. A paired watch auto-syncs `host:port` from the phone over
  the Wearable Data Layer; any watch (including AOSP / de-Googled builds with no
  Play Services) can enter the IP directly with an on-watch wheel picker —
  rotary crown or swipe, no keyboard, no companion app required.

Install: grab `everything-imu-phone-<tag>.apk` (and optionally
`everything-imu-wear-<tag>.apk`) from the [latest release](https://github.com/matiaspalmac/everything-imu/releases/latest).
The release APKs are signed; if you are upgrading from an earlier unsigned
build, uninstall it first — Android blocks an in-place upgrade when the signing
key changes. Build locally from `mobile/` with `./gradlew :app-mobile:assembleDebug`.

## Install

Grab the latest installer from the
[**Releases**](https://github.com/matiaspalmac/everything-imu/releases/latest)
page:

- **Windows**: `everything-imu_<version>_x64-setup.exe`
- **Linux (Debian/Ubuntu)**: `everything-imu_<version>_amd64.deb`
- **Linux (any)**: `everything-imu_<version>_amd64.AppImage`

Or build from source — see [Building from source](#building-from-source) below.

## Quickstart

1. **Install SlimeVR-Server** and leave it running. The default UDP port is `6969`.
2. **Run everything-imu**. The status bar at the bottom will read `Live` once a
   handshake with the server completes.
3. **Plug or pair a supported controller**. It appears in the *Devices* tab; a
   tracker for it shows up automatically in SlimeVR-Server.
4. **Mount the controller** to your body (waist, knee, ankle, etc.) and assign it
   in SlimeVR-Server's *Body Proportions* flow.
5. **Reset orientation**: `R` (yaw) / `Shift+R` (full) from the global hotkeys,
   or click *Reset Yaw* / *Reset Full* / *Reset Mounting* on the device card.
   On controllers without a magnetometer (Joy-Con 1, DualSense, DualShock 4,
   Wii Remote) yaw drifts on body rotation — re-yaw facing forward, or use
   Reset Mounting right after strapping the tracker. Some devices accept
   on-device gestures — see [DEVICES.md](DEVICES.md).

### Global hotkeys

| Shortcut | Action |
|----------|--------|
| `Ctrl+K` | Command palette |
| `Ctrl+F` | Global search |
| `Ctrl+Enter` | Cinema mode (immersive overlay) |
| `Ctrl+Shift+B` | Kill-switch the bridge (pause emission) |
| `R` / `Shift+R` | Broadcast yaw / full reset |

Mounting reset is per-device — click *Reset Mounting* on the device card or
the tracker detail page.

## Building from source

### Prerequisites

- **Rust** stable (1.79+ recommended)
- **Node.js** 22+ and **pnpm** 10
- **Windows**: WebView2 (preinstalled on Windows 11)
- **Linux**: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`,
  `librsvg2-dev`, `libayatana-appindicator3-dev`, `libxdo-dev`, `libssl-dev`,
  `libudev-dev`, `patchelf`

### Commands

```bash
git clone https://github.com/matiaspalmac/everything-imu.git
cd everything-imu

pnpm install                  # JS deps
pnpm tauri dev                # dev: live-reload UI + Rust backend
pnpm tauri build              # release bundle (MSI on Windows, AppImage on Linux)

# Backend-only iteration:
cargo test --workspace        # unit + integration tests
cargo run -p headless-cli     # headless bridge for debugging
```

The `everything-imu-app` binary is the full Tauri shell; `headless-cli` is a
no-UI driver useful for tracing protocol-level issues.

## Documentation

| File | Purpose |
|------|---------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Crate graph + responsibilities |
| [DEVICES.md](DEVICES.md) | Per-device IMU, transport, calibration |
| [PROTOCOL.md](PROTOCOL.md) | SlimeVR UDP wire format notes |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Dev workflow, style, PR rules |
| [SECURITY.md](SECURITY.md) | Reporting vulnerabilities |
| [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) | Community ground rules |

## Stack

- **Core**: Rust, `tokio`, `hidapi`, `btleplug`, `nalgebra`
- **Math & fusion**: VQF (Laidig 2023), Madgwick, BasicVQF
- **Haptics**: `rosc` OSC listener, per-device rumble drivers
- **Persistence**: SQLite via `rusqlite`
- **Desktop shell**: Tauri 2 + tauri-specta (typed IPC)
- **Frontend**: React 19, TypeScript, Vite, TailwindCSS 4, Zustand 5,
  react-three-fiber

## License

[MIT](LICENSE-MIT). Contributions are accepted under the same license.
