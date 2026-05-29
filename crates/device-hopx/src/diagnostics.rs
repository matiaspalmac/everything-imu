//! Hardware-characterisation helper: stream the tracker's undecoded int16 IMU
//! channels so scale, axis order, and sample rate can be measured empirically
//! (the configured full-scale range / ODR cannot be read over BLE).
//!
//! Self-contained BLE path (does not go through the `Device`/pipeline machinery)
//! so it can run as a one-shot CLI diagnostic.

use std::time::{Duration, Instant};

use btleplug::api::{
    Central, CharPropFlags, Characteristic, ConnectionParameterPreset, Manager as _,
    Peripheral as _, ScanFilter, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures_util::StreamExt;

use crate::protocol::{self, RecordParser};

/// One undecoded record plus the running average record rate.
pub struct RawSample {
    pub seq: u8,
    /// Six int16 channels in wire order (offsets 2,4,6,8,10,12) — no scale, no
    /// axis mapping.
    pub channels: [i16; 6],
    /// Average records/second since streaming started (reveals the IMU ODR).
    pub rate_hz: f32,
}

/// Scan for a `Triki` tracker, connect, and stream [`RawSample`]s to `sink`
/// until the notification stream ends (e.g. the caller is interrupted or the
/// link drops).
pub async fn stream_raw<F>(mut sink: F) -> Result<(), String>
where
    F: FnMut(RawSample),
{
    let manager = Manager::new()
        .await
        .map_err(|e| format!("manager init: {e}"))?;
    let adapter = manager
        .adapters()
        .await
        .map_err(|e| format!("adapters query: {e}"))?
        .into_iter()
        .next()
        .ok_or("no BLE adapter found")?;
    adapter
        .start_scan(ScanFilter::default())
        .await
        .map_err(|e| format!("start scan: {e}"))?;

    let peripheral = find_triki(&adapter).await?;
    let name = peripheral
        .properties()
        .await
        .ok()
        .flatten()
        .and_then(|p| p.local_name)
        .unwrap_or_default();
    eprintln!(
        "[hopx-raw] found \"{name}\" ({}), connecting...",
        peripheral.address()
    );

    peripheral
        .connect_with_timeout(Duration::from_secs(20))
        .await
        .map_err(|e| format!("connect: {e}"))?;
    peripheral
        .discover_services_with_timeout(Duration::from_secs(20))
        .await
        .map_err(|e| format!("discover services: {e}"))?;
    let _ = peripheral
        .request_connection_parameters(ConnectionParameterPreset::ThroughputOptimized)
        .await;

    let tx =
        find_char(&peripheral, protocol::NUS_TX_UUID).ok_or("NUS TX characteristic not found")?;
    let rx =
        find_char(&peripheral, protocol::NUS_RX_UUID).ok_or("NUS RX characteristic not found")?;
    peripheral
        .subscribe(&tx)
        .await
        .map_err(|e| format!("subscribe: {e}"))?;
    let write_type = if rx
        .properties
        .contains(CharPropFlags::WRITE_WITHOUT_RESPONSE)
    {
        WriteType::WithoutResponse
    } else {
        WriteType::WithResponse
    };
    peripheral
        .write(&rx, &protocol::START_CMD, write_type)
        .await
        .map_err(|e| format!("start command: {e}"))?;
    eprintln!("[hopx-raw] streaming. Ctrl-C to stop.");

    let mut notifications = peripheral
        .notifications()
        .await
        .map_err(|e| format!("notifications stream: {e}"))?;
    let mut parser = RecordParser::new();
    let mut count: u64 = 0;
    let mut first: Option<Instant> = None;
    while let Some(n) = notifications.next().await {
        if n.uuid != protocol::NUS_TX_UUID {
            continue;
        }
        for r in parser.feed_raw(&n.value) {
            let now = Instant::now();
            let started = *first.get_or_insert(now);
            count += 1;
            let secs = now.duration_since(started).as_secs_f32().max(1e-3);
            sink(RawSample {
                seq: r.seq,
                channels: r.channels,
                rate_hz: count as f32 / secs,
            });
        }
    }
    let _ = peripheral.write(&rx, &protocol::STOP_CMD, write_type).await;
    let _ = peripheral.disconnect().await;
    Ok(())
}

async fn find_triki(adapter: &Adapter) -> Result<Peripheral, String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if Instant::now() >= deadline {
            return Err("no \"Triki\" device found within 20s — is it powered on?".into());
        }
        let peripherals = adapter
            .peripherals()
            .await
            .map_err(|e| format!("peripherals query: {e}"))?;
        for p in peripherals {
            if let Ok(Some(props)) = p.properties().await {
                if props
                    .local_name
                    .as_deref()
                    .is_some_and(protocol::name_matches)
                {
                    return Ok(p);
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn find_char(peripheral: &Peripheral, uuid: uuid::Uuid) -> Option<Characteristic> {
    peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == uuid)
}
