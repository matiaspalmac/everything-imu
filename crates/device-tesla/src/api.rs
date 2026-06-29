//! Tesla Fleet REST + streaming API client.
//!
//! REST:
//! - `GET /api/1/vehicles` — list vehicles, used once to validate the
//!   configured vehicle_id is owned by the account.
//! - `GET /api/1/vehicles/{id}/vehicle_data?endpoints=drive_state` — used
//!   as a slow-poll fallback when the streaming socket is closed (vehicle
//!   asleep). One sample every ~30 s while idle is enough to keep the
//!   tracker present in SlimeVR-Server.
//!
//! Streaming (`wss://streaming.vn.teslamotors.com/streaming/`):
//! - Client sends `{"msg_type":"data:subscribe_oauth","token":"<access>","value":"speed,heading,...","tag":"<vehicle_id>"}`.
//! - Server emits `{"msg_type":"data:update","tag":"<vid>","value":"<csv>"}`
//!   at ~10 Hz while the car is moving. Columns are the requested fields,
//!   comma separated, with a leading wall-clock timestamp in ms.
//! - Server emits `{"msg_type":"data:error","tag":"<vid>","value":"vehicle_disconnected"}`
//!   when the car parks/sleeps. We back off + reconnect.

use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("http error: {0}")]
    Http(String),
    #[error("rest endpoint returned {status}: {body}")]
    Status { status: u16, body: String },
    #[error("malformed payload: {0}")]
    Malformed(String),
}

#[derive(Debug, Deserialize)]
struct VehiclesEnvelope {
    response: Vec<VehicleSummary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VehicleSummary {
    pub id: u64,
    pub vin: String,
    pub display_name: Option<String>,
    pub state: Option<String>,
}

/// List vehicles available to the access token. Used at startup to verify
/// the configured `vehicle_id` is reachable.
pub async fn list_vehicles(
    client: &reqwest::Client,
    api_host: &str,
    access_token: &str,
) -> Result<Vec<VehicleSummary>, ApiError> {
    let url = format!("{api_host}/api/1/vehicles");
    let resp = client
        .get(&url)
        .bearer_auth(access_token)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ApiError::Http(e.to_string()))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| ApiError::Http(e.to_string()))?;
    if !status.is_success() {
        return Err(ApiError::Status {
            status: status.as_u16(),
            body,
        });
    }
    let env: VehiclesEnvelope = serde_json::from_str(&body)
        .map_err(|e| ApiError::Malformed(format!("vehicles list: {e}")))?;
    Ok(env.response)
}

/// Subscribe message we send right after the WS handshake completes.
#[derive(Debug, Serialize)]
pub(crate) struct SubscribeMessage<'a> {
    pub msg_type: &'static str,
    pub token: &'a str,
    pub value: &'static str,
    pub tag: String,
}

/// Streaming columns we request, in order. Order matters because the server
/// returns CSV values in the exact same order we asked for.
pub const STREAM_COLUMNS: &str = "speed,heading,power,shift_state,est_lat,est_lng,est_heading";

/// One decoded streaming frame.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamFrame {
    /// Wall-clock millisecond timestamp the server stamped on the frame.
    pub timestamp_ms: u64,
    /// Vehicle speed (mph). None when the server emits a blank column.
    pub speed_mph: Option<f32>,
    /// Compass heading, degrees clockwise from true north (0..360).
    pub heading_deg: Option<f32>,
    /// Instantaneous battery power, kW (negative = regen).
    pub power_kw: Option<f32>,
    pub shift_state: Option<String>,
    pub est_lat: Option<f64>,
    pub est_lng: Option<f64>,
    pub est_heading_deg: Option<f32>,
}

