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

`everything-imu` is a native bridge that turns Nintendo, Sony, and other game controllers
into full-body trackers for **SlimeVR-Server**. Plug a controller into your PC,
strap it on, and it shows up in SlimeVR as a regular tracker — no extra hardware,
no firmware flashing.

It also incorporates **Haptics** via the device's rumble, for information on how to set this up on your Avatar, go [HERE](#haptics)

The bridge reads the controller's built-in IMU at its native sample rate, runs a
sensor fusion filter (VQF or Madgwick) in Rust, and forwards the resulting
quaternion to SlimeVR-Server over UDP using the official protocol.

> **Status:** 1.0.4. The core fusion + protocol pipeline is stable and
> hardware-validated across the supported controllers. If you hit a snag,
> please file an issue with reproduction steps and the logs from the **Logs**
> tab.
## Features

- **13+ controller & device families** supported across USB · BT Classic · BLE ·
  TCP · UDP forwarders.
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
| HOPX / Triki | BLE | 52 Hz | ✗ | hardware-validated |
| 3DS / 2DS (XL) | UDP forwarder (`:9305`) | 100 Hz | ✗ | needs hardware |
| PS Vita | UDP forwarder (`:9306`) | 100 Hz | ✗ | needs hardware |
| DualShock 3 (PS3) | USB | ~100 Hz | ✗ | experimental¹ |
| Steam Deck | USB (integrated) | 250 Hz | ✗ | needs hardware |
| Steam Controller | USB · BLE | 100 Hz | ✗ | needs hardware |
| Tesla (vehicle IMU) | Fleet API | — | ✗ | experimental |

¹ DualShock 3 has only a single-axis (yaw) gyroscope and no magnetometer — it is
a tilt-dominant, drift-prone tracker included for completeness. Not recommended.

The Wii, 3DS, and PS Vita are not host-drivable, so they stream their IMU from a
small companion app over the network. The Wii forwarder lives in
[`companions/wii/`](companions/wii); the 3DS and Vita homebrew live in
[`companions/3ds/`](companions/3ds) and [`companions/vita/`](companions/vita).

Deep-dive on each: [DEVICES.md](DEVICES.md).

## Mobile companion (Android + Wear OS)

