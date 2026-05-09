//! HID transport bridge for the PS Move — hidapi singleton + reader thread.
//!
//! Mirrors device-joycon / device-dualsense. Each device crate owns its own
//! `OnceLock<Arc<Mutex<HidApi>>>`. Multiple instances are harmless on Windows
//! and the libusb backend reference-counts internally.

use hidapi::{HidApi, HidDevice};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use tokio::sync::mpsc;

static HID_API: OnceLock<Arc<Mutex<HidApi>>> = OnceLock::new();

pub fn hid_api_singleton() -> Result<Arc<Mutex<HidApi>>, hidapi::HidError> {
    if let Some(api) = HID_API.get() {
        return Ok(api.clone());
    }
    let api = HidApi::new()?;
    let arc = Arc::new(Mutex::new(api));
    let _ = HID_API.set(arc);
    Ok(HID_API.get().unwrap().clone())
}

pub struct HidReaderHandle {
    pub events_rx: mpsc::Receiver<device_traits::ChannelInfo>,
    pub shutdown: Arc<AtomicBool>,
    pub join: Option<thread::JoinHandle<()>>,
}

impl HidReaderHandle {
    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(j) = self.join.take() {
            // Detach: read_timeout 50 ms guarantees prompt exit.
            drop(j);
        }
    }
}

pub fn spawn_reader<F>(device: HidDevice, mut parse: F) -> HidReaderHandle
where
    F: FnMut(&[u8], &mpsc::Sender<device_traits::ChannelInfo>) + Send + 'static,
{
    let (event_tx, events_rx) = mpsc::channel(64);
    let shutdown = Arc::new(AtomicBool::new(false));
    let sd = shutdown.clone();
    let join = thread::Builder::new()
        .name("device-psmove-hid".into())
        .spawn(move || {
            let _ = device.set_blocking_mode(true);
            let mut buf = [0u8; 64];
            while !sd.load(Ordering::Relaxed) {
                match device.read_timeout(&mut buf, 50) {
                    Ok(0) => continue,
                    Ok(n) => parse(&buf[..n], &event_tx),
                    Err(e) => {
                        let _ = event_tx.blocking_send(device_traits::ChannelInfo::Disconnected);
                        tracing::warn!(error = %e, "hid read error → PS Move gone");
                        return;
                    }
                }
            }
        })
        .expect("spawn hid thread");
    HidReaderHandle {
        events_rx,
        shutdown,
        join: Some(join),
    }
}
