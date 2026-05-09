//! JoyCon1Device — implements `Device` trait for Joy-Con L/R + Pro Controller + Charging Grip.

use crate::axis_remap;
use crate::calibration::{accel_to_m_s2, gyro_to_rad_s, raw_minus_offset};
use crate::hid::{spawn_reader, HidReaderHandle};
use crate::ids::ControllerKind;
use crate::report::{parse_0x21_spi_reply, parse_0x30};
use crate::reset_buttons;
use crate::spi_cal::{parse_factory_block, user_override_magic_present, ImuCalibration};
use crate::subcmd::{
    device_info, enable_imu, enable_rumble, set_input_report_mode, set_player_leds, spi_read,
};
use arc_swap::ArcSwap;
use device_traits::{
    ChannelInfo, Device, DeviceCapabilities, DeviceError, DeviceId, DeviceMetadata, ImuSample,
    ResetButtonDetector,
};
use hidapi::HidDevice;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Instant;
use tokio::sync::mpsc;

/// SPI flash addresses queried during connect.
const SPI_ADDR_FACTORY_CAL: u32 = 0x6020;
const SPI_ADDR_USER_OVERRIDE_MAGIC: u32 = 0x8026;
const SPI_ADDR_USER_OVERRIDE_BLOCK: u32 = 0x8028;

pub struct JoyCon1Device {
    metadata: DeviceMetadata,
    kind: ControllerKind,
    cal: Arc<ArcSwap<ImuCalibration>>,
    /// Wrapped in `Arc<StdMutex>` so the HID thread can hold it after `start()` returns.
    hid: Arc<StdMutex<Option<HidDevice>>>,
    reader: Option<HidReaderHandle>,
    pkt_counter: u8,
    /// `true` for USB-attached devices; `false` for Bluetooth. Affects connect sequence.
    is_usb: bool,
}

impl JoyCon1Device {
    pub fn new(
        device: HidDevice,
        kind: ControllerKind,
        is_usb: bool,
        serial: String,
        mac: [u8; 6],
    ) -> Self {
        let id = DeviceId { mac, serial };
        let metadata = DeviceMetadata {
            id,
            kind: kind.into_device_kind(),
            firmware: None,
            capabilities: DeviceCapabilities {
                has_magnetometer: false,
                has_battery: true,
                has_rumble: true,
                native_imu_rate_hz: 200,
            },
        };
        Self {
            metadata,
            kind,
            cal: Arc::new(ArcSwap::from_pointee(ImuCalibration::zero())),
            hid: Arc::new(StdMutex::new(Some(device))),
            reader: None,
            pkt_counter: 0,
            is_usb,
        }
    }

    fn next_counter(&mut self) -> u8 {
        let c = self.pkt_counter;
        self.pkt_counter = self.pkt_counter.wrapping_add(1);
        c
    }
}

#[async_trait::async_trait]
impl Device for JoyCon1Device {
    fn metadata(&self) -> &DeviceMetadata {
        &self.metadata
    }

    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>, DeviceError> {
        if self.reader.is_some() {
            return Err(DeviceError::Hid("already started".into()));
        }

        self.run_connect_sequence().await?;
        self.request_calibration_reads().await?;

        let kind = self.kind;
        let cal_swap = self.cal.clone();
        let id_for_log = self.metadata.id.clone();
        let hid = self
            .hid
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| DeviceError::Hid("hid handle already moved".into()))?;

        let mut detector = ResetButtonDetector::new();

        // Reader-local SPI cal reconciliation state.
        let mut user_override_magic_seen = false;
        let mut pending_user_override: Option<Vec<u8>> = None;

        // Battery debounce: only send when value changes or every ~3 s keepalive.
        let mut last_battery_fraction: f32 = -1.0;
        let mut battery_report_counter: u32 = 0;

        let mut handle = spawn_reader(hid, move |buf, tx, _sample_tx| {
            // 0x21 subcommand-reply path (cal SPI replies arrive here).
            if buf.first() == Some(&0x21) {
                if let Some(reply) = parse_0x21_spi_reply(buf) {
                    handle_spi_reply(
                        &reply,
                        &cal_swap,
                        &mut user_override_magic_seen,
                        &mut pending_user_override,
                        &id_for_log,
                    );
                }
                return;
            }

            // 0x30 IMU input-report path.
            let report = match parse_0x30(buf) {
                Ok(r) => r,
                Err(_) => return,
            };

            let cal = cal_swap.load();
            let mut samples = Vec::with_capacity(3);
            for raw_s in report.imu_samples.iter() {
                let accel_off = raw_minus_offset(raw_s.accel, cal.accel_offset);
                let gyro_off = raw_minus_offset(raw_s.gyro, cal.gyro_offset);
                let mut accel = accel_to_m_s2(accel_off);
                let mut gyro = gyro_to_rad_s(gyro_off);
                accel = axis_remap::apply(kind, accel);
                gyro = axis_remap::apply(kind, gyro);
                samples.push(ImuSample {
                    gyro,
                    accel,
                    mag: None,
                    timestamp_us: 0,
                });
            }
            let _ = tx.blocking_send(ChannelInfo::ImuSamples(samples));

            // Debounce battery: only forward when the value actually changes,
            // or every ~600 reports (~3 s at 200 Hz) as a keepalive.
            battery_report_counter += 1;
            if report.battery.fraction != last_battery_fraction || battery_report_counter >= 600 {
                let _ = tx.blocking_send(ChannelInfo::Battery(report.battery));
                last_battery_fraction = report.battery.fraction;
                battery_report_counter = 0;
            }

            let now = Instant::now();
            let btn_state = reset_buttons::decode(kind, report.buttons);
            if let Some(reset) = detector.observe(btn_state, now) {
                let _ = tx.blocking_send(ChannelInfo::ResetRequested(reset));
            }
        });

