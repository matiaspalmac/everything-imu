//! Hardware-characterisation helper for Sony controllers: stream the undecoded
//! input report so the IMU scale, axis order, report rate, and — crucially — the
//! firmware sensor-timestamp field can be measured empirically.
//!
//! Unlike the Joy-Con, the DualSense / DualShock 4 carry a hardware sensor
//! timestamp in every input report (a free-running counter incremented by the
//! controller itself, immune to Bluetooth/USB delivery jitter). This dumps that
//! field's raw value and its per-report delta so the timestamp tick can be
//! converted to seconds and fed to fusion as a true `dt`, rather than the
//! delivery-rate estimate the Joy-Con path is forced to use.
//!
//! Self-contained HID path (does not go through the `Device`/pipeline machinery)
//! so it can run as a one-shot CLI diagnostic.

use std::time::Instant;

use crate::factory::DualSenseFactory;
use crate::hid::hid_api_singleton;
use crate::ids::ControllerKind;
use crate::report::ImuOffsets;

/// One undecoded input report plus running diagnostics.
pub struct RawSample {
    /// Report id byte (`buf[0]`): 0x01 (USB) or 0x31 (DualSense BT).
    pub report_id: u8,
    /// Total report length in bytes (64 = USB, 78 = DualSense BT).
    pub len: usize,
    /// Raw gyro int16 triplet at the kind/len-specific offset (wire order).
    pub gyro: [i16; 3],
    /// Raw accel int16 triplet (wire order).
    pub accel: [i16; 3],
    /// Firmware sensor timestamp parsed as u32 LE at `ts_offset`, if the report
    /// shape is known. `None` for an unrecognized layout.
    pub sensor_timestamp: Option<u32>,
    /// Delta of `sensor_timestamp` since the previous report (wrapping), for
    /// deriving the tick → seconds scale. `None` on the first report or unknown
    /// layout.
    pub ts_delta: Option<u32>,
    /// Byte offset the timestamp was read from (for the layout note).
    pub ts_offset: Option<usize>,
    /// Running average report rate (reports/s) over the whole window.
    pub rate_hz: f32,
}

/// Open the first paired Sony pad (matching pid + interface from the live HID
/// list) and return the open device plus its kind. Shared by the streaming and
/// haptic diagnostics.
fn open_first_pad() -> Result<(hidapi::HidDevice, ControllerKind, usize), String> {
    let paired = DualSenseFactory::list_paired().map_err(|e| format!("list paired: {e}"))?;
    let first = paired.first().ok_or("no paired Sony pad found")?;
    let kind = first.kind;
    eprintln!(
        "[ds] found {:?} pid=0x{:04X} iface={}, opening...",
        kind, first.pid, first.interface
    );
    let api = hid_api_singleton().map_err(|e| format!("hidapi init: {e}"))?;
    let guard = api.lock().map_err(|_| "hidapi poisoned")?;
    let info = guard
        .device_list()
        .find(|i| {
            i.vendor_id() == crate::ids::SONY_VID
                && i.product_id() == first.pid
                && i.interface_number() == first.interface
        })
        .ok_or("paired pad vanished before open")?;
    let interface = info.interface_number().max(0) as usize;
    let device = guard
        .open_path(info.path())
        .map_err(|e| format!("open: {e}"))?;
    Ok((device, kind, interface))
}

/// Build a DualSense USB output report (0x02, 48 bytes) setting the RGB lightbar
/// and both rumble motors. Mirrors `device::fill_ds5_payload`; kept local so the
/// diagnostic stays self-contained.
fn ds5_usb_output(r: u8, g: u8, b: u8, motor: u8) -> [u8; 48] {
    let mut rep = [0u8; 48];
    rep[0] = 0x02;
    rep[1] = 0xFF; // valid_flag0: rumble enable
    rep[2] = 0xF7; // valid_flag1: lightbar + LED enable
    rep[3] = motor; // weak / high-frequency motor
    rep[4] = motor; // strong / low-frequency motor
    rep[45] = r;
    rep[46] = g;
    rep[47] = b;
    rep
}

/// Drive the first paired DualSense through a short, visible/tactile output
/// sequence — RGB lightbar colours and rumble pulses — so the output-report path
/// can be confirmed on real hardware. Returns after restoring the pad to idle.
pub fn haptic_test() -> Result<(), String> {
    use std::thread::sleep;
    use std::time::Duration;

    let (device, kind, _iface) = open_first_pad()?;
    if !matches!(
        kind,
        ControllerKind::DualSense | ControllerKind::DualSenseEdge
    ) {
        return Err(format!(
            "haptic test only wired for DualSense, got {kind:?}"
        ));
    }

    // (label, r, g, b, motor, hold_ms)
    let steps: &[(&str, u8, u8, u8, u8, u64)] = &[
        ("RED  + strong rumble", 255, 0, 0, 220, 1200),
        ("GREEN (no rumble)", 0, 255, 0, 0, 1000),
        ("BLUE + light rumble", 0, 0, 255, 110, 1200),
        ("WHITE + full rumble", 255, 255, 255, 255, 1200),
        ("OFF", 0, 0, 0, 0, 400),
    ];
    for (label, r, g, b, motor, ms) in steps {
        eprintln!("[ds-haptic] {label}");
        let rep = ds5_usb_output(*r, *g, *b, *motor);
        device
            .write(&rep)
            .map_err(|e| format!("write output: {e}"))?;
        sleep(Duration::from_millis(*ms));
    }
    // Belt-and-braces final off so the pad never latches a colour/motor.
    let off = ds5_usb_output(0, 0, 0, 0);
    let _ = device.write(&off);
    eprintln!("[ds-haptic] done, pad restored to idle.");
    Ok(())
}

