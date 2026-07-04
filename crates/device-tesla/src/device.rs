//! `TeslaDevice` — implements [`device_traits::Device`] for a Tesla vehicle.
//!
//! Owns:
//! - the OAuth refresh loop (rotates the refresh token periodically),
//! - the streaming WS connection (with reconnect + exponential backoff),
//! - the [`crate::imu::ImuSynth`] state that converts streamed
//!   heading + speed deltas into [`device_traits::ImuSample`] events.
//!
//! The constructor only stores config; all network IO happens after
//! [`TeslaDevice::start`] is called, matching the rest of the device drivers
//! in the workspace.

use std::sync::Arc;
use std::time::Duration;

use device_traits::{
    ChannelInfo, Device, DeviceCapabilities, DeviceError, DeviceId, DeviceKind, DeviceMetadata,
    ImuSample,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, watch, Mutex};
use tokio::task::JoinHandle;

use crate::api::{decode_stream_value, StreamEnvelope, SubscribeMessage, STREAM_COLUMNS};
use crate::auth::{refresh, TokenBundle};
use crate::config::{LiveConfig, SyntheticConfig, TeslaConfig};
use crate::imu::ImuSynth;

/// Hard cap on how long the reconnect loop sleeps between attempts.
const MAX_BACKOFF: Duration = Duration::from_secs(60);

pub struct TeslaDevice {
    metadata: DeviceMetadata,
    config: TeslaConfig,
    stop_tx: watch::Sender<bool>,
    stop_rx: watch::Receiver<bool>,
    runner: Option<JoinHandle<()>>,
}

impl TeslaDevice {
    /// Build a device from config. Does *no* network IO — call
    /// [`TeslaDevice::start`] to actually connect.
    pub fn new(config: TeslaConfig) -> Self {
        let (mac, serial) = match &config {
            TeslaConfig::Live(live) => (
                full_mac(live.vehicle_vin_tail),
                format!("tesla-{}", live.vehicle_id),
            ),
            TeslaConfig::Synthetic(s) => (s.mac, "tesla-synth".to_string()),
        };
        let metadata = DeviceMetadata {
            id: DeviceId { mac, serial },
            kind: DeviceKind::Tesla,
            firmware: None,
            capabilities: DeviceCapabilities {
                has_magnetometer: false,
                has_battery: false,
                has_rumble: false,
                // Tesla streaming caps out around 10 Hz in our experience;
                // we report 10 so the fusion timestep is sized accordingly.
                native_imu_rate_hz: 10,
            },
        };
        let (stop_tx, stop_rx) = watch::channel(false);
        Self {
            metadata,
            config,
            stop_tx,
            stop_rx,
            runner: None,
        }
    }
}

/// Extend a 6-byte vehicle identifier into a 6-byte MAC. The SlimeVR server
/// only sees the MAC, so we just pass it through; this helper exists for
/// symmetry with the other drivers which also pad/truncate identifiers.
fn full_mac(tail: [u8; 6]) -> [u8; 6] {
    tail
}

#[async_trait::async_trait]
impl Device for TeslaDevice {
    fn metadata(&self) -> &DeviceMetadata {
        &self.metadata
    }

    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>, DeviceError> {
        if self.runner.is_some() {
            return Err(DeviceError::Hid("tesla device already started".into()));
        }
        let (tx, rx) = mpsc::channel(64);
        let stop_rx = self.stop_rx.clone();
        let device_id = self.metadata.id.clone();
        let config = self.config.clone();
        let handle = tokio::spawn(async move {
            match config {
                TeslaConfig::Live(live) => {
                    if let Err(e) = run_live(live, device_id.clone(), tx, stop_rx).await {
                        tracing::warn!(error = %e, "tesla live runner exited");
                    }
                }
                TeslaConfig::Synthetic(s) => {
                    if let Err(e) = run_synthetic(s, device_id.clone(), tx, stop_rx).await {
                        tracing::warn!(error = %e, "tesla synthetic runner exited");
                    }
                }
            }
        });
        self.runner = Some(handle);
        Ok(rx)
    }

    async fn stop(&mut self) -> Result<(), DeviceError> {
        let _ = self.stop_tx.send(true);
        if let Some(h) = self.runner.take() {
            // Drop the task — JoinHandle::abort is safe to call always and
            // the inner loop checks the watch channel each iteration so it
            // will exit cooperatively before the abort takes effect.
            h.abort();
        }
        Ok(())
    }

    async fn set_led_mask(&mut self, _mask: u8) -> Result<(), DeviceError> {
        // Tesla has no addressable "player LED". We silently ignore the call
        // so the unified UI doesn't have to special-case this driver.
        Ok(())
    }

