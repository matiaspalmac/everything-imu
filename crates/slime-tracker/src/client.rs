//! Async UDP client with BUNDLE auto-fallback gating.
//!
//! [`SlimeClient`] owns a tokio [`UdpSocket`], spawns a background receive loop
//! to parse FEATURE_FLAGS / PING / handshake replies, and exposes a hot-path
//! `send_rotation_and_accel` that picks BUNDLE (packet 100) or two separate
//! sends (rotation 17 + accel 4) based on the latest server FEATURE_FLAGS reply.
//!
//! State machine implements the v0.4.1 fix from
//! [`memory/feedback_slimevr_bundle.md`](../../memory/feedback_slimevr_bundle.md):
//! `server_supports_bundle` defaults to `false` until the server's FEATURE_FLAGS
//! reply lands — packets emitted before the reply automatically take the
//! two-send fallback path, so no data is silently dropped on legacy servers.

use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use deku::DekuContainerWrite;
use tokio::net::UdpSocket;
use tokio::sync::broadcast;

/// Push notification emitted whenever the handshake-confirmed flag flips.
/// `true` means the server has just acknowledged the handshake (or PINGed
/// after a previous disconnect); `false` means the watchdog tripped after
/// 2 s of silence.
///
/// Subscribers obtain a receiver via [`SlimeClient::subscribe_handshake`].
/// The UI uses this to surface toasts and to highlight which device tile
/// just lost connectivity, without having to poll [`ClientStats`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandshakeEvent {
    pub confirmed: bool,
    /// Same value as [`ClientStats::handshake_reset_count`] at the moment
    /// the event was emitted. Useful to deduplicate replayed events.
    pub reset_count: u64,
}

/// Broadcast buffer depth — small because consumers should never lag the
/// producer for more than a couple of transitions; 32 absorbs UI render
/// stalls without growing memory.
const HANDSHAKE_EVENT_CAPACITY: usize = 32;

use crate::clientbound::server_feature_flag_bits;
use crate::{
    encode_bundle, BoardType, ImuType, McuType, Packet, SbPacket, SensorDataType, SensorStatus,
    SlimeQuaternion, SlimeString, TrackerDataType, TrackerPosition, BUNDLE_TAG,
};

/// Identification + handshake metadata sent in every (re)handshake.
#[derive(Debug, Clone)]
pub struct HandshakeInfo {
    pub board: BoardType,
    pub imu: ImuType,
    pub mcu: McuType,
    /// `imu_info[0]` slot — used by SlimeIMU v0.4.x to carry magnetometer status
    /// (`0 = NotSupported`, `1 = Disabled`, `2 = Enabled`).
    pub mag_status: i32,
    pub firmware: String,
    pub mac_address: [u8; 6],
}

/// Per-sensor description sent in [`SbPacket::SensorInfo`] after the handshake
/// reply lands.
#[derive(Debug, Clone)]
pub struct SensorDescriptor {
    pub sensor_id: u8,
    pub imu_type: ImuType,
    /// `sensor_config` bitmask: `0x0003` mag enabled, `0x0002` mag supported
    /// but disabled, `0x0000` not supported.
    pub mag_config: u16,
    pub position: TrackerPosition,
    pub data_type: TrackerDataType,
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("deku encode error: {0:?}")]
    Encode(deku::DekuError),
}

impl From<deku::DekuError> for ClientError {
    fn from(e: deku::DekuError) -> Self {
        Self::Encode(e)
    }
}

/// Tokio-backed UDP client with automatic BUNDLE / two-send fallback.
///
/// Construct via [`SlimeClient::connect`]. The returned value owns a background
/// task that runs the receive loop until the client is dropped.
/// Snapshot of [`SlimeClient`] runtime stats. Returned by
/// [`SlimeClient::stats`] for diagnostics / UI consumption.
///
/// All `*_ms_unix` fields are `0` when the corresponding event has not
/// happened yet. Callers compute "ms since last X" from
/// `now.duration_since(UNIX_EPOCH)` to keep the API time-zone free.
#[derive(Debug, Clone, Copy)]
pub struct ClientStats {
    pub packets_sent: u64,
    pub last_send_ms_unix: u64,
    pub last_handshake_ms_unix: u64,
    pub server_supports_bundle: bool,
    /// `true` once the server has replied with any inbound packet
    /// (FEATURE_FLAGS or PING). Until then the watchdog keeps re-sending
    /// the handshake every 5 s.
    pub handshake_confirmed: bool,
    pub last_inbound_ms_unix: u64,
    /// Number of times the connection-lost watchdog has flipped
    /// `handshake_confirmed` back to `false` after a previous successful
    /// handshake. A non-zero, increasing value tells the UI that the server
    /// is intermittent rather than absent.
    pub handshake_reset_count: u64,
    /// `Instant`-millis when the most recent reset occurred (0 = never).
    pub last_reset_ms_unix: u64,
}