        // Take events_rx for caller; self.reader keeps shutdown + samples_rx + thread join.
        let events_rx = std::mem::replace(&mut handle.events_rx, mpsc::channel(1).1);
        self.reader = Some(handle);

        let id = self.metadata.id.clone();
        let (tx_out, rx_out) = mpsc::channel(64);
        tokio::spawn(async move {
            let _ = tx_out.send(ChannelInfo::Connected(id)).await;
            let mut inner = events_rx;
            while let Some(ev) = inner.recv().await {
                if tx_out.send(ev).await.is_err() {
                    break;
                }
            }
        });

        Ok(rx_out)
    }

    async fn stop(&mut self) -> Result<(), DeviceError> {
        if let Some(mut r) = self.reader.take() {
            r.shutdown();
        }
        Ok(())
    }

    async fn set_led_mask(&mut self, mask: u8) -> Result<(), DeviceError> {
        let cnt = self.next_counter();
        self.write_report(set_player_leds(cnt, mask)).await
    }

    async fn set_rumble(&mut self, on: bool) -> Result<(), DeviceError> {
        let cnt = self.next_counter();
        let arg = if on { 0x01 } else { 0x00 };
        let buf = crate::subcmd::build_report_0x01(cnt, 0x48, &[arg]);
        self.write_report(buf).await
    }
}

impl JoyCon1Device {
    async fn write_report(&self, bytes: Vec<u8>) -> Result<(), DeviceError> {
        let hid = self.hid.clone();
        tokio::task::spawn_blocking(move || -> Result<(), DeviceError> {
            let guard = hid.lock().unwrap();
            let dev = guard
                .as_ref()
                .ok_or_else(|| DeviceError::Hid("device handle moved to reader thread".into()))?;
            dev.write(&bytes)
                .map_err(|e| DeviceError::Hid(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| DeviceError::Hid(e.to_string()))??;
        Ok(())
    }

    async fn run_connect_sequence(&mut self) -> Result<(), DeviceError> {
        if self.is_usb {
            for opcode in [0x01_u8, 0x02, 0x03, 0x02, 0x04] {
                self.write_report(crate::subcmd::build_report_0x80(opcode))
                    .await?;
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        }

        let cnt = self.next_counter();
        self.write_report(enable_rumble(cnt)).await?;
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        let cnt = self.next_counter();
        self.write_report(enable_imu(cnt)).await?;
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        let cnt = self.next_counter();
        self.write_report(set_input_report_mode(cnt, 0x30)).await?;
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        let cnt = self.next_counter();
        self.write_report(set_player_leds(cnt, 0b0000_0001)).await?;
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        let cnt = self.next_counter();
        self.write_report(device_info(cnt)).await?;
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        Ok(())
    }

    /// Issue the three SPI reads that feed the reader-side cal reconciliation
    /// state machine: factory block, user-override magic, user-override block.
    /// Replies arrive asynchronously as 0x21 reports and update `self.cal`
    /// via the ArcSwap; the IMU stream begins applying the resolved cal as
    /// soon as the relevant reply lands (typically within ~150ms).
    async fn request_calibration_reads(&mut self) -> Result<(), DeviceError> {
        let cnt = self.next_counter();
        self.write_report(spi_read(cnt, SPI_ADDR_FACTORY_CAL, 24))
            .await?;
        tokio::time::sleep(std::time::Duration::from_millis(15)).await;

        let cnt = self.next_counter();
        self.write_report(spi_read(cnt, SPI_ADDR_USER_OVERRIDE_MAGIC, 2))
            .await?;
        tokio::time::sleep(std::time::Duration::from_millis(15)).await;

        let cnt = self.next_counter();
        self.write_report(spi_read(cnt, SPI_ADDR_USER_OVERRIDE_BLOCK, 24))
            .await?;
        Ok(())
    }
}

/// Apply an SPI reply to the calibration ArcSwap.
fn handle_spi_reply(
    reply: &crate::report::SpiReadReply,
    cal_swap: &Arc<ArcSwap<ImuCalibration>>,
    magic_seen: &mut bool,
    pending_override: &mut Option<Vec<u8>>,
    id: &DeviceId,
) {
    match reply.addr {
        SPI_ADDR_FACTORY_CAL => {
            if let Ok(cal) = parse_factory_block(&reply.data) {
                if cal.plausibility_warning {
                    tracing::warn!(
                        id = %id,
                        "factory cal block implausible — using zeros (clone unit?)",
                    );
                }
                // Only adopt factory cal if no user override already won.
                if pending_override.is_none() || !*magic_seen {
                    cal_swap.store(Arc::new(cal));
                    tracing::info!(id = %id, "factory cal block applied");
                }
            }
        }
        SPI_ADDR_USER_OVERRIDE_MAGIC => {
            *magic_seen = user_override_magic_present(&reply.data);
            if *magic_seen {
                tracing::info!(id = %id, "user-override magic present (0xA1B2)");
                if let Some(data) = pending_override.take() {
                    if let Ok(cal) = parse_factory_block(&data) {
                        cal_swap.store(Arc::new(cal));
                        tracing::info!(id = %id, "user-override cal applied (deferred)");
                    }
                }
            }
        }
        SPI_ADDR_USER_OVERRIDE_BLOCK => {
            if *magic_seen {
                if let Ok(cal) = parse_factory_block(&reply.data) {
                    cal_swap.store(Arc::new(cal));
                    tracing::info!(id = %id, "user-override cal applied");
                }
            } else {
                // Magic reply not seen yet — cache the block and apply when the magic confirms.
                *pending_override = Some(reply.data.clone());
            }
        }
        _ => {}
    }
}
