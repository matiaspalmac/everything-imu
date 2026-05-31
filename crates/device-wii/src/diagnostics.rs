//! Live raw-packet diagnostic for the Wii forwarder (`--wii-raw`).
//!
//! Binds the same TCP listener the real factory uses, accepts the first
//! companion connection, and prints each decoded 17-byte record (raw + scaled
//! accel/gyro and the extension/battery flags) so the wire format and IMU scale
//! can be eyeballed during bring-up. Bring-up / validation only — bypasses the
//! `Device` pipeline. Sends a neutral 5-byte reply (no rumble, 16 ms interval)
//! so the companion keeps streaming.

use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const RECORD_BYTES: usize = 17;
const ACCEL_ZERO_G: f32 = 512.0;
const ACCEL_LSB_PER_G: f32 = 200.0;
const GYRO_DPS_PER_LSB: f32 = 0.07;

/// Listen on `bind`, decode forwarded packets, and print until the connection
/// closes or `duration_secs` elapses.
pub async fn run(bind: &str, duration_secs: Option<u64>) -> Result<(), String> {
    let listener = TcpListener::bind(bind)
        .await
        .map_err(|e| format!("wii-raw bind {bind} failed: {e}"))?;
    println!("[wii-raw] listening on {bind} — launch the Wii companion now");

    let (mut stream, peer) = listener
        .accept()
        .await
        .map_err(|e| format!("wii-raw accept failed: {e}"))?;
    println!("[wii-raw] companion connected from {peer}");

    let started = Instant::now();
    let mut buf = vec![0u8; RECORD_BYTES * 32];
    let mut last_print = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or_else(Instant::now);

    loop {
        if let Some(cap) = duration_secs {
            if started.elapsed().as_secs() >= cap {
                break;
            }
        }
        let n = stream
            .read(&mut buf)
            .await
            .map_err(|e| format!("wii-raw read failed: {e}"))?;
        if n == 0 {
            println!("[wii-raw] companion disconnected");
            break;
        }
        if n % RECORD_BYTES == 0 && last_print.elapsed().as_millis() >= 200 {
            for rec in buf[..n].chunks_exact(RECORD_BYTES) {
                print_record(rec);
            }
            last_print = Instant::now();
        }
        // Neutral reply: no rumble, 16 ms frame interval.
        let reply = [0u8, 0, 0, 0, 16];
        stream
            .write_all(&reply)
            .await
            .map_err(|e| format!("wii-raw reply failed: {e}"))?;
    }
    Ok(())
}

fn rd(buf: &[u8], o: usize) -> i16 {
    i16::from_le_bytes([buf[o], buf[o + 1]])
}

fn print_record(buf: &[u8]) {
    let id = buf[0];
    if id == 0xFF {
        return; // empty slot
    }
    let accel = [rd(buf, 1), rd(buf, 3), rd(buf, 5)];
    let data = [rd(buf, 7), rd(buf, 9), rd(buf, 11)];
    let nunchuk = buf[13];
    let mp = buf[14];
    let battery = buf[15];
    let button = buf[16];

    let accel_g = |r: i16| (r as f32 - ACCEL_ZERO_G) / ACCEL_LSB_PER_G;
    let gyro_dps = |r: i16| (r as f32 - 8192.0) * GYRO_DPS_PER_LSB;

    println!(
        "slot {id} accel raw[{:+6},{:+6},{:+6}] g[{:+.2},{:+.2},{:+.2}] | data raw[{:+6},{:+6},{:+6}]{} | nun={nunchuk} mp={mp} batt={battery} btn={button}",
        accel[0], accel[1], accel[2],
        accel_g(accel[0]), accel_g(accel[1]), accel_g(accel[2]),
        data[0], data[1], data[2],
        if nunchuk == 0 {
            format!(
                " gyro dps[{:+.1},{:+.1},{:+.1}]",
                gyro_dps(data[0]), gyro_dps(data[1]), gyro_dps(data[2])
            )
        } else {
            " (nunchuk accel)".to_string()
        },
    );
}
