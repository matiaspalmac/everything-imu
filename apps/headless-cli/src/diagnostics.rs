//! Hardware bring-up and reverse-engineering subcommands (`--*-raw`,
//! `--hopx-probe`, `--ps-pair`, `--list-devices`). Each runs one diagnostic
//! and exits; none of them stream to SlimeVR.

use device_dualsense::DualSenseFactory;
use device_joycon::JoyconFactory;
use device_psmove::PsMoveFactory;
use device_steam_controller::SteamControllerFactory;
use device_steam_deck::SteamDeckFactory;

pub async fn run_ds_raw() -> anyhow::Result<()> {
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

pub async fn run_psmove_raw() -> anyhow::Result<()> {
    eprintln!("everything-imu psmove-raw — PS Move (ZCM1/ZCM2) raw report dump");
    eprintln!("Steps:");
    eprintln!("  1. Pair the PS Move over Bluetooth (IMU only streams over BT).");
    eprintln!("  2. HOLD STILL on a flat surface for ~5 s.");
    eprintln!("  3. Then rotate around each axis; watch which component spikes.");
    eprintln!();

    let handle = tokio::task::spawn_blocking(|| device_psmove::diagnostics::run(None));

    tokio::select! {
        r = handle => {
            match r {
                Ok(Ok(())) => {}
                Ok(Err(e)) => eprintln!("[psmove-raw] error: {e}"),
                Err(e) => eprintln!("[psmove-raw] task join error: {e}"),
            }
        }
        _ = tokio::signal::ctrl_c() => {
            println!("\n[psmove-raw] stopped.");
        }
    }
    Ok(())
}

/// Pair the first USB-tethered PS Move to `mac` and report the device id.
pub fn run_ps_pair(mac: &str) -> anyhow::Result<()> {
    let host_mac = device_psmove::pairing::parse_mac_str(mac).map_err(|e| anyhow::anyhow!(e))?;
    let id = device_psmove::PsMoveFactory::new()
        .pair(host_mac)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("paired {id} to host {mac}");
    Ok(())
}

pub async fn run_wii_raw(bind: &str) -> anyhow::Result<()> {
    eprintln!("everything-imu wii-raw — Wii forwarder packet dump");
    eprintln!("Steps:");
    eprintln!("  1. Launch the eimu-wii homebrew on the Wii (companions/wii).");
    eprintln!("  2. Point it at this PC's IP:{bind} (config.txt server_ip/port).");
    eprintln!("  3. Hold the remote still ~5 s, then rotate about each axis.");
    eprintln!("  4. Press Ctrl-C and send the output back.");
    eprintln!();

    tokio::select! {
        r = device_wii::diagnostics::run(bind, None) => {
            if let Err(e) = r {
                eprintln!("[wii-raw] error: {e}");
                std::process::exit(1);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            println!("\n[wii-raw] stopped.");
        }
    }
    Ok(())
}

pub async fn run_hopx_raw() -> anyhow::Result<()> {
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

pub async fn run_hopx_probe(spec: &str, repeats: u8) -> anyhow::Result<()> {
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

/// One-shot scan: print every paired/nearby controller hidapi or BLE can see.
pub async fn run_list_devices() -> anyhow::Result<()> {
    // Keep each enumerator independent: a failure in one transport (notably
    // the BLE JC2 scan) must not abort the whole listing — warn and move on.
    let nintendo = JoyconFactory::list_paired().unwrap_or_else(|e| {
        eprintln!("[warn] Nintendo HID scan unavailable: {e}");
        Vec::new()
    });
    let jc2_nearby = JoyconFactory::list_nearby_jc2(1200)
        .await
        .unwrap_or_else(|e| {
            eprintln!("[warn] BLE scan unavailable: {e}");
            Vec::new()
        });
    let sony_pads = DualSenseFactory::list_paired().unwrap_or_else(|e| {
        eprintln!("[warn] Sony pad scan unavailable: {e}");
        Vec::new()
    });
    let sony_moves = PsMoveFactory::list_paired().unwrap_or_else(|e| {
        eprintln!("[warn] PS Move scan unavailable: {e}");
        Vec::new()
    });
    let steam_decks = SteamDeckFactory::list_paired().unwrap_or_else(|e| {
        eprintln!("[warn] Steam Deck scan unavailable: {e}");
        Vec::new()
    });
    let steam_ctrls = SteamControllerFactory::list_paired().unwrap_or_else(|e| {
        eprintln!("[warn] Steam Controller scan unavailable: {e}");
        Vec::new()
    });
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
    Ok(())
}
