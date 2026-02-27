use serde::Serialize;
use tokio::sync::broadcast;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// A single log entry captured from tracing.
#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

/// Custom tracing Layer that broadcasts log events over a channel.
pub struct LogBroadcastLayer {
    sender: broadcast::Sender<LogEntry>,
}

impl LogBroadcastLayer {
    pub fn new(sender: broadcast::Sender<LogEntry>) -> Self {
        Self { sender }
    }
}

/// Visitor that extracts the message field from a tracing event.
struct MessageVisitor {
    message: String,
}

impl MessageVisitor {
    fn new() -> Self {
        Self {
            message: String::new(),
        }
    }
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        } else if self.message.is_empty() {
            // Fallback: use first field if no explicit "message"
            self.message = format!("{value:?}");
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" || self.message.is_empty() {
            self.message = value.to_string();
        }
    }
}

impl<S: Subscriber> Layer<S> for LogBroadcastLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let level = *event.metadata().level();

        // Only capture info, warn, error
        if level > Level::INFO {
            return;
        }

        let mut visitor = MessageVisitor::new();
        event.record(&mut visitor);

        let entry = LogEntry {
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            level: level.to_string().to_uppercase(),
            target: event.metadata().target().to_string(),
            message: visitor.message,
        };

        // Best-effort send — drop if no subscribers or channel full
        drop(self.sender.send(entry));
    }
}
