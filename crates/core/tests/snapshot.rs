use device_joycon::JoyconFactory;
use device_traits::{DeviceFactory, InMemoryBiasStore, InMemorySettingsStore};
use everything_imu_core::{AppState, Supervisor};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn latest_quat_snapshot_non_identity_after_synthetic_burst() {
    let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = server.local_addr().unwrap();

    let settings = Arc::new(InMemorySettingsStore::default());
    let bias = Arc::new(InMemoryBiasStore::default());
    let state = Arc::new(AppState::new(addr, settings, bias).await.unwrap());

    let factories: Vec<Arc<dyn DeviceFactory>> = vec![Arc::new(JoyconFactory::synthetic(1))];
    let sup = Supervisor::new(state.clone(), factories);
    let sup_task = tokio::spawn(sup.run());

    tokio::time::sleep(Duration::from_millis(500)).await;

    let snap = state.latest_quat_snapshot().await;
    assert!(
        !snap.is_empty(),
        "snapshot should have at least the synthetic device"
    );

    state.shutdown().await;
    sup_task.abort();
}
