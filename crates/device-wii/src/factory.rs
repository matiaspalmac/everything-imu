use crate::device::{metadata_for_key, WiiDevice, WiiPacket};
use device_traits::{Device, DeviceError, DeviceFactory};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};

const PACKET_LEN: usize = 17;
// The Wii homebrew forwarder runs on the console and connects over the LAN,
// so the listener must accept non-loopback peers (matches the 3DS/Vita
// forwarders). Binding 127.0.0.1 made the Wii path unreachable from real
// hardware.
const DEFAULT_BIND: &str = "0.0.0.0:9909";
const DEFAULT_POLLING_RATE_MS: u8 = 10;

#[derive(Clone)]
pub struct WiiFactory {
    bind_addr: String,
}

impl WiiFactory {
    pub fn new() -> Self {
        Self {
            bind_addr: DEFAULT_BIND.to_string(),
        }
    }

    pub fn with_bind_addr(bind_addr: impl Into<String>) -> Self {
        Self {
            bind_addr: bind_addr.into(),
        }
    }
}

impl Default for WiiFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl DeviceFactory for WiiFactory {
    async fn enumerate_loop(
        &self,
        out: mpsc::Sender<(device_traits::DeviceMetadata, Box<dyn Device>)>,
    ) -> Result<(), DeviceError> {
        let listener = TcpListener::bind(&self.bind_addr)
            .await
            .map_err(|e| DeviceError::Hid(format!("wii bind {} failed: {e}", self.bind_addr)))?;
        tracing::info!(addr = %self.bind_addr, "wii forwarded listener online");

        let routing: Arc<RwLock<HashMap<String, mpsc::Sender<WiiPacket>>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let rumble_state: Arc<RwLock<HashMap<String, [u8; 4]>>> =
            Arc::new(RwLock::new(HashMap::new()));

        loop {
            let (stream, peer) = listener
                .accept()
                .await
                .map_err(|e| DeviceError::Hid(format!("wii accept failed: {e}")))?;
            let route = routing.clone();
            let out_tx = out.clone();
            let rumble = rumble_state.clone();
            tokio::spawn(async move {
                if let Err(e) =
                    handle_client(stream, peer.ip().to_string(), out_tx, route, rumble).await
                {
                    tracing::warn!(error = %e, peer = %peer, "wii client handler exited");
                }
            });
        }
    }
}

async fn handle_client(
    mut stream: TcpStream,
    base_ip: String,
    out: mpsc::Sender<(device_traits::DeviceMetadata, Box<dyn Device>)>,
    routing: Arc<RwLock<HashMap<String, mpsc::Sender<WiiPacket>>>>,
    rumble_state: Arc<RwLock<HashMap<String, [u8; 4]>>>,
) -> Result<(), DeviceError> {
    {
        let mut rumble = rumble_state.write().await;
        rumble.entry(base_ip.clone()).or_insert([0, 0, 0, 0]);
    }

    // Track the routing keys created on this connection so every exit path
    // (clean EOF, read/write error, or a closed out-channel) tears down the
    // routing and rumble entries. Dropping the per-device senders lets each
    // WiiDevice reader observe recv() returning None and emit Disconnected,
    // instead of leaking the map entry and the reader task.
    let mut created_keys: Vec<String> = Vec::new();
    let result = client_loop(
        &mut stream,
        &base_ip,
        &out,
        &routing,
        &rumble_state,
        &mut created_keys,
    )
    .await;

    {
        let mut map = routing.write().await;
        for key in &created_keys {
            map.remove(key);
        }
    }
    rumble_state.write().await.remove(&base_ip);

    result
}

