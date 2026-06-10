//! Custom tracing Layer that pushes formatted events into a ring buffer
//! AND emits a `LogEntry` event per record.

use crate::dto::LogEntryDto;
use crate::events::LogEntry;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use tauri::AppHandle as TauriAppHandle;
use tauri_specta::Event;
use tracing::{Event as TracingEvent, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

pub const LOG_RING_CAPACITY: usize = 500;

pub struct UiLayer {
    pub buffer: Arc<Mutex<VecDeque<LogEntryDto>>>,
    pub app: TauriAppHandle,
}

impl<S: Subscriber> Layer<S> for UiLayer {
    fn on_event(&self, event: &TracingEvent<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);
        let entry = LogEntryDto {
            ts_ms: now_ms(),
            level: metadata.level().to_string(),
            target: metadata.target().to_string(),
            message: visitor.0,
        };
        let mut buf = self.buffer.lock();
        if buf.len() >= LOG_RING_CAPACITY {
            buf.pop_front();
        }
        buf.push_back(entry.clone());
        drop(buf);
        let ev = LogEntry {
            ts_ms: entry.ts_ms,
            level: entry.level,
            target: entry.target,
            message: entry.message,
        };
        let _ = ev.emit(&self.app);
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

struct MessageVisitor(String);

impl tracing::field::Visit for MessageVisitor {
    fn record_str(&mut self, _: &tracing::field::Field, value: &str) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
        self.0.push_str(value);
    }
    fn record_debug(&mut self, _: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
        self.0.push_str(&format!("{value:?}"));
    }
}
