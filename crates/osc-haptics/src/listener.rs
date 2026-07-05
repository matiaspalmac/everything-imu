//! Async UDP runtime for the haptic bridge.

use crate::config::HapticConfig;
use crate::mapping::{osc_value_to_f32, resolve, HapticAction};
use crate::sniffer::Sniffer;
use rosc::{OscPacket, OscType};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

/// Thread-safe handle on the live OSC parameter sniffer.
///
/// Held by both the listener (which writes via `ingest`) and the UI / Tauri
/// command layer (which reads via `snapshot`). Wrapped in `std::sync::Mutex`
/// so callers from sync contexts (Tauri commands, tests) can grab a snapshot
/// without entering an async runtime.
pub type SnifferHandle = Arc<StdMutex<Sniffer>>;

/// Minimum gap between rumble writes to the same device. Caps Bluetooth
/// traffic (~66 Hz) so a flood of proximity updates can't disconnect a pad.
const MIN_SEND_GAP: Duration = Duration::from_millis(15);

/// Intensity change below this is treated as noise and not re-sent.
const INTENSITY_EPSILON: f32 = 0.03;

/// If no OSC packet arrives for this long, every active motor is forced off
/// (VRChat closed, OSC disabled, or avatar swapped).
const WATCHDOG_IDLE: Duration = Duration::from_secs(2);

/// Coarsest tick — bounds how late a pulse expiry or watchdog can fire.
const MAX_TICK: Duration = Duration::from_millis(250);

/// Upper bound on distinct OSC addresses tracked for discovery. Untrusted UDP
/// could otherwise grow `seen_addrs` without limit; past this we stop tracking
/// new addresses.
const MAX_SEEN_ADDRS: usize = 4096;

/// Where the bridge sends resolved rumble commands.
///
/// Implemented by the application (`AppState`) so this crate does not depend
/// on `core`.
#[async_trait::async_trait]
pub trait RumbleSink: Send + Sync {
    async fn set_rumble(&self, mac: [u8; 6], intensity: f32);
}

/// Run the haptic bridge until `config_rx` is dropped.
///
/// Re-binds the UDP socket whenever the port changes and idles (no socket)
/// while `enabled` is false. `discovery_tx`, if provided, receives every
/// distinct OSC address seen — the config UI uses it for live binding.
pub async fn run_bridge(
    config_rx: watch::Receiver<HapticConfig>,
    sink: Arc<dyn RumbleSink>,
    discovery_tx: Option<mpsc::Sender<String>>,
) {
    run_bridge_with_sniffer(config_rx, sink, discovery_tx, None).await
}

/// Same as [`run_bridge`] but with an optional [`SnifferHandle`] that the
/// listener writes every routed message into. Construct via
/// `Arc::new(StdMutex::new(Sniffer::new(512)))` and pass the same handle to
/// the UI command layer for live `snapshot()` reads.
pub async fn run_bridge_with_sniffer(
    mut config_rx: watch::Receiver<HapticConfig>,
    sink: Arc<dyn RumbleSink>,
    discovery_tx: Option<mpsc::Sender<String>>,
    sniffer: Option<SnifferHandle>,
) {
    loop {
        let config = config_rx.borrow_and_update().clone();
        if !config.enabled {
            // Idle until the config changes; bail out if the sender is gone.
            if config_rx.changed().await.is_err() {
                return;
            }
            continue;
        }

        // Bind loopback only: OSC haptics come from a locally-running VRChat.
        // Binding all interfaces would let any LAN host drive the physical
        // rumble motors.
        let socket = match UdpSocket::bind(("127.0.0.1", config.listen_port)).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(port = config.listen_port, error = %e,
                    "haptic bridge: UDP bind failed; retrying on next config change");
                if config_rx.changed().await.is_err() {
                    return;
                }
                continue;
            }
        };
        tracing::info!(
            port = config.listen_port,
            rules = config.rules.len(),
            "haptic bridge listening"
        );

        let rebind = serve(
            &socket,
            &mut config_rx,
            &sink,
            discovery_tx.as_ref(),
            sniffer.as_ref(),
        )
        .await;
        match rebind {
            ServeExit::Shutdown => return,
            ServeExit::Rebind => continue,
        }
    }
}

enum ServeExit {
    /// `config_rx` dropped — stop entirely.
    Shutdown,
    /// Config changed — rebind the socket.
    Rebind,
}

