# Changelog

All notable changes to this project are documented here. The format is loosely
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
project follows [SemVer](https://semver.org/).

Unreleased changes are tracked on `main`. Tagged releases are listed in
reverse-chronological order below.

## [1.0.0-beta.6] - 2026-05-22

- Crash reporter with opt-in upload of panic backtraces.
- Auto-update flow backed by GitHub releases (`self_update`).
- UDP haptics bridge for OSC-driven rumble.
- Haptic calibrator UI with per-device curves.
- Linux `udev` rule installer.
- Accessibility pass on dashboard widgets (Lighthouse a11y 78 → 98).

## [1.0.0-beta.1] - 2026-05-21

- Joy-Con driver overhaul: JC1 + JC2 connectivity, calibration, and drift fixes.
- Pro Controller 2 support.
- OSC haptics bridge shipped (VRChat → device rumble).

## [1.0.0-beta.0] - 2026-05-17

- First public beta. Full Tauri UI with 8 routes.
- Drivers: Joy-Con (1st gen), DualSense, PSMove, Wii Remote.
- IMU fusion (VQF + Madgwick) and SlimeVR tracker protocol bridge.

[1.0.0-beta.6]: https://github.com/matiaspalmac/everything-imu/releases/tag/v1.0.0-beta.6
[1.0.0-beta.1]: https://github.com/matiaspalmac/everything-imu/releases/tag/v1.0.0-beta.1
[1.0.0-beta.0]: https://github.com/matiaspalmac/everything-imu/releases/tag/v1.0.0-beta.0
