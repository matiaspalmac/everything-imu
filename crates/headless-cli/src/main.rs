use clap::Parser;
use device_dualsense::DualSenseFactory;
use device_hopx::HopxFactory;
use device_joycon::JoyconFactory;
use device_psmove::PsMoveFactory;
use device_steam_controller::SteamControllerFactory;
use device_steam_deck::SteamDeckFactory;
use device_tesla::{TeslaConfig, TeslaFactory};
use device_traits::{
    BiasStore, DeviceFactory, InMemoryBiasStore, InMemorySettingsStore, SettingsStore,
};
use device_wii::WiiFactory;
use everything_imu_core::{AppState, Supervisor};
use persistence::{PersistenceDb, SqliteBiasStore, SqliteSettingsStore};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "everything-imu-cli",
    version,
    about = "Headless SlimeVR IMU bridge"
)]
struct Cli {
    #[arg(long, default_value = "127.0.0.1:6969")]
    server: SocketAddr,

    /// Spawn N synthetic Joy-Cons (no real-device enumeration in this mode).
    #[arg(long)]
    synthetic: Option<u8>,

    /// Spawn N synthetic DualSense controllers alongside the JC synth pool.
    /// Implies --synthetic if --synthetic is unset (and JC count = 0).
    #[arg(long, default_value_t = 0)]
    synth_ds: u8,

    /// Spawn N synthetic PS Move controllers alongside the JC synth pool.
    #[arg(long, default_value_t = 0)]
    synth_move: u8,

    /// Spawn N synthetic Steam Decks alongside the other synth factories.
    #[arg(long, default_value_t = 0)]
    synth_deck: u8,

    /// Spawn N synthetic Steam Controllers alongside the other synth factories.
    #[arg(long, default_value_t = 0)]
    synth_steam_ctrl: u8,

    /// Run a synthetic Tesla tracker (figure-eight drive trace) alongside the
    /// other factories. Useful for end-to-end smoke tests without a vehicle.
    #[arg(long, default_value_t = false)]
    synth_tesla: bool,

    /// Connect to a real Tesla vehicle via the Fleet API. Requires
    /// `TESLA_REFRESH_TOKEN`, `TESLA_CLIENT_ID`, and `TESLA_VEHICLE_ID`
    /// environment variables. Optional `TESLA_REGION=eu|cn|na`.
    #[arg(long, default_value_t = false)]
    tesla: bool,

    /// Optional path to SQLite state DB. When omitted, uses in-memory stores
    /// (no persistence across restarts — useful for synthetic smoke runs).
    #[arg(long)]
    db: Option<PathBuf>,

    #[arg(long, default_value = "info")]
    log: String,

    /// One-shot scan: list paired Joy-Con / Pro Controller HID devices and exit.
    /// Useful to verify hidapi sees the controller before launching tracking.
    #[arg(long)]
    list_devices: bool,

    /// Run a preflight diagnostic and print a report. Use this when sending
    /// bug reports — paste the full output. Exits 0 on full pass, 1 if any
    /// check failed.
    #[arg(long)]
    doctor: bool,

    /// Hardware characterisation: connect to a "Triki" HOPX tracker and stream
    /// its raw int16 IMU channels (no scaling, no SlimeVR). Use to measure scale,
    /// axis order, and sample rate. Redirect to a file and send the output.
    #[arg(long)]
    hopx_raw: bool,

    /// Reverse-engineering aid: connect to a "Triki" HOPX tracker and write each
    /// command byte to its NUS RX characteristic, logging what it sends back.
    /// Optional value is a command spec (default "0x00-0x1f"): a comma list of
    /// single bytes and inclusive `lo-hi` ranges, hex (`0x..`) or decimal —
    /// e.g. `--hopx-probe 0x09,0x0a` or `--hopx-probe 0-255`.
    #[arg(long, value_name = "SPEC", num_args = 0..=1, default_missing_value = "0x00-0x1f")]
    hopx_probe: Option<String>,

    /// How many times to send each probed command (--hopx-probe). Repeats reveal
    /// whether a reply is stable config or live-changing data.
    #[arg(long, default_value_t = 3)]
    hopx_probe_repeats: u8,

    /// Hardware-characterisation: open the first paired Sony pad (DualSense /
    /// DualShock 4) and dump its undecoded input report — raw gyro/accel int16,
    /// report id/length, and the firmware sensor-timestamp field with its
    /// per-report delta. Use to confirm the timestamp tick scale, axis order,
    /// and report rate. Redirect to a file and send the output.
    #[arg(long)]
    ds_raw: bool,
}