async fn client_loop(
    stream: &mut TcpStream,
    base_ip: &str,
    out: &mpsc::Sender<(device_traits::DeviceMetadata, Box<dyn Device>)>,
    routing: &Arc<RwLock<HashMap<String, mpsc::Sender<WiiPacket>>>>,
    rumble_state: &Arc<RwLock<HashMap<String, [u8; 4]>>>,
    created_keys: &mut Vec<String>,
) -> Result<(), DeviceError> {
    let mut read_buf = vec![0u8; PACKET_LEN * 32];
    // Persistent reassembly buffer: TCP is a byte stream, so a single read can
    // split a 17-byte packet or coalesce several. Keep the trailing partial
    // packet across reads and drain only complete packets, instead of dropping
    // the connection whenever a read does not land on the 17-byte grid.
    let mut leftover: Vec<u8> = Vec::with_capacity(PACKET_LEN * 32);
    loop {
        let n = stream
            .read(&mut read_buf)
            .await
            .map_err(|e| DeviceError::Hid(format!("wii read failed: {e}")))?;
        if n == 0 {
            break;
        }
        leftover.extend_from_slice(&read_buf[..n]);

        let complete = leftover.len() - (leftover.len() % PACKET_LEN);
        for chunk in leftover[..complete].chunks_exact(PACKET_LEN) {
            let Some((id, packet)) = parse_packet(chunk) else {
                continue;
            };
            let key = format!("{base_ip}:{id}");
            let tx = {
                let existing = routing.read().await.get(&key).cloned();
                if let Some(tx) = existing {
                    tx
                } else {
                    let (pkt_tx, pkt_rx) = mpsc::channel::<WiiPacket>(256);
                    {
                        let mut map = routing.write().await;
                        map.insert(key.clone(), pkt_tx.clone());
                    }
                    created_keys.push(key.clone());
                    let meta = metadata_for_key(&key);
                    let dev =
                        WiiDevice::new(meta.clone(), pkt_rx, key.clone(), rumble_state.clone());
                    if out.send((meta, Box::new(dev))).await.is_err() {
                        return Ok(());
                    }
                    pkt_tx
                }
            };

            if tx.send(packet).await.is_err() {
                routing.write().await.remove(&key);
            }
        }
        leftover.drain(..complete);
        write_response(stream, base_ip, rumble_state).await?;
    }
    Ok(())
}

async fn write_response(
    stream: &mut TcpStream,
    base_ip: &str,
    rumble_state: &Arc<RwLock<HashMap<String, [u8; 4]>>>,
) -> Result<(), DeviceError> {
    let rumble = {
        rumble_state
            .read()
            .await
            .get(base_ip)
            .copied()
            .unwrap_or([0, 0, 0, 0])
    };
    let mut out = [0u8; 5];
    out[..4].copy_from_slice(&rumble);
    out[4] = DEFAULT_POLLING_RATE_MS;
    stream
        .write_all(&out)
        .await
        .map_err(|e| DeviceError::Hid(format!("wii response write failed: {e}")))?;
    Ok(())
}

fn parse_packet(buf: &[u8]) -> Option<(u8, WiiPacket)> {
    if buf.len() != PACKET_LEN {
        return None;
    }
    let id = buf[0];
    if id == u8::MAX {
        return None;
    }
    // A Wii console drives at most four controllers, and the rumble model is a
    // 4-slot array per IP. Reject ids outside that range so one connection
    // cannot spawn an unbounded number of synthetic devices.
    if id >= 4 {
        return None;
    }
    let read_i16 = |o: usize| i16::from_le_bytes([buf[o], buf[o + 1]]);
    Some((
        id,
        WiiPacket {
            accel: [read_i16(1), read_i16(3), read_i16(5)],
            data: [read_i16(7), read_i16(9), read_i16(11)],
            nunchuk_connected: buf[13] != 0,
            battery_level: buf[15],
            button_up: buf[16] != 0,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_packet_layout_matches_legacy_struct() {
        let raw = [
            0x01, 0x00, 0x02, 0x00, 0x04, 0x00, 0x06, 0x00, 0x08, 0x00, 0x0A, 0x00, 0x0C, 0x01,
            0x01, 0x64, 0x01,
        ];
        let (id, pkt) = parse_packet(&raw).expect("parse");
        assert_eq!(id, 1);
        assert_eq!(pkt.accel, [512, 1024, 1536]);
        assert_eq!(pkt.data, [2048, 2560, 3072]);
        assert!(pkt.nunchuk_connected);
        assert_eq!(pkt.battery_level, 100);
        assert!(pkt.button_up);
    }
}