pub struct SlimeClient {
    socket: Arc<UdpSocket>,
    seq: Arc<AtomicU64>,
    server_supports_bundle: Arc<AtomicBool>,
    handshake_confirmed: Arc<AtomicBool>,
    packets_sent: Arc<AtomicU64>,
    last_send_ms_unix: Arc<AtomicU64>,
    last_handshake_ms_unix: Arc<AtomicU64>,
    last_inbound_ms_unix: Arc<AtomicU64>,
    handshake_reset_count: Arc<AtomicU64>,
    last_reset_ms_unix: Arc<AtomicU64>,
    handshake_events: broadcast::Sender<HandshakeEvent>,
    receive_task: tokio::task::JoinHandle<()>,
    watchdog_task: tokio::task::JoinHandle<()>,
}

impl Drop for SlimeClient {
    fn drop(&mut self) {
        // JoinHandle::drop does NOT abort the underlying task — without an
        // explicit abort, the receive and watchdog loops outlive every
        // `SlimeClient` and leak forever. Each device reconnect would
        // accumulate two more zombie tasks (recv + watchdog) per cycle.
        self.receive_task.abort();
        self.watchdog_task.abort();
    }
}

fn now_ms_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl SlimeClient {
    /// Bind a local UDP socket, connect it to `server_addr`, send the initial
    /// handshake, and spawn the background receive loop. The receive loop
    /// updates [`SlimeClient::server_supports_bundle`] when the server's
    /// FEATURE_FLAGS reply arrives.
    pub async fn connect(
        server_addr: SocketAddr,
        info: &HandshakeInfo,
    ) -> Result<Self, ClientError> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(server_addr).await?;
        let socket = Arc::new(socket);
        let seq = Arc::new(AtomicU64::new(0));
        let server_supports_bundle = Arc::new(AtomicBool::new(false));
        let handshake_confirmed = Arc::new(AtomicBool::new(false));
        let packets_sent = Arc::new(AtomicU64::new(0));
        let last_send_ms_unix = Arc::new(AtomicU64::new(0));
        let last_handshake_ms_unix = Arc::new(AtomicU64::new(0));
        let last_inbound_ms_unix = Arc::new(AtomicU64::new(0));
        let handshake_reset_count = Arc::new(AtomicU64::new(0));
        let last_reset_ms_unix = Arc::new(AtomicU64::new(0));
        let (handshake_events, _) = broadcast::channel(HANDSHAKE_EVENT_CAPACITY);

        // Send initial handshake before spawning the receive loop so the
        // socket has something to receive.
        send_handshake_inner(&socket, &seq, info).await?;
        last_handshake_ms_unix.store(now_ms_unix(), Ordering::Release);
        packets_sent.fetch_add(1, Ordering::Relaxed);
        last_send_ms_unix.store(now_ms_unix(), Ordering::Release);

        let receive_task = tokio::spawn(receive_loop(
            socket.clone(),
            server_supports_bundle.clone(),
            handshake_confirmed.clone(),
            last_inbound_ms_unix.clone(),
            handshake_reset_count.clone(),
            handshake_events.clone(),
        ));

        let info_arc = Arc::new(info.clone());
        let watchdog_task = tokio::spawn(handshake_watchdog(
            socket.clone(),
            seq.clone(),
            info_arc,
            server_supports_bundle.clone(),
            handshake_confirmed.clone(),
            last_handshake_ms_unix.clone(),
            packets_sent.clone(),
            last_send_ms_unix.clone(),
            last_inbound_ms_unix.clone(),
            handshake_reset_count.clone(),
            last_reset_ms_unix.clone(),
            handshake_events.clone(),
        ));

        Ok(Self {
            socket,
            seq,
            server_supports_bundle,
            handshake_confirmed,
            packets_sent,
            last_send_ms_unix,
            last_handshake_ms_unix,
            last_inbound_ms_unix,
            handshake_reset_count,
            last_reset_ms_unix,
            handshake_events,
            receive_task,
            watchdog_task,
        })
    }

    /// Subscribe to handshake-state transitions. Each subscriber gets its
    /// own receiver; lagged consumers see [`broadcast::error::RecvError::Lagged`]
    /// and may drop events.
    pub fn subscribe_handshake(&self) -> broadcast::Receiver<HandshakeEvent> {
        self.handshake_events.subscribe()
    }

    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    /// Returns whether the most recent FEATURE_FLAGS reply from the server
    /// advertised `PROTOCOL_BUNDLE_SUPPORT`. Defaults to `false` before the
    /// reply lands.
    pub fn server_supports_bundle(&self) -> bool {
        self.server_supports_bundle.load(Ordering::Acquire)
    }

    /// Snapshot of runtime stats for diagnostics. Cheap (only atomic loads).
    pub fn stats(&self) -> ClientStats {
        ClientStats {
            packets_sent: self.packets_sent.load(Ordering::Relaxed),
            last_send_ms_unix: self.last_send_ms_unix.load(Ordering::Acquire),
            last_handshake_ms_unix: self.last_handshake_ms_unix.load(Ordering::Acquire),
            server_supports_bundle: self.server_supports_bundle.load(Ordering::Acquire),
            handshake_confirmed: self.handshake_confirmed.load(Ordering::Acquire),
            last_inbound_ms_unix: self.last_inbound_ms_unix.load(Ordering::Acquire),
            handshake_reset_count: self.handshake_reset_count.load(Ordering::Relaxed),
            last_reset_ms_unix: self.last_reset_ms_unix.load(Ordering::Acquire),
        }
    }

    /// Internal: record one outgoing UDP datagram. Called by every `send_*`
    /// path after `socket.send` returns Ok. Counts the packets-sent stat and
    /// updates the last-send timestamp shown in the diagnostics panel.
    fn record_sent(&self) {
        self.packets_sent.fetch_add(1, Ordering::Relaxed);
        self.last_send_ms_unix
            .store(now_ms_unix(), Ordering::Release);
    }

    async fn send_packet(&self, bytes: &[u8]) -> Result<(), ClientError> {
        match self.socket.send(bytes).await {
            Ok(_) => {
                self.record_sent();
                Ok(())
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionRefused
                ) =>
            {
                Ok(())
            }
            Err(e) => Err(ClientError::Io(e)),
        }
    }

    /// Re-send the handshake (used by the watchdog on connection timeout, or
    /// when explicitly forcing a re-handshake).
    pub async fn send_handshake(&self, info: &HandshakeInfo) -> Result<(), ClientError> {
        send_handshake_inner(&self.socket, &self.seq, info).await?;
        self.last_handshake_ms_unix
            .store(now_ms_unix(), Ordering::Release);
        self.record_sent();
        Ok(())
    }

    /// Send a [`SbPacket::SensorInfo`] for one logical sensor.
    pub async fn send_sensor_info(&self, desc: &SensorDescriptor) -> Result<(), ClientError> {
        let pkt = Packet::new(
            self.next_seq(),
            SbPacket::SensorInfo {
                sensor_id: desc.sensor_id,
                sensor_status: SensorStatus::Ok,
                sensor_type: desc.imu_type,
                sensor_config: desc.mag_config,
                has_completed_rest_calibration: 1,
                tracker_position: desc.position.clone(),
                tracker_data_type: desc.data_type.clone(),
            },
        );
        let bytes = pkt.to_bytes()?;
        self.send_packet(&bytes).await?;
        Ok(())
    }

    /// Advertise tracker-side feature bits to the server. SlimeIMU v0.4.x sends
    /// `[1 << SENSOR_CONFIG]` so the server may issue `SET_CONFIG_FLAG`
    /// requests (e.g. magnetometer enable toggles).
    pub async fn send_feature_flags(&self, flag_bytes: Vec<u8>) -> Result<(), ClientError> {
        let pkt = Packet::new(self.next_seq(), SbPacket::FeatureFlags { flag_bytes });
        let bytes = pkt.to_bytes()?;
        self.send_packet(&bytes).await?;
        Ok(())
    }

    /// Hot-path send: chooses BUNDLE (packet 100) or two-send (rotation 17 +
    /// accel 4) based on the latest server FEATURE_FLAGS reply. When falling
    /// back to two sends, rotation goes first — accel-first produced visible
    /// 1-frame jitter on legacy servers.
    pub async fn send_rotation_and_accel(
        &self,
        sensor_id: u8,
        rotation: SlimeQuaternion,
        accel: (f32, f32, f32),
    ) -> Result<(), ClientError> {
        if self.server_supports_bundle() {
            self.send_bundle_rot_accel(sensor_id, rotation, accel).await
        } else {
            // Rotation first: legacy servers attached the accel sample to the
            // *previous* rotation when accel arrived first, producing a visible
            // 1-frame jitter.
            self.send_rotation(sensor_id, rotation).await?;
            self.send_acceleration(sensor_id, accel).await?;
            Ok(())
        }
    }

    async fn send_bundle_rot_accel(
        &self,
        sensor_id: u8,
        rotation: SlimeQuaternion,
        accel: (f32, f32, f32),
    ) -> Result<(), ClientError> {
        // Build full packets to extract their bodies. The seq we pass here is
        // discarded — `encode_bundle` strips bytes [4..12] from the inner
        // packet bytes (= drops the inner sequence number).
        let rot_bytes = Packet::new(
            0,
            SbPacket::RotationData {
                sensor_id,
                data_type: SensorDataType::Normal,
                quat: rotation,
                calibration_info: 0,
            },
        )
        .to_bytes()?;
        let accel_bytes = Packet::new(
            0,
            SbPacket::Acceleration {
                vector: accel,
                sensor_id,
            },
        )
        .to_bytes()?;

        let inners = [(17u32, &rot_bytes[12..]), (4u32, &accel_bytes[12..])];
        let bundle = encode_bundle(self.next_seq(), &inners)
            .map_err(|_| ClientError::Encode(deku::DekuError::Assertion("bundle too large")))?;
        self.send_packet(&bundle).await?;
        Ok(())
    }

    /// Send a standalone [`SbPacket::RotationData`] (packet 17). Prefer
    /// [`send_rotation_and_accel`] on the hot path so BUNDLE is used when the
    /// server supports it.
    pub async fn send_rotation(
        &self,
        sensor_id: u8,
        quat: SlimeQuaternion,
    ) -> Result<(), ClientError> {
        let pkt = Packet::new(
            self.next_seq(),
            SbPacket::RotationData {
                sensor_id,
                data_type: SensorDataType::Normal,
                quat,
                calibration_info: 0,
            },
        );
        let bytes = pkt.to_bytes()?;
        self.send_packet(&bytes).await?;
        Ok(())
    }

    /// Send a standalone [`SbPacket::Acceleration`] (packet 4).
    pub async fn send_acceleration(
        &self,
        sensor_id: u8,
        vector: (f32, f32, f32),
    ) -> Result<(), ClientError> {
        let pkt = Packet::new(
            self.next_seq(),
            SbPacket::Acceleration { vector, sensor_id },
        );
        let bytes = pkt.to_bytes()?;
        self.send_packet(&bytes).await?;
        Ok(())
    }

    /// Send a standalone [`SbPacket::Magnetometer`] (packet 5).
    pub async fn send_magnetometer(
        &self,
        sensor_id: u8,
        vector: (f32, f32, f32),
    ) -> Result<(), ClientError> {
        let pkt = Packet::new(
            self.next_seq(),
            SbPacket::Magnetometer {
                sensor_id,
                data_type: SensorDataType::Normal,
                vector,
                calibration_info: 0,
            },
        );
        let bytes = pkt.to_bytes()?;
        self.send_packet(&bytes).await?;
        Ok(())
    }

    /// Send a [`SbPacket::UserAction`] (packet 21) with the given action variant.
    pub async fn send_user_action(
        &self,
        action: crate::serverbound::ActionType,
    ) -> Result<(), ClientError> {
        let pkt = Packet::new(self.next_seq(), SbPacket::UserAction { action });
        let bytes = pkt.to_bytes()?;
        self.send_packet(&bytes).await?;
        Ok(())
    }

    /// Send a battery-level update (packet 12).
    pub async fn send_battery(
        &self,
        voltage_volts: f32,
        level_0_to_1: f32,
    ) -> Result<(), ClientError> {
        let pkt = Packet::new(
            self.next_seq(),
            SbPacket::BatteryLevel {
                voltage_volts,
                level: level_0_to_1,
            },
        );
        let bytes = pkt.to_bytes()?;
        self.send_packet(&bytes).await?;
        Ok(())
    }
}

