//! Per-table enrichment configuration.
//!
//! Replaces the hardcoded `ENRICHABLE_TABLES` lookup as the source of truth:
//! builtin defaults per table type, overlaid with user overrides stored in
//! `table_enrichment_settings` (loaded by the API/scheduler — the engine
//! itself stays DB-free).

use serde::{Deserialize, Serialize};

use crate::embedding::EmbedderId;
use crate::nlp::CleaningProfile;

/// Effective enrichment configuration for one table.
#[derive(Debug, Clone)]
pub struct EnrichmentConfig {
    /// Text columns concatenated into the embed text; first = headline.
    pub text_columns: Vec<String>,
    pub cleaning_profile: CleaningProfile,
    /// Explicit language column; None = auto-detect (`lang`/`language`).
    pub language_column: Option<String>,
    pub embedder: EmbedderId,
    /// Minimum cluster size (HDBSCAN input + UI floor); None = dynamic.
    pub min_cluster_size: Option<usize>,
    /// Clustering algorithm: "kmeans" (default) or "hdbscan".
    pub algorithm: String,
}

impl EnrichmentConfig {
    /// Builtin default for a table type; None = not enrichable.
    pub fn builtin_default(table_name: &str) -> Option<Self> {
        match table_name {
            "issues" => Some(Self {
                text_columns: vec!["title".to_string(), "body".to_string()],
                cleaning_profile: CleaningProfile::MarkdownIssue,
                language_column: None,
                embedder: EmbedderId::default(),
                min_cluster_size: None,
                algorithm: "kmeans".to_string(),
            }),
            "posts" => Some(Self {
                text_columns: vec!["text".to_string()],
                cleaning_profile: CleaningProfile::Social,
                language_column: Some("lang".to_string()),
                embedder: EmbedderId::default(),
                min_cluster_size: None,
                algorithm: "kmeans".to_string(),
            }),
            _ => None,
        }
    }

    /// Builtin default ⊕ stored overrides.
    pub fn resolve(table_name: &str, overrides: Option<&EnrichmentOverrides>) -> Option<Self> {
        let mut config = Self::builtin_default(table_name)?;
        if let Some(o) = overrides {
            config.apply(o);
        }
        Some(config)
    }

    pub fn apply(&mut self, o: &EnrichmentOverrides) {
        if let Some(cols) = &o.text_columns {
            if !cols.is_empty() {
                self.text_columns.clone_from(cols);
            }
        }
        if let Some(profile) = o
            .cleaning_profile
            .as_deref()
            .and_then(CleaningProfile::parse)
        {
            self.cleaning_profile = profile;
        }
        if let Some(lang_col) = &o.language_column {
            self.language_column = if lang_col.is_empty() {
                None
            } else {
                Some(lang_col.clone())
            };
        }
        if let Some(embedder) = o.embedder.as_deref().and_then(EmbedderId::parse) {
            self.embedder = embedder;
        }
        if let Some(mcs) = o.min_cluster_size {
            self.min_cluster_size = Some(mcs);
        }
        if let Some(algo) = o.algorithm.as_deref() {
            if algo == "kmeans" || algo == "hdbscan" {
                self.algorithm = algo.to_string();
            }
        }
    }
}

/// User overrides, as stored in `table_enrichment_settings`. All fields are
/// optional deltas over the builtin default.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnrichmentOverrides {
    pub text_columns: Option<Vec<String>>,
    pub cleaning_profile: Option<String>,
    pub language_column: Option<String>,
    /// Embedder id. Only `"potion-base-32M"` parses today; the field (and its
    /// settings column) stays so adding a second backend is purely additive.
    /// Unknown ids are ignored and the default applies (`apply` above).
    pub embedder: Option<String>,
    pub min_cluster_size: Option<usize>,
    pub algorithm: Option<String>,
}

impl EnrichmentOverrides {
    /// Build from raw stored fields (`text_columns` is a JSON array string).
    #[allow(clippy::too_many_arguments)]
    pub fn from_stored(
        text_columns_json: Option<&str>,
        cleaning_profile: Option<String>,
        language_column: Option<String>,
        embedder: Option<String>,
        min_cluster_size: Option<i64>,
        algorithm: Option<String>,
    ) -> Self {
        let text_columns = text_columns_json
            .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
            .filter(|v| !v.is_empty());
        Self {
            text_columns,
            cleaning_profile,
            language_column,
            embedder,
            min_cluster_size: min_cluster_size.and_then(|v| usize::try_from(v).ok()),
            algorithm,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn builtin_defaults_match_table_types() {
        let issues = EnrichmentConfig::builtin_default("issues").unwrap();
        assert_eq!(issues.cleaning_profile, CleaningProfile::MarkdownIssue);
        assert_eq!(issues.text_columns, vec!["title", "body"]);

        let posts = EnrichmentConfig::builtin_default("posts").unwrap();
        assert_eq!(posts.cleaning_profile, CleaningProfile::Social);
        assert_eq!(posts.language_column.as_deref(), Some("lang"));

        assert!(EnrichmentConfig::builtin_default("users").is_none());
    }

    #[test]
    fn overrides_overlay_defaults() {
        let overrides = EnrichmentOverrides {
            cleaning_profile: Some("plain".to_string()),
            embedder: Some("potion-base-32M".to_string()),
            min_cluster_size: Some(25),
            ..Default::default()
        };
        let config = EnrichmentConfig::resolve("issues", Some(&overrides)).unwrap();
        assert_eq!(config.cleaning_profile, CleaningProfile::Plain);
        assert_eq!(config.min_cluster_size, Some(25));
        assert_eq!(config.embedder.name(), "potion-base-32M");
        // Untouched fields keep the default
        assert_eq!(config.text_columns, vec!["title", "body"]);
    }

    #[test]
    fn from_stored_parses_raw_fields() {
        let o = EnrichmentOverrides::from_stored(
            Some(r#"["title","body"]"#),
            Some("plain".to_string()),
            None,
            Some("potion-base-32M".to_string()),
            Some(-3),
            None,
        );
        assert_eq!(
            o.text_columns,
            Some(vec!["title".to_string(), "body".to_string()])
        );
        assert_eq!(o.min_cluster_size, None, "negative sizes dropped");

        let bad =
            EnrichmentOverrides::from_stored(Some("not json"), None, None, None, Some(25), None);
        assert!(
            bad.text_columns.is_none(),
            "unparseable JSON treated as unset"
        );
        assert_eq!(bad.min_cluster_size, Some(25));

        let empty = EnrichmentOverrides::from_stored(Some("[]"), None, None, None, None, None);
        assert!(empty.text_columns.is_none(), "empty list treated as unset");
    }

    #[test]
    fn unknown_override_values_are_ignored() {
        let overrides = EnrichmentOverrides {
            cleaning_profile: Some("bogus".to_string()),
            embedder: Some("bogus".to_string()),
            ..Default::default()
        };
        let config = EnrichmentConfig::resolve("posts", Some(&overrides)).unwrap();
        assert_eq!(config.cleaning_profile, CleaningProfile::Social);
        assert_eq!(config.embedder, EmbedderId::PotionBase32M);
    }
}
