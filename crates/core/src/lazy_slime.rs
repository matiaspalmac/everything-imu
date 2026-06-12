//! Lazily-connected SlimeVR client.
//!
//! A registered device does not open its SlimeVR-Server connection until the
//! first pipeline event arrives. Haptics-only endpoints (a phone in
//! gamepads-only hub role announces `rate_hz = 0`) never emit events, so they
//! never handshake — an idle SlimeClient would otherwise trip the 2 s
//! connection-lost watchdog forever and make the UI's "Last handshake"
//! oscillate between fresh and stale.

use crate::error::AppError;
use slime_tracker::client::{HandshakeInfo, SlimeClient};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::OnceCell;

pub struct LazySlime {
    addr: SocketAddr,
    info: HandshakeInfo,
    cell: OnceCell<Arc<SlimeClient>>,
}

impl LazySlime {
    pub fn new(addr: SocketAddr, info: HandshakeInfo) -> Self {
        Self {
            addr,
            info,
            cell: OnceCell::new(),
        }
    }

    /// Connect on first use. A failed connect is not cached — the next call
    /// retries, so a transient bind error doesn't permanently mute a device.
    pub async fn get(&self) -> Result<Arc<SlimeClient>, AppError> {
        self.cell
            .get_or_try_init(|| async {
                SlimeClient::connect(self.addr, &self.info)
                    .await
                    .map(Arc::new)
                    .map_err(|e| AppError::Slime(e.to_string()))
            })
            .await
            .cloned()
    }

    /// The client, if a connection has already been established. Never
    /// triggers a connect — stats/reset paths must not wake idle endpoints.
    pub fn peek(&self) -> Option<&Arc<SlimeClient>> {
        self.cell.get()
    }
}
