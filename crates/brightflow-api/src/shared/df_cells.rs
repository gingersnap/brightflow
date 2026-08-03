//! Typed single-cell readers over a DataFrame, plus title derivation.
//!
//! The topics and text-explore handlers both walk result rows cell-by-cell to
//! build sample payloads; these helpers keep the physical-type coercions (i32
//! vs i64 ids, string vs numeric identifiers) identical in both.

use polars::prelude::DataFrame;

pub fn read_string_at(df: &DataFrame, col: &str, row: usize) -> Option<String> {
    df.column(col)
        .ok()?
        .as_materialized_series()
        .str()
        .ok()?
        .get(row)
        .map(str::to_string)
}

pub fn read_i64_at(df: &DataFrame, col: &str, row: usize) -> Option<i64> {
    let series = df.column(col).ok()?.as_materialized_series();
    if let Ok(ca) = series.i64() {
        return ca.get(row);
    }
    if let Ok(ca) = series.i32() {
        return ca.get(row).map(i64::from);
    }
    None
}

/// Read an identifier cell as a string, whatever its physical type
/// (issue ids are i64, post ids are at:// uri strings).
pub fn read_id_at(df: &DataFrame, col: &str, row: usize) -> Option<String> {
    read_string_at(df, col, row).or_else(|| read_i64_at(df, col, row).map(|v| v.to_string()))
}

const DERIVED_TITLE_CHARS: usize = 120;

/// Headline for tables without a title column: first line of the body,
/// truncated at a char boundary.
pub fn derive_title(body: &str) -> String {
    let first_line = body.lines().next().unwrap_or("").trim();
    if first_line.chars().count() > DERIVED_TITLE_CHARS {
        let head: String = first_line.chars().take(DERIVED_TITLE_CHARS).collect();
        format!("{head}…")
    } else {
        first_line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polars::df;

    #[test]
    fn id_reader_handles_both_physical_types() {
        let df = df!(
            "num_id" => &[7_i64],
            "str_id" => &["at://post/1"],
        )
        .unwrap();
        assert_eq!(read_id_at(&df, "num_id", 0).unwrap(), "7");
        assert_eq!(read_id_at(&df, "str_id", 0).unwrap(), "at://post/1");
        assert!(read_id_at(&df, "missing", 0).is_none());
    }

    #[test]
    fn derive_title_truncates_on_char_boundary() {
        let long = "å".repeat(200);
        let title = derive_title(&long);
        assert!(title.ends_with('…'));
        assert_eq!(title.chars().count(), DERIVED_TITLE_CHARS + 1);
        assert_eq!(derive_title("short line\nrest"), "short line");
    }
}
