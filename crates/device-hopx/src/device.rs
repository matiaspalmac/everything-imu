//! [`device_traits::Device`] implementation over the BLE Nordic UART Service.
//!
//! The constructor only stores the discovered peripheral; all GATT IO happens
//! in [`HopxDevice::start`], which connects, subscribes to the TX notification
//! characteristic, writes the start command, and spawns a reader that decodes
//! records with [`crate::protocol::RecordParser`] and forwards
//! [`device_traits::ImuSample`] bursts over an mpsc channel.

use std::time::Duration;

use btleplug::api::{
    CharPropFlags, Characteristic, ConnectionParameterPreset, Peripheral as _, WriteType,
};
use btleplug::platform::Peripheral;
use device_traits::{
    ChannelInfo, Device, DeviceCapabilities, DeviceError, DeviceId, DeviceKind, DeviceMetadata,
    ImuSample,
};
use futures_util::StreamExt;
use tokio::sync::{mpsc, watch};

use crate::protocol::{self, RecordParser};

/// Report rate (IMU ODR). Measured at a stable 52 Hz across three hardware
/// captures; fusion sizes its gyro-integration timestep from this. The earlier
/// 200 Hz assumption made rotation read ~3.8× too slow.
const DEFAULT_RATE_HZ: u16 = 52;

pub struct HopxDevice {
    metadata: DeviceMetadata,
    peripheral: Peripheral,
    rx_char: Option<Characteristic>,
    task: Option<tokio::task::JoinHandle<()>>,
    stop_tx: Option<watch::Sender<bool>>,
}

impl HopxDevice {
    pub fn new(peripheral: Peripheral, serial: String, mac: [u8; 6]) -> Self {
        Self {
            metadata: DeviceMetadata {
                id: DeviceId { mac, serial },
                kind: DeviceKind::Hopx,
                firmware: None,
                capabilities: DeviceCapabilities {
                    has_magnetometer: false,
                    has_battery: false,
                    has_rumble: false,
                    native_imu_rate_hz: DEFAULT_RATE_HZ,
                },
            },
            peripheral,
            rx_char: None,
            task: None,
            stop_tx: None,
        }
    }
}

fn find_char(peripheral: &Peripheral, uuid: uuid::Uuid) -> Option<Characteristic> {
    peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == uuid)
}

async fn ensure_connected(peripheral: &Peripheral) -> Result<(), DeviceError> {
    let connected = peripheral
        .is_connected()
        .await
        .map_err(|e| DeviceError::Hid(format!("hopx is_connected failed: {e}")))?;
    if !connected {
        // *_with_timeout cancels the underlying OS call on timeout; wrapping
        // connect() in tokio::time::timeout would leave the WinRT call dangling.
        peripheral
            .connect_with_timeout(Duration::from_secs(20))
            .await
            .map_err(|e| DeviceError::Hid(format!("hopx connect failed: {e}")))?;
    }
    peripheral
        .discover_services_with_timeout(Duration::from_secs(20))
        .await
        .map_err(|e| DeviceError::Hid(format!("hopx discover_services failed: {e}")))?;
    Ok(())
}

/// Request a short BLE connection interval so the ~200 Hz notification stream is
/// not throttled to the host's slow default. Non-fatal: unsupported stacks
/// (Windows 10, BlueZ) keep the default rate.
async fn request_fast_connection_interval(peripheral: &Peripheral) {
    if let Err(e) = peripheral
        .request_connection_parameters(ConnectionParameterPreset::ThroughputOptimized)
        .await
    {
        tracing::warn!(error = %e, "hopx fast connection interval unavailable; report rate may stay low");
    }
}

async fn write_command(
    peripheral: &Peripheral,
    rx_char: &Characteristic,
    data: &[u8],
) -> Result<(), DeviceError> {
    let write_type = if rx_char
        .properties
        .contains(CharPropFlags::WRITE_WITHOUT_RESPONSE)
    {
        WriteType::WithoutResponse
    } else {
        WriteType::WithResponse
    };
    peripheral
        .write(rx_char, data, write_type)
        .await
        .map_err(|e| DeviceError::Hid(format!("hopx command write failed: {e}")))
}

