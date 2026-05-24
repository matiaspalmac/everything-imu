//! HID transport bridge: hidapi singleton, dedicated std::thread reader, tokio bridges.

use device_traits::{ChannelInfo, ImuSample};
use hidapi::{HidApi, HidDevice};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use tokio::sync::{mpsc, watch};

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
    pub events_rx: mpsc::Receiver<ChannelInfo>,
    pub samples_rx: watch::Receiver<Option<ImuSample>>,
    pub shutdown: Arc<AtomicBool>,
    pub join: Option<thread::JoinHandle<()>>,
}

impl HidReaderHandle {
    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(j) = self.join.take() {
            // Don't block — thread exits within 50ms via read_timeout. Detach.
            drop(j);
        }
    }
}

impl Drop for HidReaderHandle {
    fn drop(&mut self) {
        // If the owner forgot to call shutdown(), still flip the atomic so
        // the reader thread exits on its next read_timeout boundary instead
        // of running forever holding the HidDevice mutex.
        if self.join.is_some() {
            self.shutdown.store(true, Ordering::Relaxed);
            // Drop the join handle without blocking — same rationale as
            // `shutdown()`. Thread terminates within ~50 ms.
        }
    }
}

/// Spawn the blocking HID reader thread.
///
/// The device handle is shared (`Arc<Mutex<HidDevice>>`) rather than moved so
/// the owning `Device` can still issue output reports (LED, rumble) after the
/// reader is running. The reader holds the lock only for the duration of one
/// `read_timeout` call, so a concurrent write waits at most one poll interval
/// (~50 ms) — imperceptible for non-realtime commands.
pub fn spawn_reader<F>(device: Arc<Mutex<HidDevice>>, mut parse: F) -> HidReaderHandle
where
    F: FnMut(&[u8], &mpsc::Sender<ChannelInfo>, &watch::Sender<Option<ImuSample>>) + Send + 'static,
{
    let (event_tx, events_rx) = mpsc::channel(64);
    let (sample_tx, samples_rx) = watch::channel(None);
    let shutdown = Arc::new(AtomicBool::new(false));
    let sd = shutdown.clone();
    let join = thread::Builder::new()
        .name("device-joycon-hid".into())
        .spawn(move || {
            {
                let dev = device.lock().unwrap();
                let _ = dev.set_blocking_mode(true);
            }
            let mut buf = [0u8; 64];
            while !sd.load(Ordering::Relaxed) {
                let read = {
                    let dev = device.lock().unwrap();
                    dev.read_timeout(&mut buf, 50)
                };
                match read {
                    Ok(0) => continue,
                    Ok(n) => parse(&buf[..n], &event_tx, &sample_tx),
                    Err(e) => {
                        let _ = event_tx.blocking_send(ChannelInfo::Disconnected);
                        tracing::warn!(error = %e, "hid read error → device gone");
                        return;
                    }
                }
            }
        })
        .expect("spawn hid thread");
    HidReaderHandle {
        events_rx,
        samples_rx,
        shutdown,
        join: Some(join),
    }
}
