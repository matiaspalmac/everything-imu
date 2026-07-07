# Changelog

All notable changes to this project are documented here. The format is loosely
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
project follows [SemVer](https://semver.org/).

Unreleased changes are tracked on `main`. Tagged releases are listed in
reverse-chronological order below.

## [Unreleased]

## [1.0.7] - 2026-07-06

### Fixed

- DualShock 4 over Bluetooth now streams motion data on Windows. The input
  report was matched on an exact length, but Windows delivers it padded, so
  every Bluetooth frame previously decoded to zero and tracking was dead over
  BT. Hardware-validated.
- DualSense over Bluetooth now reports correct motion and the PS button. The
  gyroscope, accelerometer, and PS/Mute byte offsets were off by one, producing
  garbage orientation and a non-responsive PS button over Bluetooth.
  Hardware-validated.
- Guarded the fusion filters against non-finite input so a bad sample can no
  longer poison the orientation estimate.

### Changed

- Tuned the default Madgwick filter gain to a standard value; the previous
  default over-weighted the accelerometer and produced a jumpy estimate for
  anyone selecting that algorithm.
- Correctness and robustness hardening across the device drivers, networking,
  and diagnostics from a full-repository audit.

## [1.0.5] - 2026-06-10

### Added

- Nintendo 3DS / 2DS support via a libctru homebrew UDP forwarder (`:9305`),
  with a clean-room companion app in `companions/3ds/`.
- PlayStation Vita support via a VitaSDK homebrew UDP forwarder (`:9306`), with a
  companion app in `companions/vita/`.
- DualShock 3 / SIXAXIS (PS3) USB driver — experimental: single-axis gyro, no
  magnetometer, drift-prone, not recommended.
- Headless CLI flags `--three-ds-bind` and `--vita-bind` for the forwarder
  listen addresses.
- Logs page export and live-view pause controls.
- New eIMU logo and app icon set.

### Changed

- Full UI redesign: new "Ember" theme (charcoal + orange), flat surfaces across
  every page, and snappier rendering — the 3D tracker view now renders on
  demand and WebGL loads lazily.
- Connection page protocol labels rewritten in plain language.
- The Android phone and Wear OS apps moved to a dedicated repository,
  [everything-imu-mobile](https://github.com/matiaspalmac/everything-imu-mobile).
  The `jni-android` crate and the Android CI jobs moved with them; this repo is
  now desktop + console forwarders only.
- Repository layout reorganized: binaries now live under `apps/`
  (`everything-imu-app`, `headless-cli`, `ui`), libraries stay under `crates/`.

## [1.0.4] - 2026-06-01

### Added

- HOPX / Triki BLE IMU support (Nordic UART Service driver, name-based discovery).
- DualShock 4 Bluetooth input and DualSense RGB lightbar control.
- Wii Remote MotionPlus gyro via a homebrew Wi-Fi forwarder companion.
- PS Move factory and magnetometer calibration, plus USB pairing.
- Diagnostics CLI flags: `--hopx-raw`, `--ds-raw`, `--wii-raw` / `--wii-bind`, `--ps-pair`.
- In-app and VRChat avatar haptics setup guide.

### Changed

- DualSense fusion timestep now driven by the controller hardware sensor clock.
- Phone and Wear OS apps: single connect button, launch auto-connect, adaptive
  gyro-rate fusion, OS rotation source, and steadier networking.
- Dependency updates (uuid, winreg, vitest, zustand, @vitejs/plugin-react,
  @tanstack/react-virtual).

### Fixed

- Joy-Con gyro drift on high-rate Bluetooth links (live-adaptive fusion timestep).
- Corrected Wii Remote accelerometer scale.
- PS Move HID input-report parser and axis frame.

## [1.0.2] - 2026-05-28

### Changed

- Desktop app promoted to stable `1.0.2`, aligning the version line with the
  mobile clients. Supersedes the `1.0.0-beta.x` / `1.0.1-beta.0` pre-releases;
  no functional change beyond the version bump.

## [mobile/1.0.2] - 2026-05-28

### Added

- Wear OS standalone host configuration. The watch no longer needs a companion
  phone to set the SlimeVR server address:
    - **Auto-sync from phone.** When the watch is paired and Play Services is
      present, setting the server address on the phone pushes it to the watch over
      the Wearable Data Layer; the watch persists it automatically.
    - **On-watch IP picker.** A rotary/swipe wheel picker (four octet wheels plus
      a port wheel) lets you enter the address directly on the watch — no keyboard,
      no network, no Play Services. This is the universal fallback for AOSP /
      de-Googled / China-market watches where Data Layer sync is unavailable.
    - An "Edit IP" button on the watch reopens the picker at any time.
- Release APK signing wired through Gradle and CI so the phone and watch builds
  share one signing key (a hard requirement for the Data Layer to deliver
  messages between the two apps).

### Changed

- Mobile phone + wear apps bumped to `1.0.2`.

### Fixed

- Release APKs are now signed. The previous unsigned release artifacts failed to
  install ("package appears invalid") because Android rejects APKs with no
  certificate.

### Upgrade notes

- Upgrading from an earlier unsigned build: uninstall the old app first, then
  install `1.0.2`. Android refuses an in-place upgrade when the signing key
  changes.

## [1.0.0-beta.8] - 2026-05-24

### Security

- Bumped `sentry` 0.34 → 0.48 to drop the vulnerable `rustls` 0.22.4 /
  `rustls-webpki` 0.102.8 subtree. Closes GHSA-82j2-j2ch-gfr8 (high —
  DoS via panic on malformed CRL BIT STRING) and three medium/low
  `rustls-webpki` advisories (CRL distribution-point matching and
  name-constraint handling).
- pnpm override forces transitive `ws` to `>=8.20.1` (8.21.0 lands),
  closing GHSA-58qx-3vcg-4xpx (medium — uninitialized memory disclosure
  in `ws` receive path) reachable via `jsdom` from vitest.

## [1.0.0-beta.7] - 2026-05-24

### Fixed

- `slime-tracker`: receive + watchdog tasks are now aborted on `SlimeClient`
  drop instead of leaking across device reconnects.
- `slime-tracker`: `encode_bundle` returns `Err` on u16 length overflow
  (previously `debug_assert!` that silently truncated in release).
- `slime-tracker`: `SlimeString::from` truncates payloads longer than the
  u8 length prefix instead of wrapping the cast.
- `persistence` + `device-traits`: in-memory and SQLite stores reject NaN
  and ±inf values for bias and rotation offsets; defense in depth at the
  trait, writer, and reader.
- `persistence`: rotation-offset key now uses lower-case hex consistently
  with the rest of the per-device key namespace, fixing a casing mismatch
  that silently dropped the user's saved offset.
- `core`: outbound accel is now remapped into the VQF/SlimeVR body frame
  so the rotation + accel bundle shares a single gravity vector; the mag
  channel applies the hard-iron calibration before send.
- `core`: rotation-offset gate raised from `f32::EPSILON` to `1e-4` deg
  and the composed quaternion is re-normalized after mounting/offset
  composition.
- `core`: app shutdown waits per-pipeline with a 2 s timeout so a hung
  UDP send cannot block global teardown.
- `device-joycon` / `device-dualsense` / `device-psmove`: HID factories
  prune disconnected entries so a mid-session unplug re-emits cleanly;
  `HidReaderHandle` Drop trips the shutdown atomic if the owner forgot
  to call `shutdown()`.
- `device-joycon`: `JoyCon2Device::stop()` bounds its task join to 2 s.
- `device-wii`: companion handler closes the connection on packet-grid
  desync instead of feeding garbage to the parser.
- `device-traits`: each `MockDevice` instance now gets a unique
  locally-administered MAC; `start` errors on double-start, `stop` joins
  and aborts the emitter task.
- `everything-imu-app`: emitter loops (tracker, bias, latency, connection
  status) adopt `MissedTickBehavior::Skip`.
- `osc-haptics`: serve loop snapshots rules once per session instead of
  per packet.
- `headless-cli`: doctor reuses a single `list_paired` probe.
- UI: status-bar sparkline now repaints (state-based history),
  TrackerDetailPage hydrates the persisted rotation offset on mount,
  DevicesPage batch-hide toast uses a proper plural string, DashboardPage
  navigates via react-router instead of `window.location.hash`,
  `useLogStore.pushBatch` honors the paused flag.

### Tooling

- Release workflow autodetects prerelease tags via the `-alpha|-beta|-rc`
  suffix instead of always shipping as stable.
- CI gates UI tests (`pnpm test`) in addition to typecheck and build.
- Rust toolchain pinned to 1.84.0 in `rust-toolchain.toml` and CI
  workflows to insulate against upstream rustc regressions.
- Dependabot groups `serde`, `tracing`, and `clap` families.
- Biome devDep tightened to `^2.4.0` to match the pinned schema.

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

[1.0.0-beta.8]: https://github.com/matiaspalmac/everything-imu/releases/tag/v1.0.0-beta.8
[1.0.0-beta.7]: https://github.com/matiaspalmac/everything-imu/releases/tag/v1.0.0-beta.7
[1.0.0-beta.6]: https://github.com/matiaspalmac/everything-imu/releases/tag/v1.0.0-beta.6
[1.0.0-beta.1]: https://github.com/matiaspalmac/everything-imu/releases/tag/v1.0.0-beta.1
[1.0.0-beta.0]: https://github.com/matiaspalmac/everything-imu/releases/tag/v1.0.0-beta.0
