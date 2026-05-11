<div align="center">
  <h1>everything-imu</h1>
  <p>Cross-platform SlimeVR IMU bridge</p>
</div>

`everything-imu` is a modern, high-performance bridge that connects Nintendo and Sony controllers to SlimeVR, turning them into full-body tracking devices.

## Features

- **Cross-platform**: Written in Rust + Tauri 2. Runs as a lightweight native app on Windows and Linux.
- **High performance**: Low-latency sensor fusion (VQF, Madgwick) running natively in Rust.
- **Robust protocol**: Fully compliant SlimeVR UDP protocol implementation with auto-fallback for older servers.
- **Modern UI**: Clean React 19 interface with real-time 3D tracking visualization.

## Supported Devices

- **Joy-Con (L/R)** (2017)
- **Switch Pro Controller**
- **Joy-Con 2** (2024 BLE models)
- **DualSense (PS5)**
- **DualShock 4 (PS4)**
- **PS Move (ZCM1/ZCM2)**
- **Wii Remote** (via forwarded companion TCP `127.0.0.1:9909`)

For complete hardware details and IMU specs, see our [Devices Matrix](DEVICES.md).3

### Building from source

#### Prerequisites
- Rust stable
- Node.js 22+ & pnpm 10
- **Windows**: WebView2 (preinstalled on Win11)
- **Linux**: `webkit2gtk-4.1`, `libgtk-3-dev`, `libsoup-3.0-dev`, `librsvg2-dev`, `libudev-dev`

```bash
# Clone the repository
git clone https://github.com/matiaspalmac/everything-imu.git
cd everything-imu

# Install JS dependencies
pnpm install

# Run the app in development mode
pnpm tauri dev

# Or build the release bundle
pnpm tauri build
```

## Documentation

Dive deeper into how `everything-imu` works under the hood:

- [Architecture Overview](ARCHITECTURE.md) - Crate dependency graph and responsibilities.
- [SlimeVR Wire Protocol](PROTOCOL.md) - Technical reference for our UDP implementation.
- [Devices Matrix](DEVICES.md) - Hardware deep-dive for controllers and IMU sensors.
- [Contributing](CONTRIBUTING.md) - Guidelines for code style, pull requests, and tests.

## Stack

- **Core**: Rust, `tokio`, `hidapi`, `btleplug`, `nalgebra`
- **Math & Fusion**: VQF, Madgwick
- **Desktop Shell**: Tauri 2
- **Frontend**: React 19, TypeScript, Vite, TailwindCSS 4, shadcn/ui, Zustand 5, three.js

## License

This project is licensed under the [MIT License](LICENSE-MIT).
