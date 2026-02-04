// Allow ignoring write results in debug logging - this is intentional
#![allow(let_underscore_drop)]

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Mutex;

/// Debug logger that writes detailed analysis traces to a file
pub struct DebugLog {
    writer: Option<Mutex<BufWriter<File>>>,
}

impl DebugLog {
    /// Create a disabled debug logger
    pub fn disabled() -> Self {
        Self { writer: None }
    }

    /// Create a debug logger that writes to the specified file
    pub fn to_file(path: &Path) -> std::io::Result<Self> {
        let file = File::create(path)?;
        Ok(Self {
            writer: Some(Mutex::new(BufWriter::new(file))),
        })
    }

    pub fn is_enabled(&self) -> bool {
        self.writer.is_some()
    }

    /// Log a section header
    pub fn section(&self, title: &str) {
        if let Some(ref writer) = self.writer {
            let Ok(mut w) = writer.lock() else { return };
            let _ = writeln!(w, "\n{}", "=".repeat(80));
            let _ = writeln!(w, "  {title}");
            let _ = writeln!(w, "{}\n", "=".repeat(80));
        }
    }

    /// Log a subsection
    pub fn subsection(&self, title: &str) {
        if let Some(ref writer) = self.writer {
            let Ok(mut w) = writer.lock() else { return };
            let _ = writeln!(w, "\n--- {title} ---\n");
        }
    }

    /// Log a key-value pair
    pub fn kv(&self, key: &str, value: &str) {
        if let Some(ref writer) = self.writer {
            let Ok(mut w) = writer.lock() else { return };
            let _ = writeln!(w, "  {key}: {value}");
        }
    }

    /// Log an analysis attempt with its result
    pub fn analysis(&self, analysis_type: &str, column: &str, result: AnalysisOutcome) {
        if let Some(ref writer) = self.writer {
            let Ok(mut w) = writer.lock() else { return };
            let icon = match &result {
                AnalysisOutcome::Triggered { .. } => "[SIGNAL]",
                AnalysisOutcome::BelowThreshold { .. } => "[skip]  ",
                AnalysisOutcome::NoData => "[nodata]",
                AnalysisOutcome::Error(_) => "[ERROR] ",
            };
            let _ = writeln!(w, "{icon} {analysis_type} on '{column}'");

            match result {
                AnalysisOutcome::Triggered { values, reason } => {
                    for (k, v) in values {
                        let _ = writeln!(w, "         {k} = {v}");
                    }
                    let _ = writeln!(w, "         -> TRIGGERED: {reason}");
                },
                AnalysisOutcome::BelowThreshold { values, reason } => {
                    for (k, v) in values {
                        let _ = writeln!(w, "         {k} = {v}");
                    }
                    let _ = writeln!(w, "         -> skipped: {reason}");
                },
                AnalysisOutcome::NoData => {
                    let _ = writeln!(w, "         -> no data available");
                },
                AnalysisOutcome::Error(msg) => {
                    let _ = writeln!(w, "         -> error: {msg}");
                },
            }
            let _ = writeln!(w);
        }
    }

    /// Log a segment attribution result
    pub fn segment(
        &self,
        target: &str,
        segment_col: &str,
        segment_val: &str,
        result: AnalysisOutcome,
    ) {
        if let Some(ref writer) = self.writer {
            let Ok(mut w) = writer.lock() else { return };
            let icon = match &result {
                AnalysisOutcome::Triggered { .. } => "[SIGNAL]",
                AnalysisOutcome::BelowThreshold { .. } => "[skip]  ",
                AnalysisOutcome::NoData => "[nodata]",
                AnalysisOutcome::Error(_) => "[ERROR] ",
            };
            let _ = writeln!(
                w,
                "{icon} Segment '{segment_col}' = '{segment_val}' for '{target}'"
            );

            match result {
                AnalysisOutcome::Triggered { values, reason } => {
                    for (k, v) in values {
                        let _ = writeln!(w, "         {k} = {v}");
                    }
                    let _ = writeln!(w, "         -> TRIGGERED: {reason}");
                },
                AnalysisOutcome::BelowThreshold { values, reason } => {
                    for (k, v) in values {
                        let _ = writeln!(w, "         {k} = {v}");
                    }
                    let _ = writeln!(w, "         -> skipped: {reason}");
                },
                AnalysisOutcome::NoData => {
                    let _ = writeln!(w, "         -> no data");
                },
                AnalysisOutcome::Error(msg) => {
                    let _ = writeln!(w, "         -> error: {msg}");
                },
            }
            let _ = writeln!(w);
        }
    }

    /// Log task queue activity
    pub fn task(&self, action: &str, task_desc: &str) {
        if let Some(ref writer) = self.writer {
            let Ok(mut w) = writer.lock() else { return };
            let _ = writeln!(w, "  [{action}] {task_desc}");
        }
    }

    /// Log a free-form message
    pub fn log(&self, msg: &str) {
        if let Some(ref writer) = self.writer {
            let Ok(mut w) = writer.lock() else { return };
            let _ = writeln!(w, "{msg}");
        }
    }

    /// Flush the buffer
    pub fn flush(&self) {
        if let Some(ref writer) = self.writer {
            let Ok(mut w) = writer.lock() else { return };
            let _ = w.flush();
        }
    }
}

/// Outcome of an analysis attempt
pub enum AnalysisOutcome {
    /// Analysis triggered a signal (met thresholds)
    Triggered {
        values: Vec<(&'static str, String)>,
        reason: String,
    },
    /// Analysis ran but didn't meet thresholds
    BelowThreshold {
        values: Vec<(&'static str, String)>,
        reason: String,
    },
    /// No data available for this analysis
    NoData,
    /// Error during analysis
    Error(String),
}

impl AnalysisOutcome {
    pub fn triggered(values: Vec<(&'static str, String)>, reason: impl Into<String>) -> Self {
        Self::Triggered {
            values,
            reason: reason.into(),
        }
    }

    pub fn below_threshold(values: Vec<(&'static str, String)>, reason: impl Into<String>) -> Self {
        Self::BelowThreshold {
            values,
            reason: reason.into(),
        }
    }
}
