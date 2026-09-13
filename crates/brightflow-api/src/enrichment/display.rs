//! Which columns make a row readable as a document: the id to key on, the
//! number and title to head it, the body to quote, the timestamp to order
//! by, and how to link back to the source.
//!
//! Comes from the table's resolved `doc` fields: a connector or a person
//! declared them, or the detector guessed them from the column names when
//! it gave the table its base layer (`brightflow_engine::data::schema::
//! infer_doc_fields`, the one place that guess lives). The same guess is the
//! fallback here for a table with no semantic rows at all. Presentation-only,
//! except that the id column is also the join key the mentions child table
//! and induction sampling use, so every consumer reads it from here.

use brightflow_store::ParquetStore;
use brightflow_types::DocFields;
use polars::prelude::DataFrame;

use crate::shared::{read_string_at, AppResult};

/// Columns used to render one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DocDisplay {
    /// Stringified into `DocRef.id`.
    pub id_column: String,
    pub number_column: Option<String>,
    /// None = derive a headline from the body's first line.
    pub title_column: Option<String>,
    pub body_column: Option<String>,
    pub timestamp_column: Option<String>,
    /// `{column}` placeholders, rendered per row.
    pub url_template: Option<String>,
}

impl DocDisplay {
    /// The declared doc fields where present, column-name inference for
    /// every field left unstated. `columns` are the table's actual columns,
    /// so inference never names one that is not there.
    pub fn from_doc(doc: Option<&DocFields>, columns: &[String]) -> Self {
        let inferred = Self::infer(columns);
        let Some(doc) = doc else {
            return inferred;
        };
        Self {
            id_column: doc.id.clone().unwrap_or(inferred.id_column),
            number_column: doc.number.clone().or(inferred.number_column),
            title_column: doc.title.clone().or(inferred.title_column),
            body_column: doc.body.clone().or(inferred.body_column),
            timestamp_column: doc.timestamp.clone().or(inferred.timestamp_column),
            url_template: doc.url_template.clone().or(inferred.url_template),
        }
    }

    /// What the column names say, for a table nobody has described: the
    /// detector's guess, applied at read time. A missing id column still
    /// yields `id`, which readers treat as "use the row index".
    pub fn infer(columns: &[String]) -> Self {
        let doc = brightflow_engine::data::schema::infer_doc_fields(columns).unwrap_or_default();
        Self {
            id_column: doc.id.unwrap_or_else(|| "id".to_string()),
            number_column: doc.number,
            title_column: doc.title,
            body_column: doc.body,
            timestamp_column: doc.timestamp,
            url_template: doc.url_template,
        }
    }

    /// The table's display, from its resolved semantics in the store.
    pub async fn resolve(
        store: &ParquetStore,
        source_id: &str,
        table: &str,
        columns: &[String],
    ) -> AppResult<Self> {
        let resolved = store.resolved_table(source_id, table).await?;
        Ok(Self::from_doc(
            resolved.as_ref().and_then(|t| t.doc.as_ref()),
            columns,
        ))
    }

    /// The link for one row; `None` when there is no template or any of its
    /// placeholders has no value on this row — half a URL is worse than none.
    pub fn url_at(&self, df: &DataFrame, row: usize) -> Option<String> {
        let template = self.url_template.as_deref()?;
        let mut values = std::collections::HashMap::new();
        for column in placeholders(template) {
            let value = read_string_at(df, &column, row).filter(|v| !v.is_empty())?;
            values.insert(column, value);
        }
        let doc = DocFields {
            url_template: Some(template.to_string()),
            ..DocFields::default()
        };
        doc.render_url(|k| values.get(k).map(String::as_str))
            .filter(|u| !u.is_empty())
    }
}

/// The `{column}` names in a template.
fn placeholders(template: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else { break };
        out.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use polars::df;

    fn cols(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn inference_follows_column_names() {
        let d = DocDisplay::infer(&cols(&[
            "id",
            "number",
            "title",
            "body",
            "html_url",
            "created_at",
        ]));
        assert_eq!(d.id_column, "id");
        assert_eq!(d.number_column.as_deref(), Some("number"));
        assert_eq!(d.title_column.as_deref(), Some("title"));
        assert_eq!(d.body_column.as_deref(), Some("body"));
        assert_eq!(d.url_template.as_deref(), Some("{html_url}"));
        assert_eq!(d.timestamp_column.as_deref(), Some("created_at"));

        let posts = DocDisplay::infer(&cols(&["uri", "text", "indexed_at"]));
        assert_eq!(posts.id_column, "uri");
        assert_eq!(posts.title_column, None);
        assert_eq!(posts.body_column.as_deref(), Some("text"));
        assert_eq!(posts.timestamp_column, None);
        assert_eq!(posts.url_template, None);

        let bare = DocDisplay::infer(&cols(&["x"]));
        assert_eq!(bare.id_column, "id");
        assert_eq!(bare.body_column, None);
    }

    #[test]
    fn declared_doc_fields_win_and_inference_fills_the_rest() {
        let doc = DocFields {
            id: Some("uri".into()),
            body: Some("text".into()),
            url_template: Some("https://bsky.app/profile/{author_handle}/post/{rkey}".into()),
            ..DocFields::default()
        };
        let d = DocDisplay::from_doc(Some(&doc), &cols(&["uri", "text", "created_at", "title"]));
        assert_eq!(d.id_column, "uri");
        assert_eq!(d.body_column.as_deref(), Some("text"));
        assert_eq!(d.title_column.as_deref(), Some("title"));
        assert_eq!(d.timestamp_column.as_deref(), Some("created_at"));
        assert!(d.url_template.as_deref().unwrap().contains("{rkey}"));
    }

    #[test]
    fn url_renders_per_row_and_is_none_when_incomplete() {
        let df = df!(
            "author_handle" => &["alice.bsky.social", ""],
            "rkey" => &["3kxyz", "3kabc"],
            "html_url" => &["https://x/1", ""],
        )
        .unwrap();
        let bsky = DocDisplay {
            url_template: Some("https://bsky.app/profile/{author_handle}/post/{rkey}".into()),
            ..DocDisplay::infer(&[])
        };
        assert_eq!(
            bsky.url_at(&df, 0).as_deref(),
            Some("https://bsky.app/profile/alice.bsky.social/post/3kxyz")
        );
        // A missing handle renders a broken profile path; better no link.
        assert_eq!(bsky.url_at(&df, 1), None);

        let plain = DocDisplay::infer(&cols(&["html_url"]));
        assert_eq!(plain.url_at(&df, 0).as_deref(), Some("https://x/1"));
        assert_eq!(plain.url_at(&df, 1), None);
        assert_eq!(DocDisplay::infer(&[]).url_at(&df, 0), None);
    }

    #[test]
    fn placeholders_are_extracted_in_order() {
        assert_eq!(placeholders("a{b}c{d}"), vec!["b", "d"]);
        assert!(placeholders("plain").is_empty());
        assert_eq!(placeholders("{x"), Vec::<String>::new());
    }
}