    async fn set_rumble(&mut self, _intensity: f32) -> Result<(), DeviceError> {
        // Tesla won't shake itself on command. Drop the request silently.
        Ok(())
    }
}

async fn run_synthetic(
    config: SyntheticConfig,
    device_id: DeviceId,
    tx: mpsc::Sender<ChannelInfo>,
    stop_rx: watch::Receiver<bool>,
) -> Result<(), DeviceError> {
    crate::synthetic::run_synthetic_loop(config.rate_hz, device_id, tx, stop_rx).await
}

async fn run_live(
    config: LiveConfig,
    device_id: DeviceId,
    tx: mpsc::Sender<ChannelInfo>,
    mut stop_rx: watch::Receiver<bool>,
) -> Result<(), DeviceError> {
    let http = reqwest::Client::builder()
        .user_agent("everything-imu/tesla-bridge")
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| DeviceError::Hid(format!("http client init: {e}")))?;
    let token_state: Arc<Mutex<Option<TokenBundle>>> = Arc::new(Mutex::new(None));

    let _ = tx.send(ChannelInfo::Connected(device_id.clone())).await;

    let mut backoff = Duration::from_secs(1);
    loop {
        if *stop_rx.borrow() {
            let _ = tx.send(ChannelInfo::Disconnected).await;
            return Ok(());
        }
        let access_token = match ensure_token(&http, &config, &token_state).await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "tesla token refresh failed, backing off");
                if wait_or_stop(backoff, &mut stop_rx).await {
                    let _ = tx.send(ChannelInfo::Disconnected).await;
                    return Ok(());
                }
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }
        };

        match stream_once(&access_token, &config, &device_id, &tx, &mut stop_rx).await {
            Ok(()) => {
                // Clean disconnect (vehicle slept). Wait a beat and retry.
                tracing::info!("tesla stream ended cleanly; reconnecting");
                if wait_or_stop(Duration::from_secs(5), &mut stop_rx).await {
                    let _ = tx.send(ChannelInfo::Disconnected).await;
                    return Ok(());
                }
                backoff = Duration::from_secs(1);
            }
            Err(e) => {
                tracing::warn!(error = %e, "tesla stream errored, backing off");
                if wait_or_stop(backoff, &mut stop_rx).await {
                    let _ = tx.send(ChannelInfo::Disconnected).await;
                    return Ok(());
                }
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        }
    }
}

/// Sleep for `dur`, returning `true` if the stop signal fired in the meantime.
async fn wait_or_stop(dur: Duration, stop_rx: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(dur) => false,
        _ = stop_rx.changed() => *stop_rx.borrow(),
    }
}

async fn ensure_token(
    http: &reqwest::Client,
    config: &LiveConfig,
    state: &Arc<Mutex<Option<TokenBundle>>>,
) -> Result<String, String> {
    let now = std::time::Instant::now();
    {
        let guard = state.lock().await;
        if let Some(t) = guard.as_ref() {
            if !t.needs_refresh(now) {
                return Ok(t.access_token.clone());
            }
        }
    }
    let current_refresh = {
        let guard = state.lock().await;
        guard
            .as_ref()
            .map(|t| t.refresh_token.clone())
            .unwrap_or_else(|| config.refresh_token.clone())
    };
    let bundle = refresh(http, &config.client_id, &current_refresh)
        .await
        .map_err(|e| e.to_string())?;
    let access = bundle.access_token.clone();
    *state.lock().await = Some(bundle);
    Ok(access)
}

async fn stream_once(
    access_token: &str,
    config: &LiveConfig,
    _device_id: &DeviceId,
    tx: &mpsc::Sender<ChannelInfo>,
    stop_rx: &mut watch::Receiver<bool>,
) -> Result<(), String> {
    use tokio_tungstenite::tungstenite::Message;

    let url = config.region.streaming_url();
    let (mut socket, _resp) = tokio_tungstenite::connect_async(url)
        .await
        .map_err(|e| format!("ws connect: {e}"))?;

    let subscribe = SubscribeMessage {
        msg_type: "data:subscribe_oauth",
        token: access_token,
        value: STREAM_COLUMNS,
        tag: config.vehicle_id.to_string(),
    };
    let subscribe_payload =
        serde_json::to_string(&subscribe).map_err(|e| format!("subscribe encode: {e}"))?;
    socket
        .send(Message::Text(subscribe_payload.into()))
        .await
        .map_err(|e| format!("subscribe send: {e}"))?;

    let mut synth = ImuSynth::new();
    loop {
        tokio::select! {
            msg = socket.next() => {
                let msg = match msg {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => return Err(format!("ws read: {e}")),
                    None => return Ok(()),
                };
                if let Some(action) = handle_ws_message(msg, &mut synth) {
                    match action {
                        FrameAction::Emit(sample) => {
                            if tx.send(ChannelInfo::ImuSamples(vec![sample])).await.is_err() {
                                return Ok(());
                            }
                        }
                        FrameAction::Disconnect => {
                            return Ok(());
                        }
                    }
                }
            }
            _ = tokio::time::sleep(config.idle_timeout) => {
                return Err("idle timeout".into());
            }
            _ = stop_rx.changed() => {
                if *stop_rx.borrow() {
                    let _ = socket.close(None).await;
                    return Ok(());
                }
            }
        }
    }
}

