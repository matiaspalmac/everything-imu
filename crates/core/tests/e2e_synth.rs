use device_dualsense::DualSenseFactory;
use device_joycon::JoyconFactory;
use device_psmove::PsMoveFactory;
use device_traits::{DeviceFactory, InMemoryBiasStore, InMemorySettingsStore};
use everything_imu_core::{AppState, Supervisor};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;

/// Drives `factories` against a local UDP listener for up to 3 s and
/// returns the first-byte tag of every datagram captured. Used by each
/// driver's e2e test to assert protocol-level handshake + sensor_info +
/// rotation packets land at the server endpoint.
async fn run_until_tags(factories: Vec<Arc<dyn DeviceFactory>>, deadline: Duration) -> Vec<u32> {
    let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server_addr: SocketAddr = server.local_addr().unwrap();

    let settings = Arc::new(InMemorySettingsStore::default());
    let bias = Arc::new(InMemoryBiasStore::default());
    let state = Arc::new(AppState::new(server_addr, settings, bias).await.unwrap());

    let sup = Supervisor::new(state.clone(), factories);
    let sup_task = tokio::spawn(sup.run());

    let mut buf = vec![0u8; 1500];
    let mut tags_seen: Vec<u32> = Vec::new();
    let recv_deadline = std::time::Instant::now() + deadline;
    while std::time::Instant::now() < recv_deadline {
        match tokio::time::timeout(Duration::from_millis(500), server.recv_from(&mut buf)).await {
            Ok(Ok((n, _from))) => {
                if n >= 4 {
                    let tag = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
                    tags_seen.push(tag);
                }
                if tags_seen.contains(&3)
                    && tags_seen.contains(&15)
                    && tags_seen.iter().filter(|&&t| t == 17).count() >= 5
                {
                    break;
                }
            }
            _ => continue,
        }
    }

    state.shutdown().await;
    sup_task.abort();
    tags_seen
}

fn assert_handshake_sensor_info_rotation(tags: &[u32]) {
    assert!(tags.contains(&3), "handshake (tag=3) seen");
    assert!(tags.contains(&15), "sensor_info (tag=15) seen");
    let rotation_count = tags.iter().filter(|&&t| t == 17).count();
    assert!(
        rotation_count >= 5,
        "≥5 RotationData (tag=17) seen, got {rotation_count}",
    );
}

#[ignore = "requires cooperative UDP endpoint that ACKs handshakes"]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn synthetic_jc1_emits_handshake_then_rotation_data() {
    let factories: Vec<Arc<dyn DeviceFactory>> = vec![Arc::new(JoyconFactory::synthetic(1))];
    let tags = run_until_tags(factories, Duration::from_secs(7)).await;
    assert_handshake_sensor_info_rotation(&tags);
}

#[ignore = "requires cooperative UDP endpoint that ACKs handshakes"]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn synthetic_dualsense_emits_handshake_then_rotation_data() {
    let factories: Vec<Arc<dyn DeviceFactory>> = vec![Arc::new(DualSenseFactory::synthetic(1))];
    let tags = run_until_tags(factories, Duration::from_secs(7)).await;
    assert_handshake_sensor_info_rotation(&tags);
}

#[ignore = "requires cooperative UDP endpoint that ACKs handshakes"]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn synthetic_psmove_emits_handshake_then_rotation_data() {
    let factories: Vec<Arc<dyn DeviceFactory>> = vec![Arc::new(PsMoveFactory::synthetic(1))];
    let tags = run_until_tags(factories, Duration::from_secs(7)).await;
    assert_handshake_sensor_info_rotation(&tags);
}

#[ignore = "requires cooperative UDP endpoint that ACKs handshakes"]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mixed_synthetic_pool_runs_all_drivers_concurrently() {
    let factories: Vec<Arc<dyn DeviceFactory>> = vec![
        Arc::new(JoyconFactory::synthetic(1)),
        Arc::new(DualSenseFactory::synthetic(1)),
        Arc::new(PsMoveFactory::synthetic(1)),
    ];
    let tags = run_until_tags(factories, Duration::from_secs(10)).await;
    assert!(
        tags.iter().filter(|&&t| t == 3).count() >= 3,
        "≥3 handshakes (each device has its own SlimeClient)",
    );
    let sensor_info_count = tags.iter().filter(|&&t| t == 15).count();
    assert!(
        sensor_info_count >= 3,
        "≥3 sensor_info (one per device), got {sensor_info_count}",
    );
}