The native Android phone tracker + Wear OS companion now live in their own
repository: **[everything-imu-mobile](https://github.com/matiaspalmac/everything-imu-mobile)**.
They stream the phone IMU straight to SlimeVR-Server over the same UDP protocol
this desktop bridge speaks — no PC controller required — and receive VRChat OSC
haptics back. Grab the phone/watch APKs from that repo's releases.

## Install

Grab the latest installer from the
[**Releases**](https://github.com/matiaspalmac/everything-imu/releases/latest)
page:

- **Windows**: `everything-imu_<version>_x64-setup.exe`
- **Linux (Debian/Ubuntu)**: `everything-imu_<version>_amd64.deb`
- **Linux (any)**: `everything-imu_<version>_amd64.AppImage`

Or build from source — see [Building from source](#building-from-source) below.

## HAPTICS
### VRChat Avatar Setup
You **MUST** have [VRCFury](https://vrcfury.com/download) on your Unity Project.

You **MUST NOT** Upload the avatar as a *test*, as it prevents the initialization of OSC protocols, please create a duplicate avatar and test it from there. 

1. Import the [IMUHaptics.unitypackage](https://github.com/matiaspalmac/everything-imu/releases/latest/download/IMUHaptics.unitypackage) into your Unity Project

2. Drag and drop the prefab found under `Assets/MooshPaw/IMU Haptics` into your Avatar
<img width="489" height="698" alt="image" src="https://github.com/user-attachments/assets/183da3cd-8745-402b-9380-9466f5a2eca8" />

3. Move the GameObjects to match your avatar proportions

<img width="442" height="547" alt="image" src="https://github.com/user-attachments/assets/d4ea4d7e-2630-4f16-8a72-b6b62f8f9d6f" />

<img width="1373" height="598" alt="image" src="https://github.com/user-attachments/assets/d693fbad-2e9c-49ea-8437-b1f3b97303f4" />

4. (Optional) Disable Haptic points you won't use, this frees your PC from loading unnecessary parameters, even though the performance impact is minimal

<img width="187" height="100" alt="image" src="https://github.com/user-attachments/assets/38c9e30c-a7ca-47e6-b8e1-ff0edef4686c" />

5. Upload avatar and test

### How to create more points of tracking?
Duplicate any of the existing tracking points and position it where you want them. Then, change the Avatar Parameter to `Haptics/{YourDesiredName}` and ***REPLACE*** the parent bone under **Armature Link** 

* You may also move the GameObject to the bone you want it parented to in the *Hierarchy*
* You should **avoid** putting the bone on the **Contact Receiver** Component as the [*Gizmo*](#gizmo) won't Sync

***To create one from scratch***, take in mind the following:
You **MUST** Change the Parameter address. It's recommended to leave the `Haptic/` nomenclature for organization
You **Should** leave *Local Only* enabled, as there's no need to Sync this parameter since your Haptics run locally. This allows the avatar to get a better rating and avoids VRChat's maximum contacts

<img width="479" height="201" alt="image" src="https://github.com/user-attachments/assets/2f3a90b9-fbbd-4e48-afca-1d292cfaeb06" />

**Armature Link** automatically parents your haptic point to a Humanoid bone. You must change this bone to the one you want parented to, or move the GameObject within a bone from the *Hierarchy*.

**Avoid** putting the bone on the **Contact Receiver** Component as the [*Gizmo*](#gizmo) won't Sync

<img width="488" height="220" alt="image" src="https://github.com/user-attachments/assets/05287ac1-386a-4967-a584-c04c73c4231a" />

<img width="401" height="76" alt="image" src="https://github.com/user-attachments/assets/f76339b2-6270-4cf4-82eb-0b6a20bbc0bb" />

### Gizmo

<img width="493" height="183" alt="image" src="https://github.com/user-attachments/assets/bea44209-1f01-4038-868e-cec65a3e669e" />

The gizmo represents the center of the Contact Receiver, as well as where your IRL Tracker should be.
If your tracker is going to be at your stomach, your Haptic point should also be there

<img width="251" height="316" alt="image" src="https://github.com/user-attachments/assets/08be4641-7d34-4db0-8b53-263c8df9e061" />

<img width="791" height="364" alt="image" src="https://github.com/user-attachments/assets/4e6eedc2-b76b-4713-abf2-93b342356bf0" />

### In-App setup

<img width="1096" height="577" alt="image" src="https://github.com/user-attachments/assets/fbdce4e9-9f52-40b8-a21a-3a638982b672" />

1. Go to the Haptics section
2. Enable the *Bridge*
3. Ensure the OSC Port is `9001` (VRChat's default output)
4. Test your devices
5. Add the mappings you need
6. Change the Parameter Name to `/avatar/parameters/Haptics/{YourHapticPoint}`. It **MUST BE THE SAME NAME** as the one from the VRChat contact receiver component. If you misspell it, you won't get any haptic
7. Select the device that will rumble when your haptic point is triggered
8. Adjust settings accordingly

***Proximity (variable)*** 

<img width="159" height="64" alt="image" src="https://github.com/user-attachments/assets/de9188e0-7890-4939-babc-a47ad0704e82" />

Variable float strength from 0 to 1, will rumble more when the VRC contact receiver is touched at the center, and less when is touched at the edges. 

***Pulse (fixed)*** 

<img width="157" height="61" alt="image" src="https://github.com/user-attachments/assets/58e3e542-311d-473a-bd86-0360c71df2a5" />

Activates the Rumble **Once** for a certain amount of time (*Pulse (ms)*) before turning it off. Useful to pair with a VRC contact receiver set to **On Enter**

***Select available OSC addresses***

<img width="1023" height="150" alt="image" src="https://github.com/user-attachments/assets/9393ee7b-a520-47e7-979f-9526d949248c" />

To avoid typing the address manually, you may add them via the **Discovered OSC Addresses** section.

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
   Wii Remote, 3DS, PS Vita, DualShock 3) yaw drifts on body rotation — re-yaw
   facing forward, or use
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
