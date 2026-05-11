//! Joy-Con 2 BLE transport + input report 0x05 parser.
//!
//! Transport:
//! - BLE only (`btleplug`)
//! - discover via Nintendo manufacturer data (0x0553, prefix 01 00 03 7E)
//! - connect + subscribe to common input characteristic
//!
//! Parsing:
//! - report 0x05 fixed 62-byte layout
//! - accel/gyro from motion block offsets 0x30..0x3A
//! - optional mag from 0x19..0x1E
//! - Home/Capture bits from byte 0x05 (0x10 / 0x20)

use btleplug::api::{
    Central, CharPropFlags, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use device_traits::{
    BatteryState, ButtonState, ChannelInfo, Device, DeviceCapabilities, DeviceError, DeviceId,
    DeviceKind, DeviceMetadata, ImuSample, ResetButtonDetector,
};
use futures_util::StreamExt;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};
use uuid::{uuid, Uuid};

const NINTENDO_MFR_ID: u16 = 0x0553;
const NINTENDO_ADV_PREFIX: [u8; 4] = [0x01, 0x00, 0x03, 0x7E];
const ADV_KIND_JC2_R: u8 = 0x05;
const ADV_KIND_JC2_L: u8 = 0x06;

const INPUT_COMMON_UUID: Uuid = uuid!("ab7de9be-89fe-49ad-828f-118f09df7fd2");
const WRITE_COMMAND_UUID: Uuid = uuid!("649d4ac9-8eb7-4e6c-af44-1ea54fe5f005");
const RESPONSE_NOTIFY_UUID: Uuid = uuid!("c765a961-d9d8-4d36-a20a-5315b111836a");

const REPORT_0X05_LEN: usize = 62;

const BATTERY_MV_MIN: f32 = 3000.0;
const BATTERY_MV_MAX: f32 = 4200.0;
const MAG_MIN_MAGNITUDE_UT: f32 = 10.0;
const MAG_MAX_MAGNITUDE_UT: f32 = 120.0;
const FLASH_ADDR_PID: u32 = 0x0001_3012;
const FLASH_ADDR_MAG_BIAS: u32 = 0x0001_3100;
const ACCEL_M_S2_PER_LSB: f32 = 9.806_65 / 4096.0;
const GYRO_RAD_S_PER_LSB: f32 = 34.8 / 32767.0;
const MAG_UT_PER_LSB: f32 = 4900.0 / 32767.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoyCon2Kind {
    Left,
    Right,
}

