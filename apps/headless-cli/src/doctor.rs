//! Preflight diagnostic (`--doctor`): hidapi health, paired-device visibility,
//! and a UDP reachability probe against the configured SlimeVR server.

use device_dualsense::DualSenseFactory;
use device_joycon::JoyconFactory;
use device_psmove::PsMoveFactory;
use device_steam_controller::SteamControllerFactory;
use device_steam_deck::SteamDeckFactory;
use std::net::SocketAddr;

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

pub async fn run_doctor(server: SocketAddr) -> i32 {
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
