# Changelog

All notable changes to this project are documented here. The format is loosely
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
project follows [SemVer](https://semver.org/).

Unreleased changes are tracked on `main`. Tagged releases are listed in
reverse-chronological order below.

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
