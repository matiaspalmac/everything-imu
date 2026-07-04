//! `SteamDeckDevice` — `Device` trait impl for the integrated Deck controller.

use crate::lizard::{lizard_disable_sequence, WATCHDOG_INTERVAL};
use crate::report::{parse as parse_report, MIN_REPORT_LEN};
use crate::scale::{accel_m_s2, gyro_rad_s};
use device_traits::{
    ChannelInfo, Device, DeviceCapabilities, DeviceError, DeviceId, DeviceKind, DeviceMetadata,
    ImuSample,
};
use hidapi::HidDevice;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Instant;
use tokio::sync::mpsc;

pub struct SteamDeckDevice {
    metadata: DeviceMetadata,
    hid: Arc<StdMutex<HidDevice>>,
    shutdown: Option<tokio::sync::watch::Sender<bool>>,
    join: Option<std::thread::JoinHandle<()>>,
    watchdog: Option<tokio::task::JoinHandle<()>>,
    watchdog_stop: Option<tokio::sync::watch::Sender<bool>>,
    epoch: Instant,
}

impl SteamDeckDevice {
    pub fn new(device: HidDevice, serial: String, mac: [u8; 6]) -> Self {
        let id = DeviceId { mac, serial };
        let metadata = DeviceMetadata {
            id,
            kind: DeviceKind::SteamDeck,
            firmware: Some("Steam Deck (jupiter/galileo)".into()),
            capabilities: DeviceCapabilities {
                has_magnetometer: false,
                has_battery: false,
                has_rumble: true,
                native_imu_rate_hz: 250,
            },
        };
        Self {
            metadata,
            hid: Arc::new(StdMutex::new(device)),
            shutdown: None,
            join: None,
            watchdog: None,
            watchdog_stop: None,
            epoch: Instant::now(),
        }
    }

    pub fn with_kind(mut self, kind: DeviceKind) -> Self {
        self.metadata.kind = kind;
        self
    }

    fn send_lizard_disable(hid: &Arc<StdMutex<HidDevice>>) -> Result<(), DeviceError> {
        let guard = hid.lock().unwrap();
        for buf in lizard_disable_sequence() {
            guard
                .send_feature_report(&buf)
                .map_err(|e| DeviceError::Hid(e.to_string()))?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Device for SteamDeckDevice {
    fn metadata(&self) -> &DeviceMetadata {
        &self.metadata
    }

    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>, DeviceError> {
        if self.shutdown.is_some() {
            return Err(DeviceError::Hid("already started".into()));
        }

        // Step 1: kill lizard so we get raw HID reports.
        Self::send_lizard_disable(&self.hid)?;

        let (tx, rx) = mpsc::channel::<ChannelInfo>(64);
        let id = self.metadata.id.clone();
        let _ = tx.send(ChannelInfo::Connected(id)).await;

        // Step 2: lizard watchdog (tokio task, fires every WATCHDOG_INTERVAL).
        let wd_hid = self.hid.clone();
        let (wd_tx, mut wd_rx) = tokio::sync::watch::channel(false);
        let wd = tokio::spawn(async move {
            let mut interval = tokio::time::interval(WATCHDOG_INTERVAL);
            interval.tick().await; // immediate first tick already consumed by initial send
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let feed_hid = wd_hid.clone();
                        let res = tokio::task::spawn_blocking(move || {
                            Self::send_lizard_disable(&feed_hid)
                        })
                        .await;
                        match res {
                            Ok(Err(e)) => tracing::warn!(error = %e, "lizard watchdog feed failed"),
                            Err(e) => tracing::warn!(error = %e, "lizard watchdog feed task failed"),
                            Ok(Ok(())) => {}
                        }
                    }
                    changed = wd_rx.changed() => {
                        if changed.is_err() || *wd_rx.borrow() {
                            break;
                        }
                    }
                }
            }
        });
        self.watchdog = Some(wd);
        self.watchdog_stop = Some(wd_tx);

        // Step 3: blocking reader thread → mpsc.
        let hid = self.hid.clone();
        let epoch = self.epoch;
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let reader_tx = tx.clone();
        let join = std::thread::Builder::new()
            .name("steamdeck-reader".into())
            .spawn(move || {
                reader_loop(hid, reader_tx, shutdown_rx, epoch);
            })
            .map_err(|e| DeviceError::Hid(e.to_string()))?;
        self.shutdown = Some(shutdown_tx);
        self.join = Some(join);

        Ok(rx)
    }

    async fn stop(&mut self) -> Result<(), DeviceError> {
        if let Some(s) = self.shutdown.take() {
            let _ = s.send(true);
        }
        if let Some(j) = self.join.take() {
            // Reader thread observes shutdown on next read_timeout poll.
            let _ = tokio::task::spawn_blocking(move || j.join()).await;
        }
        if let Some(stop) = self.watchdog_stop.take() {
            let _ = stop.send(true);
        }
        if let Some(wd) = self.watchdog.take() {
            wd.abort();
        }
        Ok(())
    }

    async fn set_led_mask(&mut self, _mask: u8) -> Result<(), DeviceError> {
        // Deck has no per-LED control surface exposed via HID feature reports.
        Ok(())
    }

    async fn set_rumble(&mut self, intensity: f32) -> Result<(), DeviceError> {
        // Rumble is implemented but not yet wired — Valve uses a custom
        // ID_TRIGGER_RUMBLE_CMD feature report with amplitude + period fields.
        // Stubbed to keep the trait signature satisfied.
        tracing::debug!(intensity, "steam deck rumble not yet implemented");
        Ok(())
    }
}

fn reader_loop(
    hid: Arc<StdMutex<HidDevice>>,
    tx: mpsc::Sender<ChannelInfo>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
    epoch: Instant,
) {
    let mut buf = [0u8; 128];
    loop {
        if shutdown.has_changed().unwrap_or(false) && *shutdown.borrow_and_update() {
            break;
        }
        let n = {
            let guard = hid.lock().unwrap();
            match guard.read_timeout(&mut buf, 50) {
                Ok(n) => n,
                Err(e) => {
                    tracing::debug!(error = %e, "deck read error");
                    break;
                }
            }
        };
        if n < MIN_REPORT_LEN {
            continue;
        }
        let report = match parse_report(&buf[..n]) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let ts_us = epoch.elapsed().as_micros() as u64;
        let sample = ImuSample {
            gyro: [
                gyro_rad_s(report.gyro_raw[0]),
                gyro_rad_s(report.gyro_raw[1]),
                gyro_rad_s(report.gyro_raw[2]),
            ],
            accel: [
                accel_m_s2(report.accel_raw[0]),
                accel_m_s2(report.accel_raw[1]),
                accel_m_s2(report.accel_raw[2]),
            ],
            mag: None,
            timestamp_us: ts_us,
        };
        if tx
            .blocking_send(ChannelInfo::ImuSamples(vec![sample]))
            .is_err()
        {
            break;
        }
    }
    let _ = tx.blocking_send(ChannelInfo::Disconnected);
}

#[cfg(test)]
mod tests {
    use crate::ids::{is_gamepad_interface, STEAM_DECK_PID, VALVE_VID};

    #[test]
    fn deck_vid_pid_matches_ids_module() {
        assert_eq!(VALVE_VID, 0x28DE);
        assert_eq!(STEAM_DECK_PID, 0x1205);
        assert!(is_gamepad_interface(VALVE_VID, STEAM_DECK_PID, 0, 0));
    }
}