async fn send_handshake_inner(
    socket: &UdpSocket,
    seq: &AtomicU64,
    info: &HandshakeInfo,
) -> Result<(), ClientError> {
    let pkt = Packet::new(
        seq.fetch_add(1, Ordering::Relaxed),
        SbPacket::Handshake {
            board: info.board.clone(),
            imu: info.imu,
            mcu: info.mcu.clone(),
            imu_info: (info.mag_status, 0, 0),
            protocol_version: 19,
            firmware: SlimeString::from(info.firmware.as_str()),
            mac_address: info.mac_address,
        },
    );
    let bytes = pkt.to_bytes()?;
    match socket.send(&bytes).await {
        Ok(_) => Ok(()),
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionRefused
            ) =>
        {
            Ok(())
        }
        Err(e) => Err(ClientError::Io(e)),
    }
}

async fn receive_loop(
    socket: Arc<UdpSocket>,
    server_supports_bundle: Arc<AtomicBool>,
    handshake_confirmed: Arc<AtomicBool>,
    last_inbound_ms_unix: Arc<AtomicU64>,
    handshake_reset_count: Arc<AtomicU64>,
    handshake_events: broadcast::Sender<HandshakeEvent>,
) {
    // Collapse the WSAECONNRESET / ConnectionRefused flood when the SlimeVR
    // server is down: at 200 Hz each device generates 200 of these per
    // second. We aggregate into a single info-level line every 10 s with
    // the dropped count, instead of one debug line per packet.
    let mut last_reset_log = std::time::Instant::now() - std::time::Duration::from_secs(11);
    let mut reset_count: u64 = 0;
    let mut buf = vec![0u8; 1500];
    loop {
        match socket.recv(&mut buf).await {
            Ok(n) if n >= 4 => {
                let payload = &buf[..n];
                let tag = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                last_inbound_ms_unix.store(now_ms_unix(), Ordering::Release);
                // Any reply from the server confirms the handshake landed.
                // Detect the false→true transition so we emit exactly one
                // HandshakeEvent per recovery — otherwise the UI would see
                // hundreds of events per second once PINGs start streaming.
                let was_confirmed = handshake_confirmed.swap(true, Ordering::AcqRel);
                if !was_confirmed {
                    let _ = handshake_events.send(HandshakeEvent {
                        confirmed: true,
                        reset_count: handshake_reset_count.load(Ordering::Relaxed),
                    });
                }
                handle_inbound(tag, payload, &server_supports_bundle, &socket).await;
            }
            Ok(_) => {
                // Datagram smaller than 4 bytes — ignore.
            }
            Err(e) => {
                // Windows quirk: when we send UDP to a port with no listener, the
                // OS surfaces the ICMP "Port Unreachable" reply on the next recv as
                // WSAECONNRESET (os error 10054). UDP is connectionless, so this is
                // benign — keep the loop alive. Same for `WouldBlock` spurious wakes.
                let kind = e.kind();
                if matches!(
                    kind,
                    std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionRefused
                        | std::io::ErrorKind::WouldBlock
                ) {
                    reset_count += 1;
                    if last_reset_log.elapsed() >= std::time::Duration::from_secs(10) {
                        #[cfg(feature = "client")]
                        tracing::info!(
                            count = reset_count,
                            "slime-tracker: SlimeVR server appears unreachable (aggregated over last 10s)"
                        );
                        let _ = kind;
                        let _ = &e;
                        reset_count = 0;
                        last_reset_log = std::time::Instant::now();
                    }
                    continue;
                }
                #[cfg(feature = "client")]
                tracing::warn!("slime-tracker receive_loop unusual error: {e} (continuing anyway)");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handshake_watchdog(
    socket: Arc<UdpSocket>,
    seq: Arc<AtomicU64>,
    info: Arc<HandshakeInfo>,
    server_supports_bundle: Arc<AtomicBool>,
    handshake_confirmed: Arc<AtomicBool>,
    last_handshake_ms_unix: Arc<AtomicU64>,
    packets_sent: Arc<AtomicU64>,
    last_send_ms_unix: Arc<AtomicU64>,
    last_inbound_ms_unix: Arc<AtomicU64>,
    handshake_reset_count: Arc<AtomicU64>,
    last_reset_ms_unix: Arc<AtomicU64>,
    handshake_events: broadcast::Sender<HandshakeEvent>,
) {
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let last_inbound = last_inbound_ms_unix.load(Ordering::Acquire);
        let now = now_ms_unix();
        let confirmed = handshake_confirmed.load(Ordering::Acquire);

        // If we haven't received any packet in 2 seconds, assume SlimeVR went offline.
        // (SlimeVR sends PINGs roughly every 1 second, so 2s allows 1 missed PING).
        if confirmed && last_inbound > 0 && now.saturating_sub(last_inbound) > 2000 {
            #[cfg(feature = "client")]
            tracing::warn!("SlimeVR connection timeout. Handshake reset.");
            handshake_confirmed.store(false, Ordering::Release);
            // Re-arm the two-send fallback: the next FEATURE_FLAGS reply must
            // re-advertise PROTOCOL_BUNDLE_SUPPORT before we resume bundling.
            server_supports_bundle.store(false, Ordering::Release);
            // Only count the true→false transition; without this guard the
            // counter would tick once every 500 ms while disconnected,
            // which would make the UI look like the server is flapping
            // even though we're just in a stable disconnected state.
            let new_count = handshake_reset_count.fetch_add(1, Ordering::Relaxed) + 1;
            last_reset_ms_unix.store(now_ms_unix(), Ordering::Release);
            let _ = handshake_events.send(HandshakeEvent {
                confirmed: false,
                reset_count: new_count,
            });
        }

        if !handshake_confirmed.load(Ordering::Acquire) {
            match send_handshake_inner(&socket, &seq, &info).await {
                Ok(()) => {
                    last_handshake_ms_unix.store(now_ms_unix(), Ordering::Release);
                    packets_sent.fetch_add(1, Ordering::Relaxed);
                    last_send_ms_unix.store(now_ms_unix(), Ordering::Release);
                    #[cfg(feature = "client")]
                    tracing::trace!("watchdog: re-sent handshake");
                }
                Err(e) => {
                    #[cfg(feature = "client")]
                    tracing::warn!(error = %e, "watchdog: handshake re-send failed");
                }
            }
        }
    }
}

async fn handle_inbound(
    tag: u32,
    payload: &[u8],
    server_supports_bundle: &AtomicBool,
    socket: &UdpSocket,
) {
    match tag {
        // FEATURE_FLAGS reply from server — read bit 0 (PROTOCOL_BUNDLE_SUPPORT)
        // out of the first flag byte after the 12-byte header.
        22 if payload.len() >= 13 => {
            let flag_byte = payload[12];
            let bundle_bit =
                (flag_byte & (1u8 << server_feature_flag_bits::PROTOCOL_BUNDLE_SUPPORT)) != 0;
            server_supports_bundle.store(bundle_bit, Ordering::Release);
            #[cfg(feature = "client")]
            tracing::debug!("Server FEATURE_FLAGS bundle_support={bundle_bit}");
        }
        // PING (packet 10) — echo full datagram back so the server's latency
        // display works correctly.
        10 => {
            let _ = socket.send(payload).await;
        }
        // BUNDLE_TAG outbound only — ignore inbound.
        t if t == BUNDLE_TAG => {}
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip the FEATURE_FLAGS bit-extraction logic against a synthesized
    /// server reply. Validates without spinning up tokio sockets.
    #[test]
    fn parse_feature_flags_reply_sets_bundle_bit() {
        // Outer header (tag 22, seq 0) + flag byte with bit 0 set.
        let mut datagram = Vec::new();
        datagram.extend_from_slice(&22u32.to_be_bytes());
        datagram.extend_from_slice(&0u64.to_be_bytes());
        datagram.push(1u8 << server_feature_flag_bits::PROTOCOL_BUNDLE_SUPPORT);

        let flag = AtomicBool::new(false);
        // Synthesize the same logic the receive loop runs.
        if datagram.len() >= 13 {
            let flag_byte = datagram[12];
            let bundle_bit =
                (flag_byte & (1u8 << server_feature_flag_bits::PROTOCOL_BUNDLE_SUPPORT)) != 0;
            flag.store(bundle_bit, Ordering::Release);
        }
        assert!(flag.load(Ordering::Acquire));
    }

    #[test]
    fn parse_feature_flags_reply_keeps_default_when_bit_clear() {
        let mut datagram = Vec::new();
        datagram.extend_from_slice(&22u32.to_be_bytes());
        datagram.extend_from_slice(&0u64.to_be_bytes());
        datagram.push(0);

        let flag = AtomicBool::new(false);
        if datagram.len() >= 13 {
            let flag_byte = datagram[12];
            let bundle_bit =
                (flag_byte & (1u8 << server_feature_flag_bits::PROTOCOL_BUNDLE_SUPPORT)) != 0;
            flag.store(bundle_bit, Ordering::Release);
        }
        assert!(!flag.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn handshake_event_subscription_receives_initial_confirm() {
        // Spin a tiny echo "server" that replies to the very first inbound
        // datagram. The client should fire one confirmed=true event after
        // its first inbound packet lands.
        let listener = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let info = HandshakeInfo {
            board: BoardType::Custom,
            imu: ImuType::Lsm6ds3trc,
            mcu: McuType::Unknown,
            mag_status: 0,
            firmware: "test".into(),
            mac_address: [1, 2, 3, 4, 5, 6],
        };
        let client = SlimeClient::connect(addr, &info).await.unwrap();
        let mut events = client.subscribe_handshake();

        // Drain one datagram from the listener and reply with anything
        // (the receive loop only cares that *some* packet came back).
        let mut buf = [0u8; 256];
        let (n, from) = tokio::time::timeout(Duration::from_secs(1), listener.recv_from(&mut buf))
            .await
            .expect("listener recv timeout")
            .unwrap();
        // Echo with a tiny well-formed PING packet (tag 10, seq 0).
        let reply: Vec<u8> = {
            let mut v = Vec::new();
            v.extend_from_slice(&10u32.to_be_bytes());
            v.extend_from_slice(&0u64.to_be_bytes());
            v
        };
        let _ = n;
        listener.send_to(&reply, from).await.unwrap();

        let event = tokio::time::timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("no handshake event")
            .expect("event channel closed");
        assert!(event.confirmed, "first event after reply is confirmed=true");
        assert_eq!(event.reset_count, 0);
    }

    #[tokio::test]
    async fn handshake_reset_counter_starts_at_zero() {
        let listener = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let info = HandshakeInfo {
            board: BoardType::Custom,
            imu: ImuType::Lsm6ds3trc,
            mcu: McuType::Unknown,
            mag_status: 0,
            firmware: "test".into(),
            mac_address: [1, 2, 3, 4, 5, 6],
        };
        let client = SlimeClient::connect(addr, &info).await.unwrap();
        let stats = client.stats();
        assert_eq!(stats.handshake_reset_count, 0);
        assert_eq!(stats.last_reset_ms_unix, 0);
    }

    #[tokio::test]
    async fn handshake_increments_packet_counter_and_stamps_timestamps() {
        // Bind a local UDP listener so SlimeClient::connect has a peer to
        // talk to. Don't reply — we only check that the *outgoing* stats
        // get bumped.
        let listener = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let info = HandshakeInfo {
            board: BoardType::Custom,
            imu: ImuType::Lsm6ds3trc,
            mcu: McuType::Unknown,
            mag_status: 0,
            firmware: "test".into(),
            mac_address: [1, 2, 3, 4, 5, 6],
        };
        let client = SlimeClient::connect(addr, &info).await.unwrap();

        let stats = client.stats();
        assert_eq!(stats.packets_sent, 1, "handshake = 1 packet");
        assert!(stats.last_send_ms_unix > 0);
        assert!(stats.last_handshake_ms_unix > 0);
        assert!(!stats.server_supports_bundle, "default before reply");

        // Drain so the listener doesn't drop the datagram.
        let mut buf = [0u8; 256];
        let _ = listener.try_recv(&mut buf);
    }

    #[tokio::test]
    async fn additional_send_calls_increment_counter() {
        let listener = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let info = HandshakeInfo {
            board: BoardType::Custom,
            imu: ImuType::Lsm6ds3trc,
            mcu: McuType::Unknown,
            mag_status: 0,
            firmware: "test".into(),
            mac_address: [1, 2, 3, 4, 5, 6],
        };
        let client = SlimeClient::connect(addr, &info).await.unwrap();
        let before = client.stats().packets_sent;

        client
            .send_rotation(
                0,
                SlimeQuaternion {
                    i: 0.0,
                    j: 0.0,
                    k: 0.0,
                    w: 1.0,
                },
            )
            .await
            .unwrap();
        client.send_acceleration(0, (0.0, 0.0, 9.81)).await.unwrap();

        let after = client.stats().packets_sent;
        assert_eq!(after, before + 2, "two sends = +2 packets");
    }
}
