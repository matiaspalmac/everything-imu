//! Headless SlimeVR IMU bridge. Argument surface lives in `cli`, one-shot
//! hardware tools in `diagnostics`, preflight checks in `doctor`; this file
//! only wires them up and runs the live tracking supervisor.

mod cli;
mod diagnostics;
mod doctor;

use clap::Parser;
use cli::Cli;
use device_3ds::ThreeDsFactory;
use device_dualsense::DualSenseFactory;
use device_dualshock3::DualShock3Factory;
use device_hopx::HopxFactory;
use device_joycon::JoyconFactory;
use device_psmove::PsMoveFactory;
use device_remote::RemoteFactory;
use device_steam_controller::SteamControllerFactory;
use device_steam_deck::SteamDeckFactory;
use device_tesla::{TeslaConfig, TeslaFactory};
use device_traits::{
    BiasStore, DeviceFactory, InMemoryBiasStore, InMemorySettingsStore, SettingsStore,
};
use device_vita::VitaFactory;
use device_wii::WiiFactory;
use everything_imu_core::{AppState, Supervisor};
use persistence::{PersistenceDb, SqliteBiasStore, SqliteSettingsStore};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

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
        std::process::exit(doctor::run_doctor(args.server).await);
    }

    if args.ds_raw {
        return diagnostics::run_ds_raw().await;
    }

    if args.hopx_raw {
        return diagnostics::run_hopx_raw().await;
    }

    if args.psmove_raw {
        return diagnostics::run_psmove_raw().await;
    }

    if args.wii_raw {
        return diagnostics::run_wii_raw(&args.wii_bind).await;
    }

    if let Some(mac) = args.ps_pair.as_deref() {
        return diagnostics::run_ps_pair(mac);
    }

    if let Some(spec) = args.hopx_probe.as_deref() {
        return diagnostics::run_hopx_probe(spec, args.hopx_probe_repeats).await;
    }

    if args.list_devices {
        return diagnostics::run_list_devices().await;
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
            Arc::new(WiiFactory::with_bind_addr(args.wii_bind.clone())),
            Arc::new(ThreeDsFactory::with_bind_addr(args.three_ds_bind.clone())),
            Arc::new(VitaFactory::with_bind_addr(args.vita_bind.clone())),
            Arc::new(RemoteFactory::with_bind_addr(args.remote_bind.clone())),
            Arc::new(DualShock3Factory::new()),
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