/// Open the first paired Sony pad and stream [`RawSample`]s to `sink` until the
/// read fails (e.g. unplug) or the caller is interrupted.
pub fn stream_raw<F>(mut sink: F) -> Result<(), String>
where
    F: FnMut(RawSample),
{
    let paired = DualSenseFactory::list_paired().map_err(|e| format!("list paired: {e}"))?;
    let first = paired.first().ok_or("no paired Sony pad found")?;
    let kind = first.kind;
    eprintln!(
        "[ds-raw] found {:?} pid=0x{:04X} iface={}, opening...",
        kind, first.pid, first.interface
    );

    let api = hid_api_singleton().map_err(|e| format!("hidapi init: {e}"))?;
    // `list_paired` stores the path as `format!("{:?}", path)`; reopen from the
    // live device list by matching pid + interface instead of round-tripping the
    // debug-formatted path (which is not a valid CString on Windows).
    let device = {
        let guard = api.lock().map_err(|_| "hidapi poisoned")?;
        let info = guard
            .device_list()
            .find(|i| {
                i.vendor_id() == crate::ids::SONY_VID
                    && i.product_id() == first.pid
                    && i.interface_number() == first.interface
            })
            .ok_or("paired pad vanished before open")?;
        guard
            .open_path(info.path())
            .map_err(|e| format!("open: {e}"))?
    };

    eprintln!("[ds-raw] streaming. Ctrl-C to stop.");
    let mut buf = [0u8; 128];
    let mut count: u64 = 0;
    let mut first_t: Option<Instant> = None;
    let mut prev_ts: Option<u32> = None;

    loop {
        let n = device
            .read_timeout(&mut buf, 500)
            .map_err(|e| format!("read: {e}"))?;
        if n == 0 {
            continue;
        }
        let now = Instant::now();
        let started = *first_t.get_or_insert(now);
        count += 1;
        let secs = now.duration_since(started).as_secs_f32().max(1e-3);

        let report = &buf[..n];
        let (gyro, accel, sensor_timestamp, ts_delta, ts_offset) =
            decode_layout(kind, report, &mut prev_ts);

        sink(RawSample {
            report_id: report[0],
            len: n,
            gyro,
            accel,
            sensor_timestamp,
            ts_delta,
            ts_offset,
            rate_hz: count as f32 / secs,
        });
    }
}

/// Pull the gyro/accel int16 triplets and the firmware sensor timestamp out of a
/// raw report using the same offsets the real parser uses. The timestamp sits
/// immediately after the accel block: a u32 LE for DualSense, and (a separate
/// layout) a u16 before the gyro for DualShock 4 — handled below.
/// `(gyro, accel, sensor_timestamp, ts_delta, ts_offset)` as decoded from one
/// raw report.
type DecodedLayout = ([i16; 3], [i16; 3], Option<u32>, Option<u32>, Option<usize>);

fn decode_layout(kind: ControllerKind, buf: &[u8], prev_ts: &mut Option<u32>) -> DecodedLayout {
    let Some(offsets) = ImuOffsets::for_report(kind, buf.len()) else {
        return ([0; 3], [0; 3], None, None, None);
    };
    let g = |o: usize| read_i16(buf, o);
    let gyro = [g(offsets.gyro), g(offsets.gyro + 2), g(offsets.gyro + 4)];
    let accel = [g(offsets.accel), g(offsets.accel + 2), g(offsets.accel + 4)];

    // DualSense: u32 LE sensor timestamp right after the 6-byte accel block.
    // DualShock 4: u16 LE timestamp sits 3 bytes before gyro (timestamp +
    // temperature byte), per hid-playstation.c — read it there and widen to u32.
    let (ts, ts_offset) = match kind {
        ControllerKind::DualSense | ControllerKind::DualSenseEdge => {
            let off = offsets.accel + 6;
            (read_u32(buf, off), Some(off))
        }
        ControllerKind::DualShock4 => {
            let off = offsets.gyro.wrapping_sub(3);
            (read_u16(buf, off).map(|v| v as u32), Some(off))
        }
    };

    let ts_delta = match (ts, *prev_ts) {
        (Some(cur), Some(prev)) => Some(cur.wrapping_sub(prev)),
        _ => None,
    };
    if let Some(cur) = ts {
        *prev_ts = Some(cur);
    }
    (gyro, accel, ts, ts_delta, ts_offset)
}

fn read_i16(buf: &[u8], off: usize) -> i16 {
    buf.get(off..off + 2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .unwrap_or(0)
}

fn read_u16(buf: &[u8], off: usize) -> Option<u16> {
    buf.get(off..off + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn read_u32(buf: &[u8], off: usize) -> Option<u32> {
    buf.get(off..off + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}
