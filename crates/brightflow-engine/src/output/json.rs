//! JSON serialization of an analysis tree — the machine-readable output and the
//! one the API and frontend consume.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use crate::analysis::tree::AnalysisTree;

pub fn write_output(path: &Path, tree: &AnalysisTree, pretty: bool) -> Result<()> {
    let file = File::create(path)
        .with_context(|| format!("Failed to create output file {}", path.display()))?;
    let writer = BufWriter::new(file);

    if pretty {
        serde_json::to_writer_pretty(writer, tree)
            .with_context(|| "Failed to serialize analysis tree to JSON")?;
    } else {
        serde_json::to_writer(writer, tree)
            .with_context(|| "Failed to serialize analysis tree to JSON")?;
    }

    Ok(())
}
