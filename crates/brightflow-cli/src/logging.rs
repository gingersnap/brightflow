//! Tracing setup for the CLI: broadcast layer for the API's /system feed,
//! daily-rotated file appender (logs/backend.log.<date>, last 7 kept), and
//! the simple stderr-only variant for one-shot commands.

use std::sync::OnceLock;

use tokio::sync::broadcast;
use tracing_subscriber::{
    fmt::writer::{BoxMakeWriter, MakeWriter},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

use brightflow_api::system::log_layer::{LogBroadcastLayer, LogEntry};

/// Holds the `WorkerGuard` for the file appender so logs are flushed on exit.
/// Leaked intentionally — the appender must live for the program's lifetime.
static FILE_APPENDER_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

/// Directory where rotating backend log files are written.
const LOG_DIR: &str = "logs";

/// Filename prefix for the backend log. `tracing-appender` appends the date
/// (e.g. `backend.log.2026-08-02`) and rotates daily.
const LOG_PREFIX: &str = "backend.log";

/// Maximum number of rotated log files to retain on disk.
const MAX_LOG_FILES: usize = 7;

/// No-op `MakeWriter` used as a fallback when the rolling file appender cannot
/// be created, so logging continues to stdout only.
struct DiscardWriter;

impl<'a> MakeWriter<'a> for DiscardWriter {
    type Writer = std::io::Sink;
    fn make_writer(&'a self) -> Self::Writer {
        std::io::sink()
    }
}

/// Builds a daily-rotating non-blocking file writer for `logs/backend.log.*`,
/// keeping the last [`MAX_LOG_FILES`] files. The `WorkerGuard` is stored in a
/// static so it is only dropped (and flushed) at process exit. On failure it
/// falls back to a discard writer (stdout layer still logs) so the server
/// starts regardless.
#[allow(clippy::print_stderr)]
pub(crate) fn make_file_writer() -> BoxMakeWriter {
    if std::fs::create_dir_all(LOG_DIR).is_err() {
        eprintln!("tracing: could not create log dir '{LOG_DIR}'");
        return BoxMakeWriter::new(DiscardWriter);
    }
    let file_appender = match tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix(LOG_PREFIX)
        .max_log_files(MAX_LOG_FILES)
        .build(LOG_DIR)
    {
        Ok(appender) => appender,
        Err(err) => {
            eprintln!("tracing: failed to create rolling file appender: {err}");
            return BoxMakeWriter::new(DiscardWriter);
        },
    };

    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let _guard_result = FILE_APPENDER_GUARD.set(guard);

    BoxMakeWriter::new(non_blocking)
}

pub(crate) fn init_tracing(default_filter: &str) -> broadcast::Sender<LogEntry> {
    let (log_sender, _) = broadcast::channel(1000);
    let log_layer = LogBroadcastLayer::new(log_sender.clone());

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(make_file_writer())
                .with_ansi(false),
        )
        .with(log_layer)
        .init();

    log_sender
}

pub(crate) fn init_tracing_simple(default_filter: &str) {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(make_file_writer())
                .with_ansi(false),
        )
        .init();
}