async fn run_ds_raw() -> anyhow::Result<()> {
    eprintln!("everything-imu ds-raw — Sony pad raw report + sensor-timestamp dump");
    eprintln!("Steps:");
    eprintln!("  1. Keep the DualSense connected (USB cable or BT), then keep this running.");
    eprintln!("  2. HOLD STILL on a flat surface for ~5 s (watch ts_delta settle).");
    eprintln!("  3. Rotate exactly 90 deg about ONE axis, slowly, then back.");
    eprintln!("  4. Tilt nose-down ~45 deg, then roll left ~45 deg.");
    eprintln!("  5. Press Ctrl-C and send the whole output back.");
    eprintln!("Cols: id len  gyro[x y z] | accel[x y z]  ts=<u32>  dts=<delta/report>  ~rate");
    eprintln!();

    // The DualSense read loop is blocking; run it on a dedicated thread and
    // print every 25th report (~10 rows/s at 250 Hz) so the stream stays
    // readable while the per-report ts_delta is still shown verbatim.
    let handle = tokio::task::spawn_blocking(|| {
        let mut seen: u64 = 0;
        device_dualsense::diagnostics::stream_raw(|s| {
            seen += 1;
            if seen % 25 != 0 {
                return;
            }
            let ts = s
                .sensor_timestamp
                .map(|v| v.to_string())
                .unwrap_or_else(|| "----".into());
            let dts = s
                .ts_delta
                .map(|v| v.to_string())
                .unwrap_or_else(|| "----".into());
            let off = s
                .ts_offset
                .map(|o| o.to_string())
                .unwrap_or_else(|| "?".into());
            println!(
                "id=0x{:02x} len={:3}  g[{:6} {:6} {:6}] | a[{:6} {:6} {:6}]  ts={:>10}@{} dts={:>6}  ~{:.1}Hz",
                s.report_id, s.len,
                s.gyro[0], s.gyro[1], s.gyro[2],
                s.accel[0], s.accel[1], s.accel[2],
                ts, off, dts, s.rate_hz,
            );
        })
    });

    tokio::select! {
        r = handle => {
            match r {
                Ok(Ok(())) => {}
                Ok(Err(e)) => eprintln!("[ds-raw] error: {e}"),
                Err(e) => eprintln!("[ds-raw] task join error: {e}"),
            }
        }
        _ = tokio::signal::ctrl_c() => {
            println!("\n[ds-raw] stopped.");
        }
    }
    Ok(())
}

async fn run_hopx_raw() -> anyhow::Result<()> {
    // Guidance on stderr so it stays visible when stdout is redirected to a
    // file; the data rows below go to stdout (capture those).
    eprintln!("everything-imu hopx-raw — raw IMU channel dump");
    eprintln!("Steps:");
    eprintln!("  1. Power on the Triki tracker, then keep this running.");
    eprintln!("  2. HOLD STILL on a flat surface for ~5 s.");
    eprintln!("  3. Rotate exactly 90 deg about ONE axis, slowly, then back.");
    eprintln!("  4. Tilt nose-down ~45 deg, then roll left ~45 deg.");
    eprintln!("  5. Press Ctrl-C and send the whole output back.");
    eprintln!("Columns: seq  ch0 ch1 ch2 | ch3 ch4 ch5  (raw int16, wire order)  rate");
    eprintln!();

    let res = tokio::select! {
        r = device_hopx::diagnostics::stream_raw(|s| {
            println!(
                "seq={:3}  {:7} {:7} {:7} | {:7} {:7} {:7}  ~{:.1}Hz",
                s.seq,
                s.channels[0], s.channels[1], s.channels[2],
                s.channels[3], s.channels[4], s.channels[5],
                s.rate_hz,
            );
        }) => r,
        _ = tokio::signal::ctrl_c() => {
            println!("\n[hopx-raw] stopped.");
            Ok(())
        }
    };
    if let Err(e) = res {
        eprintln!("[hopx-raw] error: {e}");
        std::process::exit(1);
    }
    Ok(())
}

