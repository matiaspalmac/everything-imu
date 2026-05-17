use clap::Parser;
use device_dualsense::DualSenseFactory;
use device_joycon::JoyconFactory;
use device_psmove::PsMoveFactory;
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

    let hid = match JoyconFactory::list_paired() {
        Ok(_) => CheckOutcome::Pass("hidapi singleton initialized".into()),
        Err(e) => {
            any_fail = true;
            CheckOutcome::Fail(format!("hidapi init failed: {e}"))
        }
    };
    line("hidapi", &hid);

    let nintendo = JoyconFactory::list_paired().unwrap_or_default();
    let jc2_nearby = JoyconFactory::list_nearby_jc2(1200)
        .await
        .unwrap_or_default();
    let sony_pads = DualSenseFactory::list_paired().unwrap_or_default();
    let sony_moves = PsMoveFactory::list_paired().unwrap_or_default();
    let total = nintendo.len() + jc2_nearby.len() + sony_pads.len() + sony_moves.len();
    let dev = if total == 0 {
        CheckOutcome::Warn("no paired controllers visible (Bluetooth pairing?)".into())
    } else {
        CheckOutcome::Pass(format!(
            "{} controller(s) visible (jc1-hid={}, jc2-ble={}, sony-pad={}, ps-move={})",
            total,
            nintendo.len(),
            jc2_nearby.len(),
            sony_pads.len(),
            sony_moves.len(),
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

    if args.list_devices {
        let nintendo = JoyconFactory::list_paired()?;
        let jc2_nearby = JoyconFactory::list_nearby_jc2(1200).await?;
        let sony_pads = DualSenseFactory::list_paired()?;
        let sony_moves = PsMoveFactory::list_paired()?;
        if nintendo.is_empty()
            && jc2_nearby.is_empty()
            && sony_pads.is_empty()
            && sony_moves.is_empty()
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

    let any_synth = args.synthetic.is_some() || args.synth_ds > 0 || args.synth_move > 0;
    let factories: Vec<Arc<dyn DeviceFactory>> = if any_synth {
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
        v
    } else {
        vec![
            Arc::new(JoyconFactory::real()),
            Arc::new(DualSenseFactory::new()),
            Arc::new(PsMoveFactory::new()),
            Arc::new(WiiFactory::new()),
        ]
    };
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