/// Inner loop for one bound socket. Returns when the config changes (so the
/// caller can rebind) or the config sender is dropped.
async fn serve(
    socket: &UdpSocket,
    config_rx: &mut watch::Receiver<HapticConfig>,
    sink: &Arc<dyn RumbleSink>,
    discovery_tx: Option<&mpsc::Sender<String>>,
    sniffer: Option<&SnifferHandle>,
) -> ServeExit {
    let mut state = LoopState::new();
    // Sized well above a typical OSC datagram so oversized bundles aren't
    // silently truncated; see the `len == buf.len()` check below.
    let mut buf = [0u8; 8192];
    // Rules are immutable for the lifetime of a single serve() — any change
    // returns ServeExit::Rebind and re-enters this function. Snapshot once
    // here instead of cloning the Vec on every received packet (VRChat
    // proximity streams can hit hundreds of packets per second).
    let rules = config_rx.borrow().rules.clone();

    loop {
        let now = Instant::now();
        let tick = state.next_wake(now);

        tokio::select! {
            biased;

            changed = config_rx.changed() => {
                if changed.is_err() {
                    state.silence_all(sink).await;
                    return ServeExit::Shutdown;
                }
                return ServeExit::Rebind;
            }

            recv = socket.recv_from(&mut buf) => {
                match recv {
                    Ok((len, _addr)) => {
                        if len == buf.len() {
                            tracing::debug!(len, "haptic bridge: datagram filled recv buffer; may be truncated");
                        }
                        state.last_packet = Instant::now();
                        state.idle_silenced = false;
                        if let Ok((_, packet)) = rosc::decoder::decode_udp(&buf[..len]) {
                            handle_packet(&packet, &rules, &mut state, sink, discovery_tx, sniffer).await;
                        }
                    }
                    Err(e) => {
                        tracing::debug!(error = %e, "haptic bridge: recv error");
                    }
                }
            }

            _ = tokio::time::sleep(tick) => {
                state.on_tick(Instant::now(), sink).await;
            }
        }
    }
}

/// Mutable state owned exclusively by the [`serve`] loop — no locks needed.
struct LoopState {
    /// Last value sent to each device: `(when, intensity)`.
    last_sent: HashMap<[u8; 6], (Instant, f32)>,
    /// Devices currently mid-pulse and when the pulse ends.
    pulse_until: HashMap<[u8; 6], Instant>,
    /// Distinct OSC addresses already forwarded to discovery.
    seen_addrs: std::collections::HashSet<String>,
    last_packet: Instant,
    /// Set once the idle watchdog has silenced everything, so `next_wake`
    /// stops short-cycling at ~1 kHz until the next packet arrives.
    idle_silenced: bool,
}

impl LoopState {
    fn new() -> Self {
        Self {
            last_sent: HashMap::new(),
            pulse_until: HashMap::new(),
            seen_addrs: std::collections::HashSet::new(),
            last_packet: Instant::now(),
            idle_silenced: false,
        }
    }

    /// How long the loop may sleep before it must service a pulse expiry or
    /// the watchdog.
    fn next_wake(&self, now: Instant) -> Duration {
        let mut wake = MAX_TICK;
        for end in self.pulse_until.values() {
            wake = wake.min(end.saturating_duration_since(now));
        }
        if !self.idle_silenced {
            let watchdog = (self.last_packet + WATCHDOG_IDLE).saturating_duration_since(now);
            wake = wake.min(watchdog);
        }
        wake.max(Duration::from_millis(1))
    }

    /// Send `intensity` to `mac` if rate-limiting and the epsilon filter allow
    /// it. `force` bypasses both (used for pulse-off and watchdog).
    async fn send(
        &mut self,
        mac: [u8; 6],
        intensity: f32,
        force: bool,
        sink: &Arc<dyn RumbleSink>,
        now: Instant,
    ) {
        if !force {
            if let Some((when, prev)) = self.last_sent.get(&mac) {
                let too_soon = now.duration_since(*when) < MIN_SEND_GAP;
                let unchanged = (intensity - prev).abs() < INTENSITY_EPSILON;
                // Always let a return-to-zero through so a motor never sticks.
                if (too_soon || unchanged) && intensity > 0.0 {
                    return;
                }
            }
        }
        self.last_sent.insert(mac, (now, intensity));
        sink.set_rumble(mac, intensity).await;
    }

    /// Expire finished pulses and run the idle watchdog.
    async fn on_tick(&mut self, now: Instant, sink: &Arc<dyn RumbleSink>) {
        let expired: Vec<[u8; 6]> = self
            .pulse_until
            .iter()
            .filter(|(_, end)| **end <= now)
            .map(|(mac, _)| *mac)
            .collect();
        for mac in expired {
            self.pulse_until.remove(&mac);
            self.send(mac, 0.0, true, sink, now).await;
        }

        if !self.idle_silenced && now.duration_since(self.last_packet) >= WATCHDOG_IDLE {
            self.silence_all(sink).await;
            self.idle_silenced = true;
        }
    }