#[async_trait::async_trait]
impl Device for HopxDevice {
    fn metadata(&self) -> &DeviceMetadata {
        &self.metadata
    }

    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>, DeviceError> {
        if self.task.is_some() {
            return Err(DeviceError::Hid("hopx device already started".into()));
        }

        ensure_connected(&self.peripheral).await?;
        request_fast_connection_interval(&self.peripheral).await;

        let tx_char = find_char(&self.peripheral, protocol::NUS_TX_UUID)
            .ok_or_else(|| DeviceError::Hid("hopx NUS TX characteristic not found".into()))?;
        let rx_char = find_char(&self.peripheral, protocol::NUS_RX_UUID)
            .ok_or_else(|| DeviceError::Hid("hopx NUS RX characteristic not found".into()))?;
        if !tx_char.properties.contains(CharPropFlags::NOTIFY) {
            return Err(DeviceError::Hid(
                "hopx TX characteristic lacks notify property".into(),
            ));
        }
        self.rx_char = Some(rx_char.clone());

        self.peripheral
            .subscribe(&tx_char)
            .await
            .map_err(|e| DeviceError::Hid(format!("hopx subscribe failed: {e}")))?;
        write_command(&self.peripheral, &rx_char, &protocol::START_CMD).await?;

        let mut notifications = self
            .peripheral
            .notifications()
            .await
            .map_err(|e| DeviceError::Hid(format!("hopx notifications stream failed: {e}")))?;

        let (tx, rx) = mpsc::channel(128);
        let id = self.metadata.id.clone();
        let (stop_tx, mut stop_rx) = watch::channel(false);
        self.stop_tx = Some(stop_tx);

        self.task = Some(tokio::spawn(async move {
            let _ = tx.send(ChannelInfo::Connected(id)).await;
            let mut parser = RecordParser::new();
            loop {
                tokio::select! {
                    _ = stop_rx.changed() => break,
                    maybe = notifications.next() => {
                        let Some(n) = maybe else {
                            let _ = tx.send(ChannelInfo::Disconnected).await;
                            break;
                        };
                        if n.uuid != protocol::NUS_TX_UUID {
                            continue;
                        }
                        let records = parser.feed(&n.value);
                        if records.is_empty() {
                            continue;
                        }
                        let samples: Vec<ImuSample> = records
                            .into_iter()
                            .map(|r| ImuSample {
                                gyro: r.gyro,
                                accel: r.accel,
                                mag: None,
                                timestamp_us: 0,
                            })
                            .collect();
                        if tx.send(ChannelInfo::ImuSamples(samples)).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }));

        Ok(rx)
    }

    async fn stop(&mut self) -> Result<(), DeviceError> {
        if let Some(stop) = self.stop_tx.take() {
            let _ = stop.send(true);
        }
        if let Some(task) = self.task.take() {
            // Bound the wait so a notification stream wedged in the BLE stack
            // cannot hang stop() indefinitely.
            if tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .is_err()
            {
                tracing::warn!(id = %self.metadata.id, "hopx task did not exit within 2s");
            }
        }
        if let Some(rx_char) = self.rx_char.take() {
            let _ = write_command(&self.peripheral, &rx_char, &protocol::STOP_CMD).await;
        }
        let _ = self.peripheral.disconnect().await;
        Ok(())
    }

    async fn set_led_mask(&mut self, _mask: u8) -> Result<(), DeviceError> {
        // No addressable LED — ignored so the unified UI need not special-case.
        Ok(())
    }

    async fn set_rumble(&mut self, _intensity: f32) -> Result<(), DeviceError> {
        // No rumble motor — ignored.
        Ok(())
    }
}