impl JoyCon2Kind {
    fn into_device_kind(self) -> DeviceKind {
        match self {
            Self::Left => DeviceKind::JoyCon2L,
            Self::Right => DeviceKind::JoyCon2R,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ParsedJc2Report {
    pub sample: ImuSample,
    pub battery: BatteryState,
    pub home_pressed: bool,
    pub capture_pressed: bool,
}

pub fn kind_from_manufacturer_data(mfr: &HashMap<u16, Vec<u8>>) -> Option<JoyCon2Kind> {
    let data = mfr.get(&NINTENDO_MFR_ID)?;
    if data.len() < 5 || data[..4] != NINTENDO_ADV_PREFIX {
        return None;
    }
    match data[4] {
        ADV_KIND_JC2_L => Some(JoyCon2Kind::Left),
        ADV_KIND_JC2_R => Some(JoyCon2Kind::Right),
        _ => None,
    }
}

pub fn parse_input_report_0x05(
    kind: JoyCon2Kind,
    buf: &[u8],
    mag_bias_ut: Option<[f32; 3]>,
) -> Option<ParsedJc2Report> {
    if buf.len() < REPORT_0X05_LEN {
        return None;
    }

    let read_i16 = |off: usize| i16::from_le_bytes([buf[off], buf[off + 1]]);
    let read_u16 = |off: usize| u16::from_le_bytes([buf[off], buf[off + 1]]);

    let accel_raw = [
        read_i16(0x30) as f32,
        read_i16(0x32) as f32,
        read_i16(0x34) as f32,
    ];
    let gyro_raw = [
        read_i16(0x36) as f32,
        read_i16(0x38) as f32,
        read_i16(0x3A) as f32,
    ];
    let mag_raw = [
        read_i16(0x19) as f32,
        read_i16(0x1B) as f32,
        read_i16(0x1D) as f32,
    ];

    let accel = remap_axes(
        kind,
        [
            accel_raw[0] * ACCEL_M_S2_PER_LSB,
            accel_raw[1] * ACCEL_M_S2_PER_LSB,
            accel_raw[2] * ACCEL_M_S2_PER_LSB,
        ],
    );
    let gyro = remap_axes(
        kind,
        [
            gyro_raw[0] * GYRO_RAD_S_PER_LSB,
            gyro_raw[1] * GYRO_RAD_S_PER_LSB,
            gyro_raw[2] * GYRO_RAD_S_PER_LSB,
        ],
    );
    let mut mag = remap_axes(
        kind,
        [
            mag_raw[0] * MAG_UT_PER_LSB,
            mag_raw[1] * MAG_UT_PER_LSB,
            mag_raw[2] * MAG_UT_PER_LSB,
        ],
    );
    if let Some(bias) = mag_bias_ut {
        mag[0] -= bias[0];
        mag[1] -= bias[1];
        mag[2] -= bias[2];
    }
    let mag_norm = (mag[0] * mag[0] + mag[1] * mag[1] + mag[2] * mag[2]).sqrt();
    let mag = if (MAG_MIN_MAGNITUDE_UT..MAG_MAX_MAGNITUDE_UT).contains(&mag_norm) {
        Some(mag)
    } else {
        None
    };

    let battery_mv = read_u16(0x1F) as f32;
    let charging_state = buf[0x21];
    let fraction =
        ((battery_mv - BATTERY_MV_MIN) / (BATTERY_MV_MAX - BATTERY_MV_MIN)).clamp(0.0, 1.0);
    let charging = charging_state >= 0x30;

    let home_pressed = (buf[0x05] & 0x10) != 0;
    let capture_pressed = (buf[0x05] & 0x20) != 0;

    Some(ParsedJc2Report {
        sample: ImuSample {
            gyro,
            accel,
            mag,
            timestamp_us: 0,
        },
        battery: BatteryState { fraction, charging },
        home_pressed,
        capture_pressed,
    })
}

fn remap_axes(kind: JoyCon2Kind, raw_xyz: [f32; 3]) -> [f32; 3] {
    // SDL base remap for Switch 2 controllers.
    let base = [raw_xyz[0], raw_xyz[2], -raw_xyz[1]];
    // Standalone Joy-Con orientation correction.
    match kind {
        JoyCon2Kind::Left => [base[2], base[1], -base[0]],
        JoyCon2Kind::Right => [-base[2], base[1], base[0]],
    }
}

fn build_feature_select_cmd(subcmd: u8, mask: u8) -> [u8; 12] {
    [
        0x0C, 0x91, 0x01, subcmd, 0x00, 0x04, 0x00, 0x00, mask, 0x00, 0x00, 0x00,
    ]
}

fn build_rumble_preset_cmd(preset: u8) -> [u8; 12] {
    [
        0x0A, 0x91, 0x01, 0x02, 0x00, 0x04, 0x00, 0x00, preset, 0x00, 0x00, 0x00,
    ]
}

fn build_player_led_cmd(mask: u8) -> [u8; 12] {
    [
        0x09,
        0x91,
        0x01,
        0x01,
        0x00,
        0x04,
        0x00,
        0x00,
        mask & 0x0F,
        0x00,
        0x00,
        0x00,
    ]
}

fn build_flash_read_cmd(address: u32, len: u16) -> [u8; 16] {
    [
        0x02,
        0x91,
        0x01,
        0x01,
        0x00,
        0x06,
        0x00,
        0x00,
        (address & 0xFF) as u8,
        ((address >> 8) & 0xFF) as u8,
        ((address >> 16) & 0xFF) as u8,
        ((address >> 24) & 0xFF) as u8,
        (len & 0xFF) as u8,
        ((len >> 8) & 0xFF) as u8,
        0x00,
        0x00,
    ]
}

fn contains_addr_le(bytes: &[u8], addr: u32) -> bool {
    if bytes.len() < 8 {
        return false;
    }
    let needle = [
        (addr & 0xFF) as u8,
        ((addr >> 8) & 0xFF) as u8,
        ((addr >> 16) & 0xFF) as u8,
        ((addr >> 24) & 0xFF) as u8,
    ];
    let end = bytes.len().saturating_sub(4).min(24);
    (4..=end).any(|i| bytes[i..i + 4] == needle)
}

fn extract_mag_bias_from_flash_response(bytes: &[u8]) -> Option<[f32; 3]> {
    if bytes.len() < 20 {
        return None;
    }
    let max_off = bytes.len().saturating_sub(12).min(24);
    for off in 8..=max_off {
        let bx = f32::from_le_bytes(bytes[off..off + 4].try_into().ok()?);
        let by = f32::from_le_bytes(bytes[off + 4..off + 8].try_into().ok()?);
        let bz = f32::from_le_bytes(bytes[off + 8..off + 12].try_into().ok()?);
        if !bx.is_finite() || !by.is_finite() || !bz.is_finite() {
            continue;
        }
        if bx.abs() > 500.0 || by.abs() > 500.0 || bz.abs() > 500.0 {
            continue;
        }
        if bx == 0.0 && by == 0.0 && bz == 0.0 {
            continue;
        }
        return Some([bx, by, bz]);
    }
    None
}

fn extract_pid_from_flash_response(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 14 {
        return None;
    }
    let max_off = bytes.len().saturating_sub(2).min(24);
    for off in 8..=max_off {
        let pid = u16::from_le_bytes(bytes[off..off + 2].try_into().ok()?);
        if matches!(pid, 0x2066 | 0x2067 | 0x2068 | 0x2069 | 0x2073) {
            return Some(pid);
        }
    }
    None
}

fn kind_from_pid(pid: u16) -> Option<JoyCon2Kind> {
    match pid {
        0x2067 => Some(JoyCon2Kind::Left),
        0x2066 | 0x2068 => Some(JoyCon2Kind::Right),
        _ => None,
    }
}

async fn ensure_connected(peripheral: &Peripheral) -> Result<(), DeviceError> {
    let connected = peripheral
        .is_connected()
        .await
        .map_err(|e| DeviceError::Hid(format!("jc2 is_connected failed: {e}")))?;
    if !connected {
        peripheral
            .connect()
            .await
            .map_err(|e| DeviceError::Hid(format!("jc2 connect failed: {e}")))?;
    }
    peripheral
        .discover_services()
        .await
        .map_err(|e| DeviceError::Hid(format!("jc2 discover_services failed: {e}")))?;
    Ok(())
}

fn find_char(peripheral: &Peripheral, uuid: Uuid) -> Option<Characteristic> {
    peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == uuid)
}

async fn enable_imu_and_mag(
    peripheral: &Peripheral,
    write_char: &Characteristic,
) -> Result<(), DeviceError> {
    // JoyCon2 reference implementation uses 0xFF (all feature bits) for init/start.
    let mask = 0xFF;
    let set_mask = build_feature_select_cmd(0x02, mask);
    peripheral
        .write(write_char, &set_mask, WriteType::WithoutResponse)
        .await
        .map_err(|e| DeviceError::Hid(format!("jc2 set feature mask failed: {e}")))?;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let enable = build_feature_select_cmd(0x04, mask);
    peripheral
        .write(write_char, &enable, WriteType::WithoutResponse)
        .await
        .map_err(|e| DeviceError::Hid(format!("jc2 enable features failed: {e}")))?;
    Ok(())
}

async fn resolve_mag_bias_via_flash(
    peripheral: &Peripheral,
    write_char: &Characteristic,
    kind: JoyCon2Kind,
) -> Option<[f32; 3]> {
    let response_char = find_char(peripheral, RESPONSE_NOTIFY_UUID)?;
    if !response_char.properties.contains(CharPropFlags::NOTIFY) {
        return None;
    }
    if peripheral.subscribe(&response_char).await.is_err() {
        return None;
    }

    let mut notifications = match peripheral.notifications().await {
        Ok(s) => s,
        Err(_) => {
            let _ = peripheral.unsubscribe(&response_char).await;
            return None;
        }
    };

    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500 + (attempt as u64 * 500))).await;
        }
        let cmd = build_flash_read_cmd(FLASH_ADDR_MAG_BIAS, 12);
        if peripheral
            .write(write_char, &cmd, WriteType::WithoutResponse)
            .await
            .is_err()
        {
            continue;
        }

        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline.saturating_duration_since(now);
            let Some(next) = tokio::time::timeout(remaining, notifications.next())
                .await
                .ok()
                .flatten()
            else {
                break;
            };
            if next.uuid != RESPONSE_NOTIFY_UUID {
                continue;
            }
            let bytes = next.value;
            if bytes.len() < 14 || bytes[0] != 0x02 || bytes[1] != 0x01 {
                continue;
            }
            if !contains_addr_le(&bytes, FLASH_ADDR_MAG_BIAS) {
                continue;
            }
            let bias =
                extract_mag_bias_from_flash_response(&bytes).map(|raw| remap_axes(kind, raw));
            let _ = peripheral.unsubscribe(&response_char).await;
            return bias;
        }
    }
    let _ = peripheral.unsubscribe(&response_char).await;
    None
}

