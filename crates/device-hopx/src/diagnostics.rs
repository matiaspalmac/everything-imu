//! Hardware-characterisation helpers.
//!
//! Two one-shot diagnostics that talk to the tracker directly (not through the
//! `Device`/pipeline machinery) so they can run as CLI subcommands:
//!
//! - [`stream_raw`] streams the undecoded int16 IMU channels so scale, axis
//!   order, and sample rate can be measured empirically (the configured
//!   full-scale range / ODR cannot be read over BLE).
//! - [`probe_commands`] writes arbitrary single-byte commands to the NUS RX
//!   characteristic and records what the device sends back, mapping out the
//!   firmware's undocumented command set.

use std::pin::Pin;
use std::time::{Duration, Instant};

use btleplug::api::{
    Central, CharPropFlags, Characteristic, ConnectionParameterPreset, Manager as _,
    Peripheral as _, ScanFilter, ValueNotification, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures_util::future::FutureExt;
use futures_util::{Stream, StreamExt};

use crate::protocol::{self, RecordParser};

/// Owned notification stream as returned by `Peripheral::notifications`.
type NotifStream = Pin<Box<dyn Stream<Item = ValueNotification> + Send>>;

/// A connected tracker plus the two NUS characteristics and the write mode the
/// RX characteristic advertises. Built once by [`connect_triki`] and shared by
/// both diagnostics.
struct TrikiLink {
    peripheral: Peripheral,
    tx: Characteristic,
    rx: Characteristic,
    write_type: WriteType,
}

/// One undecoded record plus the running average record rate.
pub struct RawSample {
    pub seq: u8,
    /// Six int16 channels in wire order (offsets 2,4,6,8,10,12) — no scale, no
    /// axis mapping.
    pub channels: [i16; 6],
    /// Average records/second since streaming started (reveals the IMU ODR).
    pub rate_hz: f32,
}

/// The device's reply to a single probed command.
pub struct ProbeResult {
    /// The command byte that was written to the RX characteristic.
    pub cmd: u8,
    /// 1-based attempt index (commands are probed `repeats` times so a tester
    /// can see whether a reply is stable config or live-changing data).
    pub attempt: u8,
    /// Each notification payload received during the dwell window, in order.
    pub responses: Vec<Vec<u8>>,
    /// True when the link dropped during/after this command (e.g. `0x0a`).
    pub disconnected: bool,
}

/// Scan for a `Triki` tracker, connect, and stream [`RawSample`]s to `sink`
/// until the notification stream ends (e.g. the caller is interrupted or the
/// link drops).
pub async fn stream_raw<F>(mut sink: F) -> Result<(), String>
where
    F: FnMut(RawSample),
{
    let link = connect_triki().await?;
    link.peripheral
        .subscribe(&link.tx)
        .await
        .map_err(|e| format!("subscribe: {e}"))?;
    link.peripheral
        .write(&link.rx, &protocol::START_CMD, link.write_type)
        .await
        .map_err(|e| format!("start command: {e}"))?;
    eprintln!("[hopx-raw] streaming. Ctrl-C to stop.");

    let mut notifications = link
        .peripheral
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
    let _ = link
        .peripheral
        .write(&link.rx, &protocol::STOP_CMD, link.write_type)
        .await;
    let _ = link.peripheral.disconnect().await;
    Ok(())
}

/// Write each command byte in `cmds` to the RX characteristic `repeats` times,
/// collecting whatever the device notifies back within `dwell` after each write.
///
/// The firmware accepts single-byte commands the host app never sends (e.g.
/// `0x09` returns three slightly-varying messages; `0x0a` disconnects). This
/// walks a caller-chosen command set so those replies can be captured and
/// decoded. A command that drops the link is reported with `disconnected:
/// true`, and the probe transparently reconnects before continuing — which also
/// exercises the reconnect path the live driver needs.
pub async fn probe_commands<F>(
    cmds: &[u8],
    repeats: u8,
    dwell: Duration,
    mut sink: F,
) -> Result<(), String>
where
    F: FnMut(ProbeResult),
{
    let link = connect_triki().await?;
    link.peripheral
        .subscribe(&link.tx)
        .await
        .map_err(|e| format!("subscribe: {e}"))?;
    let mut notifications: NotifStream = link
        .peripheral
        .notifications()
        .await
        .map_err(|e| format!("notifications stream: {e}"))?;
    eprintln!("[hopx-probe] connected. Probing {} command(s).", cmds.len());

    'commands: for &cmd in cmds {
        for attempt in 1..=repeats.max(1) {
            // Flush anything still queued (e.g. tail of an IMU stream a prior
            // command started) so this result is attributable to `cmd` alone.
            drain_ready(&mut notifications);

            if let Err(e) = link
                .peripheral
                .write(&link.rx, &[cmd], link.write_type)
                .await
            {
                // A failed write almost always means the link is already gone.
                sink(ProbeResult {
                    cmd,
                    attempt,
                    responses: Vec::new(),
                    disconnected: true,
                });
                eprintln!("[hopx-probe] write 0x{cmd:02x} failed ({e}); reconnecting...");
                match reconnect(&link).await {
                    Ok(s) => notifications = s,
                    Err(re) => {
                        eprintln!("[hopx-probe] reconnect failed: {re}");
                        break 'commands;
                    }
                }
                continue;
            }

            let mut responses = Vec::new();
            let mut disconnected = false;
            let deadline = Instant::now() + dwell;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                match tokio::time::timeout(remaining, notifications.next()).await {
                    Ok(Some(n)) if n.uuid == protocol::NUS_TX_UUID => responses.push(n.value),
                    Ok(Some(_)) => {}
                    Ok(None) => {
                        // Stream end = the peripheral dropped the connection.
                        disconnected = true;
                        break;
                    }
                    Err(_) => break, // dwell elapsed
                }
            }

            sink(ProbeResult {
                cmd,
                attempt,
                responses,
                disconnected,
            });

            if disconnected {
                eprintln!("[hopx-probe] link dropped after 0x{cmd:02x}; reconnecting...");
                match reconnect(&link).await {
                    Ok(s) => notifications = s,
                    Err(re) => {
                        eprintln!("[hopx-probe] reconnect failed: {re}");
                        break 'commands;
                    }
                }
            }
        }
    }

    let _ = link.peripheral.disconnect().await;
    Ok(())
}

