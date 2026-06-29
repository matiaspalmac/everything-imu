//! OAuth2 refresh-token flow against Tesla SSO.
//!
//! The Fleet API access token expires every 8 h. We refresh proactively at
//! 80 % of the advertised lifetime so the streaming loop never serves an
//! expired token. Refresh tokens themselves rotate every refresh — the new
//! one comes back in the response body and we hand it to the caller.

use std::time::{Duration, Instant};

use serde::Deserialize;

/// Tesla SSO token endpoint. Same host across regions.
const TOKEN_URL: &str = "https://auth.tesla.com/oauth2/v3/token";

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("http error: {0}")]
    Http(String),
    #[error("token endpoint returned {status}: {body}")]
    Status { status: u16, body: String },
    #[error("token response missing required field: {0}")]
    MissingField(&'static str),
}

#[derive(Debug, Clone)]
pub struct TokenBundle {
    pub access_token: String,
    pub refresh_token: String,
    /// Wall-clock instant at which we should refresh `access_token`. This is the
    /// 80 %-of-`expires_in` threshold (a safety margin before the hard expiry),
    /// not the token's hard expiry time.
    pub expires_at: Instant,
}

impl TokenBundle {
    /// `true` once we're past the 80 %-of-lifetime mark and should refresh.
    pub fn needs_refresh(&self, now: Instant) -> bool {
        now >= self.expires_at
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Exchange a refresh token for a fresh access + refresh token pair.
///
/// `client_id` is the Fleet API client ID the user registered with Tesla;
/// `refresh_token` rotates on every successful call.
pub async fn refresh(
    client: &reqwest::Client,
    client_id: &str,
    refresh_token: &str,
) -> Result<TokenBundle, AuthError> {
    let form = [
        ("grant_type", "refresh_token"),
        ("client_id", client_id),
        ("refresh_token", refresh_token),
        ("scope", "openid offline_access vehicle_device_data"),
    ];
    let resp = client
        .post(TOKEN_URL)
        .form(&form)
        .send()
        .await
        .map_err(|e| AuthError::Http(e.to_string()))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| AuthError::Http(e.to_string()))?;
    if !status.is_success() {
        return Err(AuthError::Status {
            status: status.as_u16(),
            body,
        });
    }
    let parsed: TokenResponse =
        serde_json::from_str(&body).map_err(|e| AuthError::Http(format!("json decode: {e}")))?;
    if let Some(err) = parsed.error {
        return Err(AuthError::Status {
            status: 400,
            body: format!("{err}: {}", parsed.error_description.unwrap_or_default()),
        });
    }
    let access_token = parsed
        .access_token
        .ok_or(AuthError::MissingField("access_token"))?;
    let refresh_token = parsed
        .refresh_token
        .ok_or(AuthError::MissingField("refresh_token"))?;
    let expires_in = parsed
        .expires_in
        .ok_or(AuthError::MissingField("expires_in"))?;
    // Refresh at 80 % lifetime to absorb clock skew + transient outages.
    let refresh_after = Duration::from_secs(expires_in.saturating_mul(4) / 5);
    Ok(TokenBundle {
        access_token,
        refresh_token,
        expires_at: Instant::now() + refresh_after,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn token_bundle_needs_refresh_after_expiry() {
        let now = Instant::now();
        let bundle = TokenBundle {
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at: now - Duration::from_secs(1),
        };
        assert!(bundle.needs_refresh(now));
    }

    #[test]
    fn token_bundle_fresh_does_not_need_refresh() {
        let now = Instant::now();
        let bundle = TokenBundle {
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at: now + Duration::from_secs(60),
        };
        assert!(!bundle.needs_refresh(now));
    }

    #[test]
    fn token_response_parses_minimal_success() {
        let body = r#"{
            "access_token": "AT",
            "refresh_token": "RT",
            "expires_in": 28800,
            "token_type": "Bearer"
        }"#;
        let parsed: TokenResponse = serde_json::from_str(body).unwrap();
        assert_eq!(parsed.access_token.as_deref(), Some("AT"));
        assert_eq!(parsed.refresh_token.as_deref(), Some("RT"));
        assert_eq!(parsed.expires_in, Some(28800));
        assert!(parsed.error.is_none());
    }

    #[test]
    fn token_response_parses_error_envelope() {
        let body = r#"{
            "error": "invalid_grant",
            "error_description": "refresh token expired"
        }"#;
        let parsed: TokenResponse = serde_json::from_str(body).unwrap();
        assert!(parsed.access_token.is_none());
        assert_eq!(parsed.error.as_deref(), Some("invalid_grant"));
    }
}