async fn resolve_kind_via_flash(
    peripheral: &Peripheral,
    write_char: &Characteristic,
) -> Option<JoyCon2Kind> {
    let response_char = find_char(peripheral, RESPONSE_NOTIFY_UUID)?;
    if !response_char.properties.contains(CharPropFlags::NOTIFY) {
        return None;
    }
    if peripheral.subscribe(&response_char).await.is_err() {
        return None;
    }

    let mut notifications = match peripheral.notifications().await {
        Ok(s) => s,
        Err(_) => {
            let _ = peripheral.unsubscribe(&response_char).await;
            return None;
        }
    };

    let cmd = build_flash_read_cmd(FLASH_ADDR_PID, 2);
    if peripheral
        .write(write_char, &cmd, WriteType::WithoutResponse)
        .await
        .is_err()
    {
        let _ = peripheral.unsubscribe(&response_char).await;
        return None;
    }

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            let _ = peripheral.unsubscribe(&response_char).await;
            return None;
        }
        let remaining = deadline.saturating_duration_since(now);
        let Some(next) = tokio::time::timeout(remaining, notifications.next())
            .await
            .ok()
            .flatten()
        else {
            let _ = peripheral.unsubscribe(&response_char).await;
            return None;
        };
        if next.uuid != RESPONSE_NOTIFY_UUID {
            continue;
        }
        let bytes = next.value;
        if bytes.len() < 14 || bytes[0] != 0x02 || bytes[1] != 0x01 {
            continue;
        }
        if !contains_addr_le(&bytes, FLASH_ADDR_PID) {
            continue;
        }
        let kind = extract_pid_from_flash_response(&bytes).and_then(kind_from_pid);
        let _ = peripheral.unsubscribe(&response_char).await;
        return kind;
    }
}