pub(crate) enum FrameAction {
    Emit(ImuSample),
    Disconnect,
}

/// Pure handler — extracted so it can be unit-tested without a socket.
pub(crate) fn handle_ws_message(
    msg: tokio_tungstenite::tungstenite::Message,
    synth: &mut ImuSynth,
) -> Option<FrameAction> {
    use tokio_tungstenite::tungstenite::Message;
    let text = match msg {
        Message::Text(t) => t,
        Message::Binary(_) | Message::Ping(_) | Message::Pong(_) => return None,
        Message::Close(_) => return Some(FrameAction::Disconnect),
        Message::Frame(_) => return None,
    };
    handle_text_frame(text.as_str(), synth)
}

pub(crate) fn handle_text_frame(text: &str, synth: &mut ImuSynth) -> Option<FrameAction> {
    let env: StreamEnvelope = serde_json::from_str(text).ok()?;
    match env.msg_type.as_str() {
        "data:update" => {
            let value = env.value?;
            let frame = decode_stream_value(&value).ok()?;
            let ts_us = frame.timestamp_ms.saturating_mul(1000);
            let sample = synth.ingest(ts_us, frame.heading_deg, frame.speed_mph)?;
            Some(FrameAction::Emit(sample))
        }
        "data:error" => {
            // vehicle_disconnected / vehicle_offline / vehicle_unavailable.
            // Bubble up so the outer loop can sleep + reconnect.
            tracing::debug!(
                error_type = env.error_type.as_deref().unwrap_or("unknown"),
                "tesla stream data:error",
            );
            synth.reset();
            Some(FrameAction::Disconnect)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_unknown_msg_type() {
        let mut synth = ImuSynth::new();
        let res = handle_text_frame(
            r#"{"msg_type":"control:hello","tag":"1","value":"world"}"#,
            &mut synth,
        );
        assert!(res.is_none());
    }

    #[test]
    fn data_update_first_frame_returns_no_action_but_sets_baseline() {
        let mut synth = ImuSynth::new();
        let res = handle_text_frame(
            r#"{"msg_type":"data:update","tag":"1","value":"1000000,30,90,,D,,,"}"#,
            &mut synth,
        );
        assert!(res.is_none(), "first frame is baseline only");

        let res2 = handle_text_frame(
            r#"{"msg_type":"data:update","tag":"1","value":"2000000,30,180,,D,,,"}"#,
            &mut synth,
        );
        let action = res2.expect("second frame yields sample");
        let sample = match action {
            FrameAction::Emit(s) => s,
            FrameAction::Disconnect => panic!("expected emit"),
        };
        assert!(sample.gyro[2].abs() > 0.0, "must record yaw rate");
    }

    #[test]
    fn data_error_returns_disconnect() {
        let mut synth = ImuSynth::new();
        let res = handle_text_frame(
            r#"{"msg_type":"data:error","tag":"1","error_type":"vehicle_disconnected"}"#,
            &mut synth,
        );
        assert!(matches!(res, Some(FrameAction::Disconnect)));
    }

    #[test]
    fn full_mac_passthrough() {
        let m = full_mac([1, 2, 3, 4, 5, 6]);
        assert_eq!(m, [1, 2, 3, 4, 5, 6]);
    }

    #[tokio::test]
    async fn synthetic_device_emits_samples() {
        let config = TeslaConfig::Synthetic(SyntheticConfig {
            rate_hz: 50,
            ..Default::default()
        });
        let mut device = TeslaDevice::new(config);
        let mut rx = device.start().await.expect("start");
        // First event should be Connected.
        let evt = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("connected timeout")
            .expect("channel closed early");
        assert!(matches!(evt, ChannelInfo::Connected(_)));
        // Drain a few samples — synthetic loop emits once per tick.
        let mut got_sample = false;
        for _ in 0..20 {
            if let Ok(Some(ChannelInfo::ImuSamples(samples))) =
                tokio::time::timeout(Duration::from_millis(200), rx.recv()).await
            {
                if !samples.is_empty() {
                    got_sample = true;
                    break;
                }
            }
        }
        assert!(got_sample, "synthetic device must emit at least one sample");
        device.stop().await.expect("stop");
    }
}
