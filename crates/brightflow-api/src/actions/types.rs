//! First-class curation actions (Linear AIG model).
//!
//! One `Action` enum is the single source of truth for three surfaces:
//! - the REST body of `POST /api/actions` (serde),
//! - the typed frontend union (ts-rs),
//! - the LLM tool definitions (schemars JSON Schema via the manifest).
//!
//! A human clicking "rename cluster" and an agent emitting a tool call
//! execute the exact same code path; only the recorded actor differs.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A curation operation. Cluster actions key on the RAW cluster id of the
/// current fit; durable storage attaches to the cluster's centroid so edits
/// survive re-fits (see `cluster_edits` + reconciliation).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    /// Give a topic cluster a human-curated display name.
    RenameCluster {
        source_id: String,
        table: String,
        cluster_id: i64,
        name: String,
    },
    /// Fold one cluster into another; sizes sum, terms union, applied at
    /// read time (the fitted artifacts are untouched).
    MergeClusters {
        source_id: String,
        table: String,
        from_cluster_id: i64,
        into_cluster_id: i64,
    },
    /// Split an over-broad cluster by refitting with one more cluster slot.
    SplitCluster {
        source_id: String,
        table: String,
        cluster_id: i64,
    },
    /// Remove a term from cluster naming and top-term lists.
    ExcludeTerm {
        source_id: String,
        table: String,
        term: String,
    },
    /// Hide a cluster as noise (its rows count as unassigned).
    MarkClusterNoise {
        source_id: String,
        table: String,
        cluster_id: i64,
        is_noise: bool,
    },
    /// Attach a classification label to a cluster; regenerates the label
    /// centroids artifact used for `predicted_label`.
    AssignClusterLabel {
        source_id: String,
        table: String,
        cluster_id: i64,
        label: String,
    },
    /// Refit topic clusters. Not undoable.
    Recluster {
        source_id: String,
        table: String,
        k: Option<u32>,
        language: Option<String>,
        embedder: Option<String>,
        min_cluster_size: Option<u32>,
        /// Clustering algorithm; honored when the pipeline supports it
        /// (k-means today, hdbscan later).
        algorithm: Option<String>,
    },
    /// Hide an insight permanently (keyed by its stable fingerprint).
    DismissInsight {
        source_id: String,
        table: String,
        fingerprint: String,
        reason: DismissReason,
    },
    /// Pin an insight to the top of future runs (or unpin).
    PinInsight {
        source_id: String,
        table: String,
        fingerprint: String,
        pinned: bool,
    },
    /// Attach a free-text note to an insight.
    AnnotateInsight {
        source_id: String,
        table: String,
        fingerprint: String,
        note: String,
    },
    /// Never surface insights about this segment or column again.
    SuppressTarget {
        source_id: String,
        table: String,
        target_kind: SuppressKind,
        target: String,
    },
    /// Flag or unflag a column as KPI (delegates to column semantics).
    SetKpi {
        source_id: String,
        table: String,
        column: String,
        is_kpi: bool,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, JsonSchema, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum DismissReason {
    Boring,
    Known,
    Wrong,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, JsonSchema, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum SuppressKind {
    Segment,
    Column,
}

impl Action {
    /// Machine name matching the serde tag.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::RenameCluster { .. } => "rename_cluster",
            Self::MergeClusters { .. } => "merge_clusters",
            Self::SplitCluster { .. } => "split_cluster",
            Self::ExcludeTerm { .. } => "exclude_term",
            Self::MarkClusterNoise { .. } => "mark_cluster_noise",
            Self::AssignClusterLabel { .. } => "assign_cluster_label",
            Self::Recluster { .. } => "recluster",
            Self::DismissInsight { .. } => "dismiss_insight",
            Self::PinInsight { .. } => "pin_insight",
            Self::AnnotateInsight { .. } => "annotate_insight",
            Self::SuppressTarget { .. } => "suppress_target",
            Self::SetKpi { .. } => "set_kpi",
        }
    }

    /// (source_id, table) scope of the action.
    pub fn scope(&self) -> (&str, &str) {
        match self {
            Self::RenameCluster {
                source_id, table, ..
            }
            | Self::MergeClusters {
                source_id, table, ..
            }
            | Self::SplitCluster {
                source_id, table, ..
            }
            | Self::ExcludeTerm {
                source_id, table, ..
            }
            | Self::MarkClusterNoise {
                source_id, table, ..
            }
            | Self::AssignClusterLabel {
                source_id, table, ..
            }
            | Self::Recluster {
                source_id, table, ..
            }
            | Self::DismissInsight {
                source_id, table, ..
            }
            | Self::PinInsight {
                source_id, table, ..
            }
            | Self::AnnotateInsight {
                source_id, table, ..
            }
            | Self::SuppressTarget {
                source_id, table, ..
            }
            | Self::SetKpi {
                source_id, table, ..
            } => (source_id, table),
        }
    }
}

/// Envelope for `POST /api/actions`. The client-generated `request_id`
/// makes the dispatch idempotent: replays return the stored response.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ActionRequest {
    pub action: Action,
    pub request_id: String,
}

#[derive(Debug, Serialize, Deserialize, TS, Clone, Copy, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    Applied,
    Proposed,
    Rejected,
    Undone,
    Failed,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ActionResponse {
    pub log_id: i64,
    pub status: ActionStatus,
    #[ts(type = "unknown")]
    pub result: serde_json::Value,
}

/// One manifest entry: everything an LLM (or the UI) needs to know about an
/// available action.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ActionManifestEntry {
    pub kind: String,
    pub description: String,
    pub undoable: bool,
    /// JSON Schema for the action's parameters
    #[ts(type = "unknown")]
    pub schema: serde_json::Value,
}

