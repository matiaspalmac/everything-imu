//! Command-line interface definition.

use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "everything-imu-cli",
    version,
    about = "Headless SlimeVR IMU bridge"
)]
pub struct Cli {
    #[arg(long, default_value = "127.0.0.1:6969")]
    pub server: SocketAddr,

    /// Spawn N synthetic Joy-Cons (no real-device enumeration in this mode).
    #[arg(long)]
    pub synthetic: Option<u8>,

    /// Spawn N synthetic DualSense controllers alongside the JC synth pool.
    /// Implies --synthetic if --synthetic is unset (and JC count = 0).
    #[arg(long, default_value_t = 0)]
    pub synth_ds: u8,

    /// Spawn N synthetic PS Move controllers alongside the JC synth pool.
    #[arg(long, default_value_t = 0)]
    pub synth_move: u8,

    /// Spawn N synthetic Steam Decks alongside the other synth factories.
    #[arg(long, default_value_t = 0)]
    pub synth_deck: u8,

    /// Spawn N synthetic Steam Controllers alongside the other synth factories.
    #[arg(long, default_value_t = 0)]
    pub synth_steam_ctrl: u8,

    /// Run a synthetic Tesla tracker (figure-eight drive trace) alongside the
    /// other factories. Useful for end-to-end smoke tests without a vehicle.
    #[arg(long, default_value_t = false)]
    pub synth_tesla: bool,

    /// Connect to a real Tesla vehicle via the Fleet API. Requires
    /// `TESLA_REFRESH_TOKEN`, `TESLA_CLIENT_ID`, and `TESLA_VEHICLE_ID`
    /// environment variables. Optional `TESLA_REGION=eu|cn|na`.
    #[arg(long, default_value_t = false)]
    pub tesla: bool,

    /// Optional path to SQLite state DB. When omitted, uses in-memory stores
    /// (no persistence across restarts — useful for synthetic smoke runs).
    #[arg(long)]
    pub db: Option<PathBuf>,

    #[arg(long, default_value = "info")]
    pub log: String,

    /// One-shot scan: list paired Joy-Con / Pro Controller HID devices and exit.
    /// Useful to verify hidapi sees the controller before launching tracking.
    #[arg(long)]
    pub list_devices: bool,

    /// Run a preflight diagnostic and print a report. Use this when sending
    /// bug reports — paste the full output. Exits 0 on full pass, 1 if any
    /// check failed.
    #[arg(long)]
    pub doctor: bool,

    /// Hardware characterisation: connect to a "Triki" HOPX tracker and stream
    /// its raw int16 IMU channels (no scaling, no SlimeVR). Use to measure scale,
    /// axis order, and sample rate. Redirect to a file and send the output.
    #[arg(long)]
    pub hopx_raw: bool,
    /// Live PS Move (ZCM1/ZCM2) raw input-report dump for IMU bring-up.
    #[arg(long)]
    pub psmove_raw: bool,

    /// Reverse-engineering aid: connect to a "Triki" HOPX tracker and write each
    /// command byte to its NUS RX characteristic, logging what it sends back.
    /// Optional value is a command spec (default "0x00-0x1f"): a comma list of
    /// single bytes and inclusive `lo-hi` ranges, hex (`0x..`) or decimal —
    /// e.g. `--hopx-probe 0x09,0x0a` or `--hopx-probe 0-255`.
    #[arg(long, value_name = "SPEC", num_args = 0..=1, default_missing_value = "0x00-0x1f")]
    pub hopx_probe: Option<String>,

    /// How many times to send each probed command (--hopx-probe). Repeats reveal
    /// whether a reply is stable config or live-changing data.
    #[arg(long, default_value_t = 3)]
    pub hopx_probe_repeats: u8,

    /// Hardware-characterisation: open the first paired Sony pad (DualSense /
    /// DualShock 4) and dump its undecoded input report — raw gyro/accel int16,
    /// report id/length, and the firmware sensor-timestamp field with its
    /// per-report delta. Use to confirm the timestamp tick scale, axis order,
    /// and report rate. Redirect to a file and send the output.
    #[arg(long)]
    pub ds_raw: bool,

    /// Bring-up: bind the Wii forwarder TCP listener and dump decoded packets
    /// from the homebrew companion (raw + scaled accel/gyro, extension flags).
    /// Does not stream to SlimeVR. Defaults to 127.0.0.1:9909.
    #[arg(long)]
    pub wii_raw: bool,

    /// Address the Wii forwarder listens on for `--wii-raw` and live tracking.
    #[arg(long, default_value = "127.0.0.1:9909")]
    pub wii_bind: String,

    /// UDP address the 3DS homebrew forwarder listens on for live tracking.
    #[arg(long, default_value = "0.0.0.0:9305")]
    pub three_ds_bind: String,

    /// UDP address the PS Vita homebrew forwarder listens on for live tracking.
    #[arg(long, default_value = "0.0.0.0:9306")]
    pub vita_bind: String,

    /// UDP address the eimu remote-hub (phone) listener binds to.
    #[arg(long, default_value = "0.0.0.0:9320")]
    pub remote_bind: String,

    /// Pair the first USB-connected PS Move to a host Bluetooth MAC
    /// (AA:BB:CC:DD:EE:FF) via feature report 0x05, then exit.
    #[arg(long, value_name = "MAC")]
    pub ps_pair: Option<String>,
}
