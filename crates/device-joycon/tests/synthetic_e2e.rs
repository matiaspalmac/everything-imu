#![cfg(feature = "synthetic-source")]

use device_joycon::synthetic::SyntheticJoyConL;
use device_traits::{ChannelInfo, Device};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn synthetic_emits_packets_at_rate() {
    let mut dev = SyntheticJoyConL::new(0);
    let mut rx = dev.start().await.unwrap();

    let start = std::time::Instant::now();
    let mut packets = 0;
    let mut imu_samples = 0;
    while start.elapsed() < Duration::from_secs(1) {
        match tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
            Ok(Some(ChannelInfo::ImuSamples(s))) => {
                packets += 1;
                imu_samples += s.len();
            }
            Ok(Some(_)) => {}
            _ => {}
        }
    }
    dev.stop().await.unwrap();
    assert!(packets >= 50, "expected ~67 packets/s; got {packets}");
    assert!(
        imu_samples >= 150,
        "expected ≥150 samples/s; got {imu_samples}"
    );
}