/// One row in the audit feed (mirrors `action_log`).
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ActionLogEntry {
    pub id: i64,
    pub request_id: String,
    pub actor_type: String,
    #[ts(optional)]
    pub agent_run_id: Option<i64>,
    pub action_kind: String,
    #[ts(type = "unknown")]
    pub params: serde_json::Value,
    #[ts(optional, type = "unknown | null")]
    pub result: Option<serde_json::Value>,
    pub status: String,
    pub undoable: bool,
    pub created_at: i64,
    #[ts(optional)]
    pub resolved_at: Option<i64>,
}

/// Inverse operations stored in `undo_json`. Internal — never exposed as a
/// dispatchable action.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum UndoOp {
    /// Restore a cluster edit to a previous field snapshot.
    RestoreClusterEdit {
        table_id: String,
        centroid_fingerprint: String,
        centroid_json: String,
        cluster_id: Option<i64>,
        custom_name: Option<String>,
        label: Option<String>,
        is_noise: bool,
        merged_into: Option<i64>,
        /// True when the edit row did not exist before the action.
        delete_row: bool,
        /// Regenerate the label-centroids artifact after restoring.
        refresh_labels: bool,
    },
    RemoveExcludedTerm {
        table_id: String,
        term: String,
    },
    DeleteInsightState {
        table_id: String,
        fingerprint: String,
    },
    RestoreInsightState {
        table_id: String,
        fingerprint: String,
        state: String,
        reason: Option<String>,
        annotation: Option<String>,
    },
    DeleteSuppression {
        table_id: String,
        kind: String,
        target: String,
    },
    RestoreKpi {
        source_id: String,
        table: String,
        column: String,
        role: String,
        is_kpi: bool,
    },
}

/// Static list of action kinds with undoability — the manifest registry.
pub const ACTION_KINDS: &[(&str, &str, bool)] = &[
    (
        "rename_cluster",
        "Give a topic cluster a human-curated display name",
        true,
    ),
    (
        "merge_clusters",
        "Fold one cluster into another (read-time overlay)",
        true,
    ),
    (
        "split_cluster",
        "Split an over-broad cluster by refitting with one more cluster",
        false,
    ),
    (
        "exclude_term",
        "Remove a term from cluster naming and top-term lists",
        true,
    ),
    (
        "mark_cluster_noise",
        "Hide a cluster as noise (rows count as unassigned)",
        true,
    ),
    (
        "assign_cluster_label",
        "Attach a classification label to a cluster",
        true,
    ),
    ("recluster", "Refit topic clusters", false),
    (
        "dismiss_insight",
        "Hide an insight permanently (boring/known/wrong)",
        true,
    ),
    (
        "pin_insight",
        "Pin an insight to the top of future runs",
        true,
    ),
    ("annotate_insight", "Attach a note to an insight", true),
    (
        "suppress_target",
        "Never surface insights about this segment or column",
        true,
    ),
    ("set_kpi", "Flag or unflag a column as KPI", true),
];

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The manifest registry must cover every Action variant — adding a
    /// variant without a manifest entry is a compile-adjacent error.
    #[test]
    fn manifest_registry_is_complete() {
        let samples: Vec<Action> = vec![
            Action::RenameCluster {
                source_id: String::new(),
                table: String::new(),
                cluster_id: 0,
                name: String::new(),
            },
            Action::MergeClusters {
                source_id: String::new(),
                table: String::new(),
                from_cluster_id: 0,
                into_cluster_id: 1,
            },
            Action::SplitCluster {
                source_id: String::new(),
                table: String::new(),
                cluster_id: 0,
            },
            Action::ExcludeTerm {
                source_id: String::new(),
                table: String::new(),
                term: String::new(),
            },
            Action::MarkClusterNoise {
                source_id: String::new(),
                table: String::new(),
                cluster_id: 0,
                is_noise: true,
            },
            Action::AssignClusterLabel {
                source_id: String::new(),
                table: String::new(),
                cluster_id: 0,
                label: String::new(),
            },
            Action::Recluster {
                source_id: String::new(),
                table: String::new(),
                k: None,
                language: None,
                embedder: None,
                min_cluster_size: None,
                algorithm: None,
            },
            Action::DismissInsight {
                source_id: String::new(),
                table: String::new(),
                fingerprint: String::new(),
                reason: DismissReason::Boring,
            },
            Action::PinInsight {
                source_id: String::new(),
                table: String::new(),
                fingerprint: String::new(),
                pinned: true,
            },
            Action::AnnotateInsight {
                source_id: String::new(),
                table: String::new(),
                fingerprint: String::new(),
                note: String::new(),
            },
            Action::SuppressTarget {
                source_id: String::new(),
                table: String::new(),
                target_kind: SuppressKind::Segment,
                target: String::new(),
            },
            Action::SetKpi {
                source_id: String::new(),
                table: String::new(),
                column: String::new(),
                is_kpi: true,
            },
        ];
        assert_eq!(
            samples.len(),
            ACTION_KINDS.len(),
            "every Action variant needs a manifest entry"
        );
        for action in &samples {
            assert!(
                ACTION_KINDS.iter().any(|(k, _, _)| *k == action.kind()),
                "missing manifest entry for {}",
                action.kind()
            );
        }
    }

    #[test]
    fn action_serde_round_trip() {
        let action = Action::DismissInsight {
            source_id: "s".to_string(),
            table: "posts".to_string(),
            fingerprint: "abc".to_string(),
            reason: DismissReason::Known,
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"kind\":\"dismiss_insight\""));
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(back.kind(), "dismiss_insight");
    }
}
