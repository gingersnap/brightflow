//! Cross-cutting helpers shared by every handler module.
//!
//! Deliberately small — this is the error type plus the handful of predicates
//! that would otherwise be duplicated. Anything that belongs to one area should
//! live in that area, not here.

mod df_cells;
mod error;
mod path_guard;

pub use df_cells::{derive_title, read_i64_at, read_id_at, read_string_at};
pub use error::{AppError, AppResult};
pub use path_guard::reject_unsafe_path_params;

/// True when a table's stored schema has at least one `String` column: the
/// "enrichable" gate (any table with text can be enriched).
pub fn schema_has_text_column(table: &brightflow_store::TableRow) -> bool {
    table.schema().is_some_and(|schema| {
        schema
            .columns
            .iter()
            .any(|c| c.datatype == brightflow_types::LogicalType::String)
    })
}
