//! Live raw input-report diagnostic for PS Move (ZCM1 / ZCM2) bring-up.
//!
//! Invoked from the headless CLI via `--psmove-raw`. Dumps the raw report hex
//! plus the IMU triplets decoded at the *current* parser offsets, in several
//! interpretations (big-endian, big-endian minus the `0x8000` ZCM1 shift, and
//! little-endian), so the true byte layout, axis order, and scale can be
//! eyeballed against a known still + rotate motion before the parser is trusted
//! (plan items B1/B2/B4).
//!
//! Self-contained HID path (does not go through the `Device`/pipeline) so it can
//! run as a one-shot CLI diagnostic. Bring-up / validation only.

use crate::ids::{ControllerKind, SONY_VID};
use hidapi::HidApi;
use std::time::Instant;

// Offsets mirrored from `report.rs` (per the ref doc layout): accel
// frame-A at 13, gyro frame-A at 25, packed mag at 38. The shipped parser reads
// little-endian and, on ZCM1, subtracts the 0x8000 bias — the `LE-0x80`
// interpretation below is what it actually feeds fusion.
const OFS_FRAME_A_ACCEL: usize = 13;
const OFS_FRAME_A_GYRO: usize = 25;
const OFS_MAG: usize = 38;

/// Read the PS Move report stream and print decoded + raw IMU until the device
/// disconnects, the optional duration cap elapses, or the process is killed.
pub fn run(duration_secs: Option<u64>) -> Result<(), String> {
    let api = HidApi::new().map_err(|e| format!("hidapi init failed: {e}"))?;
    let (device, kind) = open_first_psmove(&api)?;

    device
        .set_blocking_mode(false)
        .map_err(|e| format!("set_blocking_mode failed: {e}"))?;

    println!("[psmove-raw] reading from {kind:?}");
    println!(
        "[psmove-raw] NOTE the IMU only streams over Bluetooth; over USB the bytes \
         are present but motion stays flat. Mag is ZCM1-only."
    );

    let started = Instant::now();
    let mut buf = [0u8; 64];
    let mut last_print = Instant::now();
    let mut packet_count: u64 = 0;

    loop {
        if let Some(cap) = duration_secs {
            if started.elapsed().as_secs() >= cap {
                break;
            }
        }
        let n = device
            .read_timeout(&mut buf, 100)
            .map_err(|e| format!("read failed: {e}"))?;
        if n == 0 {
            continue;
        }
        packet_count += 1;

        // Throttle to ~5 Hz; the controller streams ~175 Hz.
        if last_print.elapsed().as_millis() >= 200 {
            print_report(&buf[..n], kind, packet_count);
            last_print = Instant::now();
        }
    }
    Ok(())
}

fn open_first_psmove(api: &HidApi) -> Result<(hidapi::HidDevice, ControllerKind), String> {
    for info in api.device_list() {
        if info.vendor_id() != SONY_VID {
            continue;
        }
        let Some(kind) = ControllerKind::from_pid(info.product_id()) else {
            continue;
        };
        return api
            .open_path(info.path())
            .map(|d| (d, kind))
            .map_err(|e| format!("open failed: {e}"));
    }
    Err("no PS Move (ZCM1/ZCM2) found — pair over Bluetooth first".to_string())
}

fn read_i16_be(buf: &[u8], ofs: usize) -> Option<i16> {
    let b = buf.get(ofs..ofs + 2)?;
    Some(i16::from_be_bytes([b[0], b[1]]))
}

fn read_i16_le(buf: &[u8], ofs: usize) -> Option<i16> {
    let b = buf.get(ofs..ofs + 2)?;
    Some(i16::from_le_bytes([b[0], b[1]]))
}

fn print_report(buf: &[u8], kind: ControllerKind, count: u64) {
    let id = buf.first().copied().unwrap_or(0);
    println!(
        "--- packet {} ({} bytes, id=0x{:02X}) ---",
        count,
        buf.len(),
        id
    );
    print_hex(buf);

    // Frame-A accel/gyro at the parser offsets, multiple interpretations so the
    // real encoding stays verifiable from a single still + rotate capture. The
    // `LE-0x80` row is what the ZCM1 parser actually uses.
    print_triplet(
        "accelA LE-0x80",
        buf,
        OFS_FRAME_A_ACCEL,
        read_i16_le,
        0x8000,
    );
    print_triplet("accelA LE     ", buf, OFS_FRAME_A_ACCEL, read_i16_le, 0);
    print_triplet("accelA BE     ", buf, OFS_FRAME_A_ACCEL, read_i16_be, 0);
    print_triplet("gyroA  LE-0x80", buf, OFS_FRAME_A_GYRO, read_i16_le, 0x8000);
    print_triplet("gyroA  LE     ", buf, OFS_FRAME_A_GYRO, read_i16_le, 0);
    if kind.has_magnetometer() {
        print_triplet("mag    BE     ", buf, OFS_MAG, read_i16_be, 0);
    }
}

fn print_triplet(
    label: &str,
    buf: &[u8],
    ofs: usize,
    rd: fn(&[u8], usize) -> Option<i16>,
    bias: i32,
) {
    // With a 0x8000 bias the raw is the unsigned 16-bit value centred by the
    // bias (matching the ZCM1 parser); with no bias it is shown signed.
    let center = |v: i16| -> i32 {
        if bias != 0 {
            (v as u16 as i32) - bias
        } else {
            v as i32
        }
    };
    match (rd(buf, ofs), rd(buf, ofs + 2), rd(buf, ofs + 4)) {
        (Some(x), Some(y), Some(z)) => {
            let (x, y, z) = (center(x), center(y), center(z));
            println!("  {label} @{ofs:>2}: [{x:+6}, {y:+6}, {z:+6}]");
        }
        _ => println!("  {label} @{ofs:>2}: <short>"),
    }
}

fn print_hex(buf: &[u8]) {
    print!("  hex:");
    for (i, b) in buf.iter().enumerate() {
        if i % 16 == 0 && i != 0 {
            print!("\n      ");
        }
        print!(" {b:02X}");
    }
    println!();
}