async fn run_hopx_probe(spec: &str, repeats: u8) -> anyhow::Result<()> {
    let cmds = match device_hopx::diagnostics::parse_probe_spec(spec) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[hopx-probe] bad command spec: {e}");
            std::process::exit(2);
        }
    };

    eprintln!("everything-imu hopx-probe — NUS command sweep");
    eprintln!("Steps:");
    eprintln!("  1. Power on the Triki tracker, then keep this running.");
    eprintln!("  2. Each command is written {repeats}x; replies are logged below.");
    eprintln!("  3. Known: 0x09 returns ~3 varying messages, 0x0a disconnects.");
    eprintln!("  4. Let it finish and send the whole output back.");
    eprintln!(
        "Probing {} command(s): {}",
        cmds.len(),
        cmds.iter()
            .map(|c| format!("0x{c:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    eprintln!();

    // Per ~200 ms dwell, long enough to catch a few-message reply without
    // dragging a 256-command sweep out forever.
    let dwell = std::time::Duration::from_millis(250);
    let res = tokio::select! {
        r = device_hopx::diagnostics::probe_commands(&cmds, repeats, dwell, |p| {
            if p.responses.is_empty() {
                println!(
                    "cmd=0x{:02x} #{}  {}",
                    p.cmd,
                    p.attempt,
                    if p.disconnected { "DISCONNECTED (no reply)" } else { "no reply" }
                );
            } else {
                let msgs = p
                    .responses
                    .iter()
                    .map(|r| r.iter().map(|b| format!("{b:02x}")).collect::<String>())
                    .collect::<Vec<_>>()
                    .join(" | ");
                println!(
                    "cmd=0x{:02x} #{}  {} msg: {}{}",
                    p.cmd,
                    p.attempt,
                    p.responses.len(),
                    msgs,
                    if p.disconnected { "  [then DISCONNECTED]" } else { "" }
                );
            }
        }) => r,
        _ = tokio::signal::ctrl_c() => {
            println!("\n[hopx-probe] stopped.");
            Ok(())
        }
    };
    if let Err(e) = res {
        eprintln!("[hopx-probe] error: {e}");
        std::process::exit(1);
    }
    Ok(())
}

#[derive(Debug)]
enum CheckOutcome {
    Pass(String),
    Warn(String),
    Fail(String),
}

fn line(label: &str, outcome: &CheckOutcome) {
    let (mark, body) = match outcome {
        CheckOutcome::Pass(m) => ("[ OK ]", m.as_str()),
        CheckOutcome::Warn(m) => ("[WARN]", m.as_str()),
        CheckOutcome::Fail(m) => ("[FAIL]", m.as_str()),
    };
    println!("  {mark}  {label:<32} {body}");
}

async fn run_doctor(server: SocketAddr) -> i32 {
    println!(
        "everything-imu-cli v{} doctor report",
        env!("CARGO_PKG_VERSION")
    );
    println!(
        "  Platform     : {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("  Server target: {server}");
    println!();
    println!("Checks:");

    let mut any_fail = false;

    // Single hidapi probe — `list_paired` initializes the singleton on first
    // call. Reusing the result avoids paying for a second OS-level scan
    // and surfaces any error consistently to both checks below.
    let nintendo_result = JoyconFactory::list_paired();
    let hid = match &nintendo_result {
        Ok(_) => CheckOutcome::Pass("hidapi singleton initialized".into()),
        Err(e) => {
            any_fail = true;
            CheckOutcome::Fail(format!("hidapi init failed: {e}"))
        }
    };
    line("hidapi", &hid);

    let nintendo = nintendo_result.unwrap_or_default();
    let jc2_nearby = JoyconFactory::list_nearby_jc2(1200)
        .await
        .unwrap_or_default();
    let sony_pads = DualSenseFactory::list_paired().unwrap_or_default();
    let sony_moves = PsMoveFactory::list_paired().unwrap_or_default();
    let steam_decks = SteamDeckFactory::list_paired().unwrap_or_default();
    let steam_ctrls = SteamControllerFactory::list_paired().unwrap_or_default();
    let total = nintendo.len()
        + jc2_nearby.len()
        + sony_pads.len()
        + sony_moves.len()
        + steam_decks.len()
        + steam_ctrls.len();
    let dev = if total == 0 {
        CheckOutcome::Warn("no paired controllers visible (Bluetooth pairing?)".into())
    } else {
        CheckOutcome::Pass(format!(
            "{} controller(s) visible (jc1-hid={}, jc2-ble={}, sony-pad={}, ps-move={}, steam-deck={}, steam-ctrl={})",
            total,
            nintendo.len(),
            jc2_nearby.len(),
            sony_pads.len(),
            sony_moves.len(),
            steam_decks.len(),
            steam_ctrls.len(),
        ))
    };
    line("paired devices", &dev);

    // UDP send-and-receive probe. We can't force the server to reply, so
    // we send a minimal datagram and treat any IO error as fail.
    let probe = match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
        Ok(sock) => match sock.connect(server).await {
            Ok(_) => match sock.send(b"\x00\x00\x00\x03").await {
                Ok(_) => CheckOutcome::Pass(format!("UDP send to {server} accepted")),
                Err(e) => {
                    any_fail = true;
                    CheckOutcome::Fail(format!("UDP send failed: {e}"))
                }
            },
            Err(e) => {
                any_fail = true;
                CheckOutcome::Fail(format!("UDP connect failed: {e}"))
            }
        },
        Err(e) => {
            any_fail = true;
            CheckOutcome::Fail(format!("UDP bind failed: {e}"))
        }
    };
    line("UDP socket", &probe);

    println!();
    if any_fail {
        println!("Result: FAIL — see above. Run with --log debug for more detail.");
        1
    } else {
        println!("Result: OK — bridge can reach hidapi and the configured server addr.");
        println!("Note: this does NOT prove SlimeVR-Server is actually listening; we");
        println!("can't tell unless it sends back a FEATURE_FLAGS reply during a real run.");
        0
    }
}

fn print_banner(server: SocketAddr) {
    eprintln!("everything-imu-cli v{}", env!("CARGO_PKG_VERSION"));
    eprintln!("  SlimeVR-Server target : {server}");
    eprintln!("  Prerequisites:");
    eprintln!("    1. SlimeVR-Server running and listening on UDP {server}");
    eprintln!("    2. Joy-Con or DualSense / DualShock 4 paired over Bluetooth or USB");
    eprintln!("    3. Move the Joy-Con after handshake to confirm tracker rotates");
    eprintln!("  Press Ctrl+C to shut down (sends RESET_FULL + persists bias).");
    eprintln!();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(&args.log))
        .with_target(true)
        .init();

    if args.doctor {
        std::process::exit(run_doctor(args.server).await);
    }

    if args.ds_raw {
        return run_ds_raw().await;
    }

    if args.hopx_raw {
        return run_hopx_raw().await;
    }

    if let Some(spec) = args.hopx_probe.as_deref() {
        return run_hopx_probe(spec, args.hopx_probe_repeats).await;
    }

    if args.list_devices {
        let nintendo = JoyconFactory::list_paired()?;
        let jc2_nearby = JoyconFactory::list_nearby_jc2(1200).await?;
        let sony_pads = DualSenseFactory::list_paired()?;
        let sony_moves = PsMoveFactory::list_paired()?;
        let steam_decks = SteamDeckFactory::list_paired()?;
        let steam_ctrls = SteamControllerFactory::list_paired()?;
        if nintendo.is_empty()
            && jc2_nearby.is_empty()
            && sony_pads.is_empty()
            && sony_moves.is_empty()
            && steam_decks.is_empty()
            && steam_ctrls.is_empty()
        {
            println!("No paired Nintendo or Sony controllers visible to hidapi.");
            println!("Check Bluetooth pairing and that no other process holds the device.");
            return Ok(());
        }
        if !nintendo.is_empty() {
            println!("Paired Nintendo HID devices ({}):", nintendo.len());
            for (idx, d) in nintendo.iter().enumerate() {
                println!(
                    "  [{idx}] {:?}  pid=0x{:04X}  iface={}  serial={}  mac={:02X?}",
                    d.kind, d.pid, d.interface, d.serial, d.mac
                );
            }
        }
        if !jc2_nearby.is_empty() {
            println!("Nearby Joy-Con 2 BLE devices ({}):", jc2_nearby.len());
            for (idx, d) in jc2_nearby.iter().enumerate() {
                println!(
                    "  [{idx}] {:?}  addr={}  name={}  mac={:02X?}",
                    d.kind, d.address, d.name, d.mac
                );
            }
        }
        if !sony_pads.is_empty() {
            println!("Paired Sony pads ({}):", sony_pads.len());
            for (idx, d) in sony_pads.iter().enumerate() {
                println!(
                    "  [{idx}] {:?}  pid=0x{:04X}  iface={}  serial={}  mac={:02X?}",
                    d.kind, d.pid, d.interface, d.serial, d.mac
                );
            }
        }
        if !sony_moves.is_empty() {
            println!("Paired PS Move ({}):", sony_moves.len());
            for (idx, d) in sony_moves.iter().enumerate() {
                println!(
                    "  [{idx}] {:?}  pid=0x{:04X}  iface={}  serial={}  mac={:02X?}",
                    d.kind, d.pid, d.interface, d.serial, d.mac
                );
            }
        }
        if !steam_decks.is_empty() {
            println!("Paired Steam Decks ({}):", steam_decks.len());
            for (idx, d) in steam_decks.iter().enumerate() {
                println!(
                    "  [{idx}] serial={}  path={}  mac={:02X?}",
                    d.serial, d.path, d.mac
                );
            }
        }
        if !steam_ctrls.is_empty() {
            println!("Paired Steam Controllers ({}):", steam_ctrls.len());
            for (idx, d) in steam_ctrls.iter().enumerate() {
                println!(
                    "  [{idx}] {:?}  pid=0x{:04X}  serial={}  mac={:02X?}",
                    d.transport, d.pid, d.serial, d.mac
                );
            }
        }
        return Ok(());
    }

    print_banner(args.server);

    let (settings, bias_store): (Arc<dyn SettingsStore>, Arc<dyn BiasStore>) = match &args.db {
        Some(path) => {
            let db = Arc::new(PersistenceDb::open(path)?);
            tracing::info!(?path, "persistence: SQLite");
            (
                Arc::new(SqliteSettingsStore::new(db.clone())),
                Arc::new(SqliteBiasStore::new(db)),
            )
        }
        None => {
            tracing::info!("persistence: in-memory");
            (
                Arc::new(InMemorySettingsStore::default()),
                Arc::new(InMemoryBiasStore::default()),
            )
        }
    };

    let state = Arc::new(AppState::new(args.server, settings, bias_store).await?);

    let any_synth = args.synthetic.is_some()
        || args.synth_ds > 0
        || args.synth_move > 0
        || args.synth_deck > 0
        || args.synth_steam_ctrl > 0
        || args.synth_tesla;
    let mut factories: Vec<Arc<dyn DeviceFactory>> = if any_synth {
        let jc_count = args.synthetic.unwrap_or(0);
        let mut v: Vec<Arc<dyn DeviceFactory>> = Vec::new();
        if jc_count > 0 {
            v.push(Arc::new(JoyconFactory::synthetic(jc_count)));
        }
        if args.synth_ds > 0 {
            v.push(Arc::new(DualSenseFactory::synthetic(args.synth_ds)));
        }
        if args.synth_move > 0 {
            v.push(Arc::new(PsMoveFactory::synthetic(args.synth_move)));
        }
        if args.synth_deck > 0 {
            v.push(Arc::new(SteamDeckFactory::synthetic(args.synth_deck)));
        }
        if args.synth_steam_ctrl > 0 {
            v.push(Arc::new(SteamControllerFactory::synthetic(
                args.synth_steam_ctrl,
            )));
        }
        if args.synth_tesla {
            v.push(Arc::new(TeslaFactory::synthetic()));
        }
        v
    } else {
        vec![
            Arc::new(JoyconFactory::real()),
            Arc::new(DualSenseFactory::new()),
            Arc::new(PsMoveFactory::new()),
            Arc::new(WiiFactory::new()),
            Arc::new(SteamDeckFactory::new()),
            Arc::new(SteamControllerFactory::new()),
            Arc::new(HopxFactory::new()),
        ]
    };
    if args.tesla {
        match TeslaConfig::from_env() {
            Some(cfg) => {
                tracing::info!("tesla bridge: live Fleet API mode enabled");
                factories.push(Arc::new(TeslaFactory::new(cfg)));
            }
            None => {
                tracing::warn!(
                    "tesla bridge requested but TESLA_REFRESH_TOKEN / TESLA_CLIENT_ID / TESLA_VEHICLE_ID not all set; skipping"
                );
            }
        }
    }
    let sup = Supervisor::new(state.clone(), factories);

    let state_for_shutdown = state.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            tracing::info!("shutdown requested");
            state_for_shutdown.shutdown().await;
            std::process::exit(0);
        }
    });

    tracing::info!(
        "everything-imu-cli starting; SlimeVR-Server={}",
        args.server
    );
    sup.run().await?;
    Ok(())
}