    /// Force every device with a non-zero last value back to silence.
    async fn silence_all(&mut self, sink: &Arc<dyn RumbleSink>) {
        let now = Instant::now();
        let active: Vec<[u8; 6]> = self
            .last_sent
            .iter()
            .filter(|(_, (_, v))| *v > 0.0)
            .map(|(mac, _)| *mac)
            .collect();
        for mac in active {
            self.pulse_until.remove(&mac);
            self.send(mac, 0.0, true, sink, now).await;
        }
    }
}

async fn handle_packet(
    packet: &OscPacket,
    rules: &[crate::config::HapticRule],
    state: &mut LoopState,
    sink: &Arc<dyn RumbleSink>,
    discovery_tx: Option<&mpsc::Sender<String>>,
    sniffer: Option<&SnifferHandle>,
) {
    match packet {
        OscPacket::Message(msg) => {
            if let Some(tx) = discovery_tx {
                // Cap discovery tracking so untrusted UDP can't grow this set
                // without bound.
                if state.seen_addrs.len() < MAX_SEEN_ADDRS
                    && state.seen_addrs.insert(msg.addr.clone())
                {
                    let _ = tx.try_send(msg.addr.clone());
                }
            }
            // Per-packet trace so users can confirm VRChat is reaching us
            // and whether their rule's address actually matches. Gated at
            // trace level so it doesn't flood at info; users debugging set
            // RUST_LOG=osc_haptics=trace.
            tracing::trace!(
                addr = %msg.addr,
                args = ?msg.args,
                "haptic osc: received"
            );
            let Some(arg) = msg.args.first() else {
                tracing::trace!(addr = %msg.addr, "haptic osc: message with no args, ignored");
                return;
            };
            let Some(value) = osc_arg_value(arg) else {
                tracing::trace!(
                    addr = %msg.addr,
                    arg = ?arg,
                    "haptic osc: unsupported arg type, ignored"
                );
                return;
            };
            // Feed the live sniffer so the UI can render exactly which
            // addresses VRChat is sending and their numeric range. Done
            // *before* the rule resolve so users see addresses even when
            // no rule matches yet — that's the whole point of the sniffer.
            if let Some(handle) = sniffer {
                if let Ok(mut s) = handle.lock() {
                    s.ingest(&msg.addr, value);
                }
            }
            let now = Instant::now();
            let actions = resolve(rules, &msg.addr, value);
            if actions.is_empty() {
                tracing::trace!(
                    addr = %msg.addr,
                    value,
                    "haptic osc: no matching rule"
                );
            } else {
                tracing::debug!(
                    addr = %msg.addr,
                    value,
                    matched = actions.len(),
                    "haptic osc: matched rule(s)"
                );
            }
            for action in actions {
                apply_action(action, state, sink, now).await;
            }
        }
        OscPacket::Bundle(bundle) => {
            for inner in &bundle.content {
                Box::pin(handle_packet(
                    inner,
                    rules,
                    state,
                    sink,
                    discovery_tx,
                    sniffer,
                ))
                .await;
            }
        }
    }
}

fn osc_arg_value(arg: &OscType) -> Option<f32> {
    osc_value_to_f32(arg)
}

