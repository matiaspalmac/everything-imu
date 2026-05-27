//! Async UDP runtime for the haptic bridge.

use crate::config::HapticConfig;
use crate::mapping::{osc_value_to_f32, resolve, HapticAction};
use rosc::{OscPacket, OscType};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

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
    mut config_rx: watch::Receiver<HapticConfig>,
    sink: Arc<dyn RumbleSink>,
    discovery_tx: Option<mpsc::Sender<String>>,
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

        let socket = match UdpSocket::bind(("0.0.0.0", config.listen_port)).await {
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

        let rebind = serve(&socket, &mut config_rx, &sink, discovery_tx.as_ref()).await;
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
) -> ServeExit {
    let mut state = LoopState::new();
    let mut buf = [0u8; 2048];
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
                        state.last_packet = Instant::now();
                        if let Ok((_, packet)) = rosc::decoder::decode_udp(&buf[..len]) {
                            handle_packet(&packet, &rules, &mut state, sink, discovery_tx).await;
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
}

impl LoopState {
    fn new() -> Self {
        Self {
            last_sent: HashMap::new(),
            pulse_until: HashMap::new(),
            seen_addrs: std::collections::HashSet::new(),
            last_packet: Instant::now(),
        }
    }

    /// How long the loop may sleep before it must service a pulse expiry or
    /// the watchdog.
    fn next_wake(&self, now: Instant) -> Duration {
        let mut wake = MAX_TICK;
        for end in self.pulse_until.values() {
            wake = wake.min(end.saturating_duration_since(now));
        }
        let watchdog = (self.last_packet + WATCHDOG_IDLE).saturating_duration_since(now);
        wake.min(watchdog).max(Duration::from_millis(1))
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

        if now.duration_since(self.last_packet) >= WATCHDOG_IDLE {
            self.silence_all(sink).await;
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
) {
    match packet {
        OscPacket::Message(msg) => {
            if let Some(tx) = discovery_tx {
                if state.seen_addrs.insert(msg.addr.clone()) {
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
                Box::pin(handle_packet(inner, rules, state, sink, discovery_tx)).await;
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
            serve(&socket, &mut cfg_rx.clone(), &sink_clone, None).await;
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
}