/// Decode a CSV "value" payload from a `data:update` frame.
///
/// Empty columns are mapped to `None` — Tesla emits blanks while the
/// drivetrain is stopped (no speed/heading reading).
pub fn decode_stream_value(value: &str) -> Result<StreamFrame, ApiError> {
    let mut iter = value.split(',');
    let ts = iter
        .next()
        .ok_or_else(|| ApiError::Malformed("stream frame missing timestamp".into()))?;
    let timestamp_ms: u64 = ts
        .parse()
        .map_err(|e| ApiError::Malformed(format!("stream timestamp parse: {e}")))?;
    let opt_f32 = |s: &str| -> Option<f32> {
        if s.is_empty() {
            None
        } else {
            match s.parse::<f32>() {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!(value = s, error = %e, "stream column not a valid f32");
                    None
                }
            }
        }
    };
    let opt_f64 = |s: &str| -> Option<f64> {
        if s.is_empty() {
            None
        } else {
            match s.parse::<f64>() {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!(value = s, error = %e, "stream column not a valid f64");
                    None
                }
            }
        }
    };
    let opt_string = |s: &str| -> Option<String> {
        if s.is_empty() {
            None
        } else {
            Some(s.to_string())
        }
    };
    let speed = iter.next().map(opt_f32).unwrap_or(None);
    let heading = iter.next().map(opt_f32).unwrap_or(None);
    let power = iter.next().map(opt_f32).unwrap_or(None);
    let shift = iter.next().map(opt_string).unwrap_or(None);
    let lat = iter.next().map(opt_f64).unwrap_or(None);
    let lng = iter.next().map(opt_f64).unwrap_or(None);
    let est_heading = iter.next().map(opt_f32).unwrap_or(None);
    Ok(StreamFrame {
        timestamp_ms,
        speed_mph: speed,
        heading_deg: heading,
        power_kw: power,
        shift_state: shift,
        est_lat: lat,
        est_lng: lng,
        est_heading_deg: est_heading,
    })
}

/// Envelope the streaming server wraps every frame in.
#[derive(Debug, Deserialize)]
pub(crate) struct StreamEnvelope {
    pub msg_type: String,
    #[allow(dead_code)]
    pub tag: Option<String>,
    pub value: Option<String>,
    pub error_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_stream_value_full_columns() {
        let raw = "1717000000000,42.5,180.0,12.3,D,37.7749,-122.4194,179.8";
        let frame = decode_stream_value(raw).expect("parse");
        assert_eq!(frame.timestamp_ms, 1717000000000);
        assert_eq!(frame.speed_mph, Some(42.5));
        assert_eq!(frame.heading_deg, Some(180.0));
        assert_eq!(frame.power_kw, Some(12.3));
        assert_eq!(frame.shift_state.as_deref(), Some("D"));
        assert_eq!(frame.est_lat, Some(37.7749));
        assert_eq!(frame.est_lng, Some(-122.4194));
        assert_eq!(frame.est_heading_deg, Some(179.8));
    }

    #[test]
    fn decode_stream_value_blank_columns_become_none() {
        let raw = "1717000000000,,,,,,,,";
        let frame = decode_stream_value(raw).expect("parse");
        assert_eq!(frame.timestamp_ms, 1717000000000);
        assert!(frame.speed_mph.is_none());
        assert!(frame.heading_deg.is_none());
        assert!(frame.power_kw.is_none());
        assert!(frame.shift_state.is_none());
    }

    #[test]
    fn decode_stream_value_truncated_frame_only_timestamp() {
        let raw = "1717000000000";
        let frame = decode_stream_value(raw).expect("parse");
        assert_eq!(frame.timestamp_ms, 1717000000000);
        assert!(frame.speed_mph.is_none());
    }

    #[test]
    fn decode_stream_value_rejects_non_numeric_timestamp() {
        let raw = "notanumber,1,2";
        assert!(decode_stream_value(raw).is_err());
    }

    #[test]
    fn stream_envelope_data_update_round_trip() {
        let payload = r#"{
            "msg_type": "data:update",
            "tag": "12345",
            "value": "1717000000000,42.5,180.0,,D,,,"
        }"#;
        let env: StreamEnvelope = serde_json::from_str(payload).unwrap();
        assert_eq!(env.msg_type, "data:update");
        assert_eq!(env.tag.as_deref(), Some("12345"));
        let frame = decode_stream_value(env.value.as_deref().unwrap()).unwrap();
        assert_eq!(frame.heading_deg, Some(180.0));
    }

    #[test]
    fn stream_envelope_disconnect_error() {
        let payload = r#"{
            "msg_type": "data:error",
            "tag": "12345",
            "error_type": "vehicle_disconnected"
        }"#;
        let env: StreamEnvelope = serde_json::from_str(payload).unwrap();
        assert_eq!(env.msg_type, "data:error");
        assert_eq!(env.error_type.as_deref(), Some("vehicle_disconnected"));
    }
}