pub struct JoyCon2Device {
    metadata: DeviceMetadata,
    kind: JoyCon2Kind,
    peripheral: Peripheral,
    write_char: Option<Characteristic>,
    mag_bias_ut: Option<[f32; 3]>,
    task: Option<tokio::task::JoinHandle<()>>,
    stop_tx: Option<watch::Sender<bool>>,
}

impl JoyCon2Device {
    pub fn new(peripheral: Peripheral, kind: JoyCon2Kind, serial: String, mac: [u8; 6]) -> Self {
        Self {
            metadata: DeviceMetadata {
                id: DeviceId { mac, serial },
                kind: kind.into_device_kind(),
                firmware: None,
                capabilities: DeviceCapabilities {
                    has_magnetometer: true,
                    has_battery: true,
                    has_rumble: true,
                    native_imu_rate_hz: 62,
                },
            },
            kind,
            peripheral,
            write_char: None,
            mag_bias_ut: None,
            task: None,
            stop_tx: None,
        }
    }
}

#[async_trait::async_trait]
impl Device for JoyCon2Device {
    fn metadata(&self) -> &DeviceMetadata {
        &self.metadata
    }

    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>, DeviceError> {
        if self.task.is_some() {
            return Err(DeviceError::Hid("jc2 already started".into()));
        }

        ensure_connected(&self.peripheral).await?;
        let input_char = find_char(&self.peripheral, INPUT_COMMON_UUID)
            .ok_or_else(|| DeviceError::Hid("jc2 input characteristic not found".into()))?;
        let write_char = find_char(&self.peripheral, WRITE_COMMAND_UUID)
            .ok_or_else(|| DeviceError::Hid("jc2 command characteristic not found".into()))?;
        self.write_char = Some(write_char.clone());
        if let Some(kind_from_flash) = resolve_kind_via_flash(&self.peripheral, &write_char).await {
            if kind_from_flash != self.kind {
                tracing::info!(
                    advertised = ?self.kind,
                    flash = ?kind_from_flash,
                    "jc2 kind corrected from flash pid"
                );
                self.kind = kind_from_flash;
            }
        }
        if !input_char.properties.contains(CharPropFlags::NOTIFY) {
            return Err(DeviceError::Hid(
                "jc2 input characteristic lacks notify property".into(),
            ));
        }

        self.peripheral
            .subscribe(&input_char)
            .await
            .map_err(|e| DeviceError::Hid(format!("jc2 subscribe failed: {e}")))?;
        enable_imu_and_mag(&self.peripheral, &write_char).await?;
        self.mag_bias_ut =
            resolve_mag_bias_via_flash(&self.peripheral, &write_char, self.kind).await;

        let mut notifications = self
            .peripheral
            .notifications()
            .await
            .map_err(|e| DeviceError::Hid(format!("jc2 notifications stream failed: {e}")))?;
        let kind = self.kind;
        let (tx, rx) = mpsc::channel(128);
        let id = self.metadata.id.clone();
        let (stop_tx, mut stop_rx) = watch::channel(false);
        self.stop_tx = Some(stop_tx);
        let mag_bias_ut = self.mag_bias_ut;

        self.task = Some(tokio::spawn(async move {
            let _ = tx.send(ChannelInfo::Connected(id)).await;
            let mut detector = ResetButtonDetector::new();
            let mut last_battery = f32::NAN;
            loop {
                tokio::select! {
                    _ = stop_rx.changed() => {
                        break;
                    }
                    maybe = notifications.next() => {
                        let Some(n) = maybe else {
                            let _ = tx.send(ChannelInfo::Disconnected).await;
                            break;
                        };
                        if n.uuid != INPUT_COMMON_UUID {
                            continue;
                        }
                        let Some(parsed) = parse_input_report_0x05(kind, &n.value, mag_bias_ut) else {
                            continue;
                        };
                        if tx.send(ChannelInfo::ImuSamples(vec![parsed.sample])).await.is_err() {
                            break;
                        }
                        if last_battery.is_nan() || (parsed.battery.fraction - last_battery).abs() > 0.01 {
                            if tx.send(ChannelInfo::Battery(parsed.battery)).await.is_err() {
                                break;
                            }
                            last_battery = parsed.battery.fraction;
                        }
                        let btn = ButtonState::HomeOrCapture {
                            home_pressed: parsed.home_pressed,
                            capture_pressed: parsed.capture_pressed,
                        };
                        if let Some(reset) = detector.observe(btn, Instant::now()) {
                            if tx.send(ChannelInfo::ResetRequested(reset)).await.is_err() {
                                break;
                            }
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
            let _ = task.await;
        }
        self.write_char = None;
        self.mag_bias_ut = None;
        let _ = self.peripheral.disconnect().await;
        Ok(())
    }

    async fn set_led_mask(&mut self, mask: u8) -> Result<(), DeviceError> {
        ensure_connected(&self.peripheral).await?;
        let write_char = self.write_char.clone().ok_or_else(|| {
            DeviceError::Hid("jc2 not started: command characteristic unavailable".into())
        })?;
        let cmd = build_player_led_cmd(mask);
        self.peripheral
            .write(&write_char, &cmd, WriteType::WithoutResponse)
            .await
            .map_err(|e| DeviceError::Hid(format!("jc2 led command failed: {e}")))?;
        Ok(())
    }

    async fn set_rumble(&mut self, on: bool) -> Result<(), DeviceError> {
        ensure_connected(&self.peripheral).await?;
        let write_char = self.write_char.clone().ok_or_else(|| {
            DeviceError::Hid("jc2 not started: command characteristic unavailable".into())
        })?;
        let preset = if on { 0x01 } else { 0x00 };
        let cmd = build_rumble_preset_cmd(preset);
        self.peripheral
            .write(&write_char, &cmd, WriteType::WithoutResponse)
            .await
            .map_err(|e| DeviceError::Hid(format!("jc2 rumble command failed: {e}")))?;
        Ok(())
    }
}

pub struct JoyCon2Scanner {
    adapters: Vec<Adapter>,
    scan_started: bool,
}

#[derive(Debug, Clone)]
pub struct NearbyJoyCon2 {
    pub kind: JoyCon2Kind,
    pub name: String,
    pub address: String,
    pub mac: [u8; 6],
}

impl JoyCon2Scanner {
    pub async fn new() -> Option<Self> {
        let manager = Manager::new().await.ok()?;
        let adapters = manager.adapters().await.ok()?;
        if adapters.is_empty() {
            return None;
        }
        Some(Self {
            adapters,
            scan_started: false,
        })
    }

    pub async fn poll(
        &mut self,
        known: &mut HashMap<String, tokio::time::Instant>,
        rediscover_after: Duration,
        out: &mpsc::Sender<(DeviceMetadata, Box<dyn Device>)>,
    ) -> Result<(), DeviceError> {
        if !self.scan_started {
            for adapter in &self.adapters {
                if let Err(e) = adapter.start_scan(ScanFilter::default()).await {
                    tracing::warn!(error = %e, "jc2 adapter start_scan failed");
                }
            }
            self.scan_started = true;
        }

        for adapter in &self.adapters {
            let peripherals = adapter
                .peripherals()
                .await
                .map_err(|e| DeviceError::Hid(format!("jc2 peripherals query failed: {e}")))?;
            for peripheral in peripherals {
                let Some(props) = peripheral.properties().await.map_err(|e| {
                    DeviceError::Hid(format!("jc2 peripheral properties failed: {e}"))
                })?
                else {
                    continue;
                };

                let Some(kind) = kind_from_manufacturer_data(&props.manufacturer_data) else {
                    continue;
                };
                let addr = props.address.to_string();
                let key = format!("jc2#{addr}");
                let is_connected = peripheral.is_connected().await.unwrap_or(false);
                if is_connected {
                    known.insert(key.clone(), tokio::time::Instant::now());
                    continue;
                }
                if let Some(last_seen) = known.get(&key) {
                    if last_seen.elapsed() < rediscover_after {
                        continue;
                    }
                }
                known.insert(key, tokio::time::Instant::now());

                let mac = mac_from_addr(&addr).unwrap_or_else(|| hash_to_mac(&addr));
                let serial = props
                    .local_name
                    .unwrap_or_else(|| format!("JoyCon2-{addr}"));
                let dev = JoyCon2Device::new(peripheral.clone(), kind, serial, mac);
                let meta = dev.metadata().clone();
                if out
                    .send((meta, Box::new(dev) as Box<dyn Device>))
                    .await
                    .is_err()
                {
                    return Ok(());
                }
            }
        }
        Ok(())
    }
}

pub async fn scan_nearby(timeout: std::time::Duration) -> Result<Vec<NearbyJoyCon2>, DeviceError> {
    let manager = Manager::new()
        .await
        .map_err(|e| DeviceError::Hid(format!("jc2 manager init failed: {e}")))?;
    let adapters = manager
        .adapters()
        .await
        .map_err(|e| DeviceError::Hid(format!("jc2 adapters query failed: {e}")))?;
    if adapters.is_empty() {
        return Ok(Vec::new());
    }
    for adapter in &adapters {
        if let Err(e) = adapter.start_scan(ScanFilter::default()).await {
            tracing::warn!(error = %e, "jc2 one-shot start_scan failed");
        }
    }
    tokio::time::sleep(timeout).await;

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for adapter in &adapters {
        let peripherals = adapter
            .peripherals()
            .await
            .map_err(|e| DeviceError::Hid(format!("jc2 one-shot peripherals query failed: {e}")))?;
        for peripheral in peripherals {
            let Some(props) = peripheral.properties().await.map_err(|e| {
                DeviceError::Hid(format!("jc2 one-shot peripheral properties failed: {e}"))
            })?
            else {
                continue;
            };
            let Some(kind) = kind_from_manufacturer_data(&props.manufacturer_data) else {
                continue;
            };
            let address = props.address.to_string();
            if !seen.insert(address.clone()) {
                continue;
            }
            let mac = mac_from_addr(&address).unwrap_or_else(|| hash_to_mac(&address));
            out.push(NearbyJoyCon2 {
                kind,
                name: props
                    .local_name
                    .unwrap_or_else(|| format!("JoyCon2-{address}")),
                address,
                mac,
            });
        }
    }
    Ok(out)
}

fn mac_from_addr(addr: &str) -> Option<[u8; 6]> {
    let mut out = [0u8; 6];
    let mut count = 0usize;
    for (idx, part) in addr.split(':').enumerate() {
        if idx >= 6 {
            return None;
        }
        out[idx] = u8::from_str_radix(part, 16).ok()?;
        count += 1;
    }
    if count == 6 {
        Some(out)
    } else {
        None
    }
}

fn hash_to_mac(seed: &str) -> [u8; 6] {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    seed.hash(&mut hasher);
    let h = hasher.finish().to_le_bytes();
    [0x02, h[0], h[1], h[2], h[3], h[4]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manufacturer_data_detects_left_and_right() {
        let mut m = HashMap::new();
        m.insert(
            NINTENDO_MFR_ID,
            vec![0x01, 0x00, 0x03, 0x7E, ADV_KIND_JC2_L],
        );
        assert_eq!(kind_from_manufacturer_data(&m), Some(JoyCon2Kind::Left));
        m.insert(
            NINTENDO_MFR_ID,
            vec![0x01, 0x00, 0x03, 0x7E, ADV_KIND_JC2_R],
        );
        assert_eq!(kind_from_manufacturer_data(&m), Some(JoyCon2Kind::Right));
    }

    #[test]
    fn manufacturer_data_rejects_wrong_prefix() {
        let mut m = HashMap::new();
        m.insert(
            NINTENDO_MFR_ID,
            vec![0xFF, 0x00, 0x03, 0x7E, ADV_KIND_JC2_L],
        );
        assert_eq!(kind_from_manufacturer_data(&m), None);
    }

    #[test]
    fn feature_cmd_layout_matches_protocol() {
        let cmd = build_feature_select_cmd(0x02, 0x84);
        assert_eq!(
            cmd,
            [0x0C, 0x91, 0x01, 0x02, 0x00, 0x04, 0x00, 0x00, 0x84, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn rumble_preset_cmd_layout_matches_protocol() {
        let cmd = build_rumble_preset_cmd(0x01);
        assert_eq!(
            cmd,
            [0x0A, 0x91, 0x01, 0x02, 0x00, 0x04, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn player_led_cmd_layout_matches_protocol() {
        let cmd = build_player_led_cmd(0b1111_0011);
        assert_eq!(
            cmd,
            [0x09, 0x91, 0x01, 0x01, 0x00, 0x04, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn pid_flash_response_extracts_known_pid() {
        let mut bytes = vec![0u8; 20];
        bytes[0] = 0x02;
        bytes[1] = 0x01;
        bytes[10..12].copy_from_slice(&0x2067u16.to_le_bytes());
        assert_eq!(extract_pid_from_flash_response(&bytes), Some(0x2067));
        assert_eq!(kind_from_pid(0x2067), Some(JoyCon2Kind::Left));
        assert_eq!(kind_from_pid(0x2066), Some(JoyCon2Kind::Right));
    }

    #[test]
    fn parse_report_0x05_extracts_imu_mag_battery_and_buttons() {
        let mut buf = [0u8; REPORT_0X05_LEN];
        buf[0x05] = 0x30; // home + capture
        buf[0x1F..0x21].copy_from_slice(&3900u16.to_le_bytes());
        buf[0x21] = 0x34; // charging
        buf[0x19..0x1B].copy_from_slice(&100i16.to_le_bytes());
        buf[0x1B..0x1D].copy_from_slice(&200i16.to_le_bytes());
        buf[0x1D..0x1F].copy_from_slice(&(-300i16).to_le_bytes());
        buf[0x30..0x32].copy_from_slice(&4096i16.to_le_bytes()); // 1G on X
        buf[0x32..0x34].copy_from_slice(&0i16.to_le_bytes());
        buf[0x34..0x36].copy_from_slice(&0i16.to_le_bytes());
        buf[0x36..0x38].copy_from_slice(&1000i16.to_le_bytes());
        buf[0x38..0x3A].copy_from_slice(&0i16.to_le_bytes());
        buf[0x3A..0x3C].copy_from_slice(&0i16.to_le_bytes());

        let p = parse_input_report_0x05(JoyCon2Kind::Left, &buf, None).expect("parsed");
        assert!(p.home_pressed);
        assert!(p.capture_pressed);
        assert!(p.battery.charging);
        assert!(p.battery.fraction > 0.70 && p.battery.fraction < 0.80);
        let mag = p.sample.mag.expect("mag present");
        assert!(mag.iter().all(|v| v.is_finite()));
        let accel_norm = (p.sample.accel[0] * p.sample.accel[0]
            + p.sample.accel[1] * p.sample.accel[1]
            + p.sample.accel[2] * p.sample.accel[2])
            .sqrt();
        assert!((accel_norm - 9.806).abs() < 0.25);
        assert!(p.sample.gyro.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn parse_mac_from_ble_address() {
        assert_eq!(
            mac_from_addr("AA:BB:CC:DD:EE:FF"),
            Some([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF])
        );
        assert_eq!(mac_from_addr("AA:BB:CC"), None);
    }
}