async fn apply_action(
    action: HapticAction,
    state: &mut LoopState,
    sink: &Arc<dyn RumbleSink>,
    now: Instant,
) {
    match action.pulse_ms {
        Some(ms) => {
            state
                .pulse_until
                .insert(action.device_mac, now + Duration::from_millis(ms as u64));
            state
                .send(action.device_mac, action.intensity, true, sink, now)
                .await;
        }
        None => {
            // A live proximity value overrides any pending pulse.
            state.pulse_until.remove(&action.device_mac);
            state
                .send(action.device_mac, action.intensity, false, sink, now)
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HapticMode, HapticRule};
    use std::sync::Mutex;
    use tokio::sync::mpsc;

    /// Records every rumble command for assertions.
    struct RecordingSink {
        calls: Mutex<Vec<([u8; 6], f32)>>,
        tx: mpsc::UnboundedSender<()>,
    }

    #[async_trait::async_trait]
    impl RumbleSink for RecordingSink {
        async fn set_rumble(&self, mac: [u8; 6], intensity: f32) {
            self.calls.lock().unwrap().push((mac, intensity));
            let _ = self.tx.send(());
        }
    }

    fn encode_msg(addr: &str, value: f32) -> Vec<u8> {
        rosc::encoder::encode(&OscPacket::Message(rosc::OscMessage {
            addr: addr.into(),
            args: vec![OscType::Float(value)],
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn osc_message_drives_the_sink() {
        let mac = [0x02, 0, 0, 0, 0, 0x99];
        let config = HapticConfig {
            enabled: true,
            listen_port: 0, // ephemeral port — assigned by the OS
            rules: vec![HapticRule {
                osc_address: "/avatar/parameters/Touch".into(),
                device_mac: mac,
                mode: HapticMode::Proximity {
                    gain: 1.0,
                    min_threshold: 0.05,
                },
            }],
        };

        // Bind the listener socket ourselves so the test knows the port.
        let socket = UdpSocket::bind(("127.0.0.1", 0)).await.unwrap();
        let port = socket.local_addr().unwrap().port();

        let (signal_tx, mut signal_rx) = mpsc::unbounded_channel();
        let sink = Arc::new(RecordingSink {
            calls: Mutex::new(Vec::new()),
            tx: signal_tx,
        });
        let (_cfg_tx, cfg_rx) = watch::channel(config);
        let sink_clone: Arc<dyn RumbleSink> = sink.clone();

        tokio::spawn(async move {
            serve(&socket, &mut cfg_rx.clone(), &sink_clone, None, None).await;
        });

        // Send a proximity hit from a separate socket.
        let client = UdpSocket::bind(("127.0.0.1", 0)).await.unwrap();
        client
            .send_to(
                &encode_msg("/avatar/parameters/Touch", 0.8),
                ("127.0.0.1", port),
            )
            .await
            .unwrap();

        tokio::time::timeout(Duration::from_secs(1), signal_rx.recv())
            .await
            .expect("sink was never called");

        let calls = sink.calls.lock().unwrap();
        assert_eq!(calls[0], (mac, 0.8));
    }

    #[tokio::test]
    async fn sniffer_records_addresses_even_without_matching_rule() {
        // Rule that matches nothing the test sends. Sniffer must still
        // record both addresses so the UI can show "VRChat is talking, no
        // rule matches".
        let mac = [0x02, 0, 0, 0, 0, 0x99];
        let config = HapticConfig {
            enabled: true,
            listen_port: 0,
            rules: vec![HapticRule {
                osc_address: "/avatar/parameters/NeverFires".into(),
                device_mac: mac,
                mode: HapticMode::Proximity {
                    gain: 1.0,
                    min_threshold: 0.05,
                },
            }],
        };

        let socket = UdpSocket::bind(("127.0.0.1", 0)).await.unwrap();
        let port = socket.local_addr().unwrap().port();

        let (signal_tx, _signal_rx) = mpsc::unbounded_channel();
        let sink = Arc::new(RecordingSink {
            calls: Mutex::new(Vec::new()),
            tx: signal_tx,
        });
        let (_cfg_tx, cfg_rx) = watch::channel(config);
        let sink_clone: Arc<dyn RumbleSink> = sink.clone();
        let sniffer: SnifferHandle = Arc::new(StdMutex::new(Sniffer::new(32)));
        let sniffer_clone = sniffer.clone();

        tokio::spawn(async move {
            serve(
                &socket,
                &mut cfg_rx.clone(),
                &sink_clone,
                None,
                Some(&sniffer_clone),
            )
            .await;
        });

        let client = UdpSocket::bind(("127.0.0.1", 0)).await.unwrap();
        for (addr, val) in [
            ("/avatar/parameters/A", 0.1f32),
            ("/avatar/parameters/B", 0.5),
            ("/avatar/parameters/A", 0.9),
        ] {
            client
                .send_to(&encode_msg(addr, val), ("127.0.0.1", port))
                .await
                .unwrap();
        }

        // Spin until the sniffer has both addresses or we time out.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            {
                let s = sniffer.lock().unwrap();
                if s.len() >= 2 {
                    break;
                }
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("sniffer never captured both addresses");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let snap = sniffer.lock().unwrap().snapshot();
        let by_addr: HashMap<_, _> = snap
            .iter()
            .map(|e| (e.address.clone(), e.clone()))
            .collect();
        let a = by_addr
            .get("/avatar/parameters/A")
            .expect("addr A captured");
        assert_eq!(a.count, 2, "two A packets");
        assert!((a.min_value - 0.1).abs() < 1e-4);
        assert!((a.max_value - 0.9).abs() < 1e-4);
        let b = by_addr
            .get("/avatar/parameters/B")
            .expect("addr B captured");
        assert_eq!(b.count, 1);
    }
}
