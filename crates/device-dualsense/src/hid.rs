//! HID transport bridge for the Sony controllers — hidapi singleton + reader thread.
//!
//! Mirrors device-joycon's pattern. Each crate owns its own static `OnceLock`, so two
//! drivers loaded into the same process initialize hidapi twice. Acceptable on Windows
//! (HidApi::new is idempotent at the OS level); a follow-up could factor a shared
//! singleton into device-traits.

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
            // Reader thread polls every 50 ms via read_timeout, so it exits promptly.
            // Detach instead of join to avoid stalling the supervisor on a wedged HID.
            drop(j);
        }
    }
}

impl Drop for HidReaderHandle {
    fn drop(&mut self) {
        // Owner forgot to call shutdown(): still flip the atomic so the
        // reader thread exits on its next read_timeout boundary instead of
        // running forever and holding the HidDevice mutex against future
        // `set_led` / `set_rumble` writes.
        if self.join.is_some() {
            self.shutdown.store(true, Ordering::Relaxed);
        }
    }
}

pub fn spawn_reader<F>(device: Arc<Mutex<HidDevice>>, mut parse: F) -> HidReaderHandle
where
    F: FnMut(&[u8], &mpsc::Sender<device_traits::ChannelInfo>) + Send + 'static,
{
    let (event_tx, events_rx) = mpsc::channel(64);
    let shutdown = Arc::new(AtomicBool::new(false));
    let sd = shutdown.clone();
    let join = thread::Builder::new()
        .name("device-dualsense-hid".into())
        .spawn(move || {
            if let Ok(dev) = device.lock() {
                let _ = dev.set_blocking_mode(true);
            }
            let mut buf = [0u8; 78];
            while !sd.load(Ordering::Relaxed) {
                let read_res = {
                    let dev = match device.lock() {
                        Ok(g) => g,
                        Err(_) => {
                            let _ =
                                event_tx.blocking_send(device_traits::ChannelInfo::Disconnected);
                            return;
                        }
                    };
                    dev.read_timeout(&mut buf, 50)
                };
                match read_res {
                    Ok(0) => continue,
                    Ok(n) => parse(&buf[..n], &event_tx),
                    Err(e) => {
                        let _ = event_tx.blocking_send(device_traits::ChannelInfo::Disconnected);
                        tracing::warn!(error = %e, "hid read error → DualSense gone");
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
