//! End-to-end loopback tests for [`SlimeClient`]'s BUNDLE auto-fallback gating.
//!
//! A fake "server" task binds to `127.0.0.1:0`, records inbound datagrams, and
//! optionally sends a FEATURE_FLAGS reply with `PROTOCOL_BUNDLE_SUPPORT` set.
//! The client's behavior is then verified against the recorded packets.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use slime_tracker::client::{HandshakeInfo, SlimeClient};
use slime_tracker::*;
use tokio::net::UdpSocket;

fn handshake_info() -> HandshakeInfo {
    HandshakeInfo {
        board: BoardType::Custom,
        imu: ImuType::Bmi270,
        mcu: McuType::Esp32,
        mag_status: 0,
        firmware: "loopback-test".to_string(),
        mac_address: [0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE],
    }
}

fn parse_tag(payload: &[u8]) -> Option<u32> {
    if payload.len() < 4 {
        return None;
    }
    Some(u32::from_be_bytes([
        payload[0], payload[1], payload[2], payload[3],
    ]))
}

async fn wait_for<F: Fn() -> bool>(check: F, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if check() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    check()
}

/// Modern server path: server replies to handshake with FEATURE_FLAGS bit 0
/// set. Client's `send_rotation_and_accel` should emit a single BUNDLE
/// datagram (tag 100) with rotation + accel inners.
#[tokio::test]
async fn bundle_used_when_server_advertises_support() {
    let server = UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("bind server socket");
    let server_addr = server.local_addr().unwrap();

    type RecordedPackets = Arc<Mutex<Vec<(u32, Vec<u8>)>>>;
    let received: RecordedPackets = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server_task = tokio::spawn(async move {
        let mut buf = [0u8; 1500];
        loop {
            let Ok((n, peer)) = server.recv_from(&mut buf).await else {
                return;
            };
            let payload = buf[..n].to_vec();
            let Some(tag) = parse_tag(&payload) else {
                continue;
            };
            received_clone.lock().unwrap().push((tag, payload));

            // After handshake, advertise PROTOCOL_BUNDLE_SUPPORT.
            if tag == 3 {
                let mut reply = Vec::new();
                reply.extend_from_slice(&22u32.to_be_bytes());
                reply.extend_from_slice(&0u64.to_be_bytes());
                reply.push(0b0000_0001); // bit 0 = PROTOCOL_BUNDLE_SUPPORT
                let _ = server.send_to(&reply, peer).await;
            }
        }
    });

    let info = handshake_info();
    let client = SlimeClient::connect(server_addr, &info)
        .await
        .expect("client connects");

    // Wait for the FEATURE_FLAGS reply to land in the receive loop.
    let saw_bundle_advert = wait_for(
        || client.server_supports_bundle(),
        Duration::from_millis(500),
    )
    .await;
    assert!(
        saw_bundle_advert,
        "server_supports_bundle should be true after FEATURE_FLAGS reply"
    );

    client
        .send_rotation_and_accel(
            0,
            SlimeQuaternion {
                i: 1.0,
                j: 0.0,
                k: 0.0,
                w: 0.0,
            },
            (0.0, 0.0, 9.8),
        )
        .await
        .expect("send_rotation_and_accel");

    // Wait for the bundle datagram to land on the server side.
    let saw_bundle = wait_for(
        || {
            received
                .lock()
                .unwrap()
                .iter()
                .any(|(tag, _)| *tag == BUNDLE_TAG)
        },
        Duration::from_millis(500),
    )
    .await;
    assert!(saw_bundle, "BUNDLE (tag 100) should have been sent");

    server_task.abort();

    let recv = received.lock().unwrap();
    let bundle_pkt = recv
        .iter()
        .find(|(tag, _)| *tag == BUNDLE_TAG)
        .expect("found bundle");
    let (_seq, inners) = decode_bundle(&bundle_pkt.1).expect("bundle decodes");
    assert_eq!(inners.len(), 2, "bundle should have 2 inners");
    assert_eq!(inners[0].0, 17, "first inner is rotation_data");
    assert_eq!(inners[1].0, 4, "second inner is acceleration");

    // Crucially: client should NOT have also sent standalone rotation/accel
    // packets when bundling.
    let standalone_rotation = recv.iter().filter(|(tag, _)| *tag == 17).count();
    let standalone_accel = recv.iter().filter(|(tag, _)| *tag == 4).count();
    assert_eq!(
        standalone_rotation, 0,
        "no standalone rotation when bundled"
    );
    assert_eq!(standalone_accel, 0, "no standalone accel when bundled");
}

/// Legacy-server path: server is silent on FEATURE_FLAGS. Client's
/// `send_rotation_and_accel` must fall back to two separate sends — rotation
/// (tag 17) FIRST, then acceleration (tag 4). Accel-first produced visible
/// 1-frame jitter on legacy servers per the v0.4.1 fix.
#[tokio::test]
async fn fallback_to_two_sends_when_server_silent() {
    let server = UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("bind server socket");
    let server_addr = server.local_addr().unwrap();

    let received: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server_task = tokio::spawn(async move {
        let mut buf = [0u8; 1500];
        loop {
            let Ok((n, _peer)) = server.recv_from(&mut buf).await else {
                return;
            };
            if let Some(tag) = parse_tag(&buf[..n]) {
                received_clone.lock().unwrap().push(tag);
            }
            // Intentionally do not reply with FEATURE_FLAGS.
        }
    });

    let info = handshake_info();
    let client = SlimeClient::connect(server_addr, &info)
        .await
        .expect("client connects");

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !client.server_supports_bundle(),
        "server_supports_bundle should default to false on a silent server"
    );

    client
        .send_rotation_and_accel(
            0,
            SlimeQuaternion {
                i: 1.0,
                j: 0.0,
                k: 0.0,
                w: 0.0,
            },
            (0.0, 0.0, 9.8),
        )
        .await
        .expect("send_rotation_and_accel");

    let saw_both = wait_for(
        || {
            let r = received.lock().unwrap();
            r.contains(&17) && r.contains(&4)
        },
        Duration::from_millis(500),
    )
    .await;
    assert!(saw_both, "both rotation and accel should arrive");

    server_task.abort();

    let recv = received.lock().unwrap();
    assert!(recv.contains(&3), "handshake (tag 3) should have arrived");
    assert!(
        !recv.contains(&BUNDLE_TAG),
        "no BUNDLE should be sent when server is silent; tags={recv:?}"
    );

    // Verify rotation came BEFORE accel (v0.4.1 fix avoiding 1-frame jitter).
    let rotation_idx = recv.iter().position(|&t| t == 17).expect("rotation seen");
    let accel_idx = recv.iter().position(|&t| t == 4).expect("accel seen");
    assert!(
        rotation_idx < accel_idx,
        "rotation must precede accel in fallback path; tags={recv:?}"
    );
}
