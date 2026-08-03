//! CSV loading.
//!
//! Infers the schema from the first 1000 rows — enough to type most files
//! correctly without reading a large one twice.

use anyhow::{Context, Result};
use polars::prelude::*;
use std::path::Path;

pub fn load_csv(path: &Path) -> Result<DataFrame> {
    CsvReadOptions::default()
        .with_has_header(true)
        .with_infer_schema_length(Some(1000))
        .try_into_reader_with_file_path(Some(path.to_path_buf()))?
        .finish()
        .with_context(|| format!("Failed to load CSV from {}", path.display()))
}
