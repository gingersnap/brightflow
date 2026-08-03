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

/// True when a stored `schema_json` contains at least one string column —
/// the schema-based "enrichable" gate (any table with text can be enriched).
pub fn schema_has_text_column(schema_json: Option<&str>) -> bool {
    schema_json
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| {
            v.get("fields").and_then(|f| f.as_array()).map(|fields| {
                fields.iter().any(|f| {
                    f.get("type")
                        .and_then(|t| t.as_str())
                        .is_some_and(|t| t == "str" || t == "string")
                })
            })
        })
        .unwrap_or(false)
}