/// Parse a probe spec into the command bytes to send. Accepts a comma-separated
/// mix of single bytes and inclusive `lo-hi` ranges, each hex (`0x..`) or
/// decimal: `"0x00-0x1f"`, `"9,10,0x0a"`, `"0-15,0x20"`. Duplicates are kept so
/// a tester can list a command several times to probe it repeatedly.
pub fn parse_probe_spec(spec: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for tok in spec.split(',') {
        let tok = tok.trim();
        if tok.is_empty() {
            continue;
        }
        if let Some((lo, hi)) = tok.split_once('-') {
            let lo = parse_byte(lo.trim())?;
            let hi = parse_byte(hi.trim())?;
            if lo > hi {
                return Err(format!("range '{tok}': start > end"));
            }
            out.extend(lo..=hi);
        } else {
            out.push(parse_byte(tok)?);
        }
    }
    if out.is_empty() {
        return Err("empty command spec".into());
    }
    Ok(out)
}

/// Parse one byte as hex when `0x`-prefixed, else decimal; range-checked 0–255.
fn parse_byte(s: &str) -> Result<u8, String> {
    let value = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16)
    } else {
        s.parse::<u32>()
    }
    .map_err(|_| format!("invalid byte '{s}'"))?;
    u8::try_from(value).map_err(|_| format!("byte out of range 0-255: '{s}'"))
}

/// Drop every notification already buffered without awaiting new ones.
fn drain_ready(stream: &mut NotifStream) {
    while stream.next().now_or_never().flatten().is_some() {}
}

/// Run the full connect handshake and return the link to a `Triki` tracker.
async fn connect_triki() -> Result<TrikiLink, String> {
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
        "[hopx] found \"{name}\" ({}), connecting...",
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
    let write_type = if rx
        .properties
        .contains(CharPropFlags::WRITE_WITHOUT_RESPONSE)
    {
        WriteType::WithoutResponse
    } else {
        WriteType::WithResponse
    };
    Ok(TrikiLink {
        peripheral,
        tx,
        rx,
        write_type,
    })
}

/// Reconnect a previously-found peripheral, re-subscribe to TX, and hand back a
/// fresh notification stream. Avoids a full rescan since the handle is retained.
async fn reconnect(link: &TrikiLink) -> Result<NotifStream, String> {
    link.peripheral
        .connect_with_timeout(Duration::from_secs(20))
        .await
        .map_err(|e| format!("reconnect: {e}"))?;
    link.peripheral
        .discover_services_with_timeout(Duration::from_secs(20))
        .await
        .map_err(|e| format!("rediscover services: {e}"))?;
    link.peripheral
        .subscribe(&link.tx)
        .await
        .map_err(|e| format!("resubscribe: {e}"))?;
    link.peripheral
        .notifications()
        .await
        .map_err(|e| format!("notifications stream: {e}"))
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

#[cfg(test)]
mod tests {
    use super::parse_probe_spec;

    #[test]
    fn parses_inclusive_hex_range() {
        assert_eq!(parse_probe_spec("0x00-0x03").unwrap(), [0, 1, 2, 3]);
    }

    #[test]
    fn parses_decimal_range() {
        assert_eq!(parse_probe_spec("0-2,5").unwrap(), [0, 1, 2, 5]);
    }

    #[test]
    fn keeps_duplicates_for_repeat_probing() {
        assert_eq!(parse_probe_spec("9,9,0x0a").unwrap(), [9, 9, 10]);
    }

    #[test]
    fn single_hex_byte() {
        assert_eq!(parse_probe_spec("0x1f").unwrap(), [31]);
    }

    #[test]
    fn tolerates_whitespace_and_trailing_commas() {
        assert_eq!(parse_probe_spec(" 1 , 2 , ").unwrap(), [1, 2]);
    }

    #[test]
    fn rejects_out_of_range_byte() {
        assert!(parse_probe_spec("256").is_err());
    }

    #[test]
    fn rejects_reversed_range() {
        assert!(parse_probe_spec("0x05-0x03").is_err());
    }

    #[test]
    fn rejects_bare_hex_without_prefix() {
        // 'ff' has no 0x prefix, so it is parsed as decimal and rejected.
        assert!(parse_probe_spec("ff").is_err());
    }

    #[test]
    fn rejects_empty_spec() {
        assert!(parse_probe_spec("   ").is_err());
    }
}
