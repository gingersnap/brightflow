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

/// The (source_id, table) pair every action carries. Inlined into each
/// variant's JSON/TS shape via flatten — not a wire type of its own.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct Scope {
    pub source_id: String,
    pub table: String,
}

/// A curation operation. Cluster actions key on the RAW cluster id of the
/// current fit; durable storage attaches to the cluster's centroid so edits
/// survive re-fits (see `cluster_edits` + reconciliation).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    /// Give a topic cluster a human-curated display name.
    RenameCluster {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        #[ts(type = "number")]
        cluster_id: i64,
        name: String,
    },
    /// Fold one cluster into another; sizes sum, terms union, applied at
    /// read time (the fitted artifacts are untouched).
    MergeClusters {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        #[ts(type = "number")]
        from_cluster_id: i64,
        #[ts(type = "number")]
        into_cluster_id: i64,
    },
    /// Split an over-broad cluster by refitting with one more cluster slot.
    SplitCluster {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        #[ts(type = "number")]
        cluster_id: i64,
    },
    /// Remove a term from cluster naming and top-term lists.
    ExcludeTerm {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        term: String,
    },
    /// Hide a cluster as noise (its rows count as unassigned).
    MarkClusterNoise {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        #[ts(type = "number")]
        cluster_id: i64,
        is_noise: bool,
    },
    /// Attach a classification label to a cluster (display/curation only).
    ///
    /// This does NOT drive `predicted_label` any more. It used to, via a
    /// circular path: the label's centroid was built from the CLUSTER centroid,
    /// so `predicted_label` was just the cluster assignment wearing a nicer
    /// name — and since clusters are format-shaped, that re-taught the format
    /// bias. Row-level intent labels (`LabelDocument`) train the classifier
    /// head, which supersedes this as the labeler.
    AssignClusterLabel {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        #[ts(type = "number")]
        cluster_id: i64,
        label: String,
    },
    /// Add an entry to one of the table's vocabularies. `kind` defaults to
    /// `category`; `parent_id` (0 or absent = root) places subcategories under
    /// a category and product components under an area. Idempotent on
    /// (table, kind, parent, name). Refused at the level's cap and for the
    /// reserved name `other`.
    DefineTaxonomyCategory {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        name: String,
        description: Option<String>,
        /// category | subcategory | feedback_category | product | competitor.
        /// Not `kind` — that is the enum's serde tag.
        #[serde(default)]
        #[ts(optional)]
        vocab_kind: Option<String>,
        #[serde(default)]
        #[ts(optional, type = "number")]
        parent_id: Option<i64>,
        /// Accepted surface forms (imported kinds); appended, never replaced.
        #[serde(default)]
        #[ts(optional)]
        aliases: Option<Vec<String>>,
    },
    /// Rename a vocabulary entry. The human's right to fix the LLM's wording;
    /// a rename never changes what the model classifies against, so it never
    /// invalidates enrichment cells. Refused on a frozen entry.
    RenameTaxonomyCategory {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        #[ts(type = "number")]
        category_id: i64,
        name: String,
    },
    /// Replace an entry's description — its definition. Unlike a rename this
    /// changes what the model sees, so it is a recalibration event: every
    /// cell classified under the old definition recomputes. Refused on a
    /// frozen entry.
    RedefineTaxonomyCategory {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        #[ts(type = "number")]
        category_id: i64,
        description: Option<String>,
    },
    /// Freeze or unfreeze an entry. Frozen entries refuse rename, redefine and
    /// delete — how `category` holds the long-horizon trend line while the
    /// levels below it are recalibrated.
    FreezeTaxonomyCategory {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        #[ts(type = "number")]
        category_id: i64,
        frozen: bool,
    },
    /// Remove a vocabulary entry; its row labels cascade away with it. Refused
    /// while the entry has children or is frozen.
    DeleteTaxonomyCategory {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        #[ts(type = "number")]
        category_id: i64,
    },
    /// Set a row's intent labels, replacing whatever it had. Multi-label: a
    /// ticket may have several intents, or none (pass an empty list to clear).
    ///
    /// Row-level on purpose — this is the supervision that breaks the
    /// format-cluster loop.
    LabelDocument {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        row_id: String,
        categories: Vec<String>,
    },
    /// Refit topic clusters. Not undoable.
    Recluster {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
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
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        fingerprint: String,
        reason: DismissReason,
    },
    /// Pin an insight to the top of future runs (or unpin).
    PinInsight {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        fingerprint: String,
        pinned: bool,
    },
    /// Attach a free-text note to an insight.
    AnnotateInsight {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        fingerprint: String,
        note: String,
    },
    /// Never surface insights about this segment or column again.
    SuppressTarget {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        target_kind: SuppressKind,
        target: String,
    },
    /// Flag or unflag a column as KPI (delegates to column semantics).
    SetKpi {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        column: String,
        is_kpi: bool,
    },
    /// Declare which direction of movement in a measure is good news
    /// (delegates to column semantics; display-only in scoring v1).
    SetColumnPolarity {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        column: String,
        polarity: ColumnPolarity,
    },
}

/// Measure polarity values (mirrors `brightflow_engine::data::config::Polarity`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, JsonSchema, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ColumnPolarity {
    HigherIsBetter,
    LowerIsBetter,
    Neutral,
}

impl ColumnPolarity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HigherIsBetter => "higher_is_better",
            Self::LowerIsBetter => "lower_is_better",
            Self::Neutral => "neutral",
        }
    }
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
            Self::DefineTaxonomyCategory { .. } => "define_taxonomy_category",
            Self::RenameTaxonomyCategory { .. } => "rename_taxonomy_category",
            Self::RedefineTaxonomyCategory { .. } => "redefine_taxonomy_category",
            Self::FreezeTaxonomyCategory { .. } => "freeze_taxonomy_category",
            Self::DeleteTaxonomyCategory { .. } => "delete_taxonomy_category",
            Self::LabelDocument { .. } => "label_document",
            Self::Recluster { .. } => "recluster",
            Self::DismissInsight { .. } => "dismiss_insight",
            Self::PinInsight { .. } => "pin_insight",
            Self::AnnotateInsight { .. } => "annotate_insight",
            Self::SuppressTarget { .. } => "suppress_target",
            Self::SetKpi { .. } => "set_kpi",
            Self::SetColumnPolarity { .. } => "set_column_polarity",
        }
    }

    /// (source_id, table) scope of the action.
    pub fn scope(&self) -> (&str, &str) {
        let scope = match self {
            Self::RenameCluster { scope, .. }
            | Self::MergeClusters { scope, .. }
            | Self::SplitCluster { scope, .. }
            | Self::ExcludeTerm { scope, .. }
            | Self::MarkClusterNoise { scope, .. }
            | Self::AssignClusterLabel { scope, .. }
            | Self::DefineTaxonomyCategory { scope, .. }
            | Self::RenameTaxonomyCategory { scope, .. }
            | Self::RedefineTaxonomyCategory { scope, .. }
            | Self::FreezeTaxonomyCategory { scope, .. }
            | Self::DeleteTaxonomyCategory { scope, .. }
            | Self::LabelDocument { scope, .. }
            | Self::Recluster { scope, .. }
            | Self::DismissInsight { scope, .. }
            | Self::PinInsight { scope, .. }
            | Self::AnnotateInsight { scope, .. }
            | Self::SuppressTarget { scope, .. }
            | Self::SetKpi { scope, .. }
            | Self::SetColumnPolarity { scope, .. } => scope,
        };
        (&scope.source_id, &scope.table)
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
    #[ts(type = "number")]
    pub log_id: i64,
    pub status: ActionStatus,
    #[ts(type = "unknown")]
    pub result: serde_json::Value,
}

/// Outcome of a bulk approval.
///
/// Reports failures rather than throwing on the first one: proposals are
/// independent, and one bad apple (say a `label_document` naming a category that
/// was since deleted) must not block the other 1,199.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct BulkApproveResponse {
    /// Proposals that were pending when the sweep started.
    pub total: usize,
    pub approved: usize,
    pub failed: usize,
    /// Log ids that failed, with why — capped so a pathological run cannot
    /// return a megabyte of errors.
    pub failures: Vec<BulkApproveFailure>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct BulkApproveFailure {
    #[ts(type = "number")]
    pub log_id: i64,
    pub action_kind: String,
    pub error: String,
}

/// Outcome of a run-level bulk undo (`POST /api/agent/runs/{id}/undo-all`).
///
/// Same shape philosophy as `BulkApproveResponse`: continue on per-row
/// failure and report what happened. A partial failure leaves the run
/// half-reverted — the failure report is the answer, not a rollback.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct BulkUndoResponse {
    /// Applied, undoable actions of the run when the sweep started.
    pub total: usize,
    pub undone: usize,
    pub failed: usize,
    pub failures: Vec<BulkApproveFailure>,
}

/// Number of proposals awaiting review.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PendingCount {
    /// `usize`, not `i64`: ts-rs maps i64 to `bigint`, and a count that arrives
    /// as a bigint cannot be compared or rendered alongside plain numbers
    /// without ceremony. A count is never negative anyway.
    pub count: usize,
}

/// One manifest entry: everything an LLM (or the UI) needs to know about an
/// available action.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ActionManifestEntry {
    pub kind: String,
    /// Short human-facing name (command palette, buttons).
    pub label: String,
    /// Long form — fed verbatim to the LLM as the tool description.
    pub description: String,
    pub undoable: bool,
    /// JSON Schema for the action's parameters
    #[ts(type = "unknown")]
    pub schema: serde_json::Value,
}

/// One row in the audit feed (mirrors `action_log`).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ActionLogEntry {
    // `number`, not the default `bigint`: these arrive via JSON.parse as plain
    // numbers at runtime, and sqlite rowids / epoch-ms stay well inside 2^53.
    #[ts(type = "number")]
    pub id: i64,
    pub request_id: String,
    pub actor_type: String,
    #[ts(optional, type = "number")]
    pub agent_run_id: Option<i64>,
    /// The human who acted; absent for agent rows.
    #[ts(optional)]
    pub user_id: Option<String>,
    pub action_kind: String,
    #[ts(type = "unknown")]
    pub params: serde_json::Value,
    #[ts(optional, type = "unknown | null")]
    pub result: Option<serde_json::Value>,
    pub status: String,
    pub undoable: bool,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(optional, type = "number")]
    pub resolved_at: Option<i64>,
}

impl ActionLogEntry {
    /// The one place the row→entry mapping (and the undoability rule) lives.
    /// The feed endpoint and every WS emit site go through here.
    pub fn from_row(row: brightflow_store::ActionLogRow) -> Self {
        Self {
            undoable: row.undo_json.is_some() && row.status == "applied",
            id: row.id,
            request_id: row.request_id,
            actor_type: row.actor_type,
            agent_run_id: row.agent_run_id,
            user_id: row.user_id,
            action_kind: row.action_kind,
            params: serde_json::from_str(&row.params_json).unwrap_or(serde_json::Value::Null),
            result: row
                .result_json
                .as_deref()
                .and_then(|j| serde_json::from_str(j).ok()),
            status: row.status,
            created_at: row.created_at,
            resolved_at: row.resolved_at,
        }
    }
}

/// Whether an action kind is undoable per the manifest registry.
pub fn kind_is_undoable(kind: &str) -> bool {
    ACTION_KINDS
        .iter()
        .any(|(k, _, _, undoable)| *k == kind && *undoable)
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
    /// Undo of set_column_polarity: restore the previous polarity string.
    RestorePolarity {
        source_id: String,
        table: String,
        column: String,
        polarity: String,
    },
    /// Undo of define/rename/redefine: put an entry's name and description
    /// back, or delete it outright when it did not exist before the action.
    RestoreTaxonomyCategory {
        category_id: i64,
        /// True when the category did not exist before the action.
        delete_row: bool,
        name: Option<String>,
        description: Option<String>,
    },
    /// Undo of freeze: put the previous frozen flag back.
    RestoreTaxonomyFrozen {
        category_id: i64,
        frozen: bool,
    },
    /// Undo of delete: recreate the entry AND the row labels that cascaded
    /// away with it.
    ///
    /// The labels are the whole point — deleting a category silently destroys
    /// human curation effort, which is the most expensive input to this system,
    /// so the undo has to bring them back rather than just the category name.
    /// Reinserting under the original `category_id` is what keeps them attached.
    RecreateTaxonomyCategory {
        table_id: String,
        category_id: i64,
        name: String,
        description: Option<String>,
        created_at: i64,
        /// (row_id, source, created_at) of every cascaded label.
        labels: Vec<(String, String, i64)>,
        /// Defaults keep undo rows written before the hierarchy readable.
        #[serde(default = "default_kind")]
        kind: String,
        #[serde(default)]
        parent_id: i64,
        #[serde(default)]
        frozen: bool,
        #[serde(default)]
        aliases_json: Option<String>,
    },
    /// Undo of label_document: restore a row's exact prior label set (empty =
    /// the row was unlabelled).
    RestoreDocumentLabels {
        table_id: String,
        row_id: String,
        /// (category_id, source, created_at).
        labels: Vec<(i64, String, i64)>,
    },
}

fn default_kind() -> String {
    "category".to_string()
}

/// Static list of `(kind, label, description, undoable)` — the manifest registry.
///
/// `label` is the short human-facing name (command palette, UI);
/// `description` is the long form fed verbatim to the LLM as the tool
/// description. Keep the order: `manifest_schemas` in the agent runner
/// destructures positionally and label/description are both `&str`, so a
/// swap compiles silently.
pub const ACTION_KINDS: &[(&str, &str, &str, bool)] = &[
    (
        "rename_cluster",
        "Rename cluster",
        "Give a topic cluster a human-curated display name",
        true,
    ),
    (
        "merge_clusters",
        "Merge clusters",
        "Fold one cluster into another (read-time overlay)",
        true,
    ),
    (
        "split_cluster",
        "Split cluster",
        "Split an over-broad cluster by refitting with one more cluster",
        false,
    ),
    (
        "exclude_term",
        "Exclude term",
        "Remove a term from cluster naming and top-term lists",
        true,
    ),
    (
        "mark_cluster_noise",
        "Mark cluster as noise",
        "Hide a cluster as noise (rows count as unassigned)",
        true,
    ),
    (
        "assign_cluster_label",
        "Assign cluster label",
        "Attach a classification label to a cluster (display only)",
        true,
    ),
    (
        "define_taxonomy_category",
        "Define vocabulary entry",
        "Define one vocabulary entry. For categories: what is WRONG for the user \
         (e.g. 'authentication failure', 'data loss on sync'). Never categorize \
         by tooling, file format, or mechanism (e.g. 'backport commits', 'stack \
         traces') — those describe how a ticket is written, not what it is about. \
         Give a short name and a one-sentence description. Pass `vocab_kind` and, for a \
         subcategory or product component, `parent_id`. Do not define 'other' — \
         it exists implicitly at every level.",
        true,
    ),
    (
        "rename_taxonomy_category",
        "Rename vocabulary entry",
        "Rename an existing vocabulary entry (label only; no recompute)",
        true,
    ),
    (
        "redefine_taxonomy_category",
        "Redefine vocabulary entry",
        "Replace an entry's description — the definition the model classifies \
         against. Recomputes every affected cell.",
        true,
    ),
    (
        "freeze_taxonomy_category",
        "Freeze vocabulary entry",
        "Freeze (or unfreeze) an entry so it cannot be renamed, redefined or deleted",
        true,
    ),
    (
        "delete_taxonomy_category",
        "Delete vocabulary entry",
        "Delete a vocabulary entry and all of its row labels",
        true,
    ),
    (
        "label_document",
        "Label document",
        "Assign intent categories to ONE ticket, replacing its current labels. \
         Choose from the approved taxonomy only. Judge by what problem the ticket \
         describes, not by how it is formatted. Pass an empty list if no category \
         applies; pass several if several genuinely apply.",
        true,
    ),
    (
        "recluster",
        "Recluster topics",
        "Refit topic clusters",
        false,
    ),
    (
        "dismiss_insight",
        "Dismiss insight",
        "Hide an insight permanently (boring/known/wrong)",
        true,
    ),
    (
        "pin_insight",
        "Pin insight",
        "Pin an insight to the top of future runs",
        true,
    ),
    (
        "annotate_insight",
        "Annotate insight",
        "Attach a note to an insight",
        true,
    ),
    (
        "suppress_target",
        "Suppress insight target",
        "Never surface insights about this segment or column",
        true,
    ),
    ("set_kpi", "Set KPI", "Flag or unflag a column as KPI", true),
    (
        "set_column_polarity",
        "Set measure polarity",
        "Declare whether rising values of a measure are good news \
         (higher_is_better), bad news (lower_is_better), or neither (neutral). \
         Findings about the measure are then framed as good or bad.",
        true,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every Action variant must have an `ACTION_KINDS` entry, checked against
    /// the schema **derived from the enum** rather than a hand-written list.
    ///
    /// This closes the hole in `manifest_registry_is_complete`: that test only
    /// compares two hand-maintained lists, so adding a variant while forgetting
    /// BOTH the sample and the manifest entry keeps the counts equal and passes
    /// silently. schemars reads the real enum, so nothing can drift past it.
    #[test]
    fn every_variant_has_a_manifest_entry() {
        let root = schemars::schema_for!(Action);
        let value = serde_json::to_value(&root).expect("schema serializes");
        let variants = value
            .get("oneOf")
            .and_then(|v| v.as_array())
            .expect("Action is an internally-tagged enum, so schemars emits oneOf");

        let schema_kinds: Vec<String> = variants
            .iter()
            .filter_map(|v| {
                v.pointer("/properties/kind/const")
                    .and_then(|c| c.as_str())
                    .map(str::to_string)
            })
            .collect();
        assert_eq!(
            schema_kinds.len(),
            variants.len(),
            "every variant must carry a kind const"
        );

        for kind in &schema_kinds {
            assert!(
                ACTION_KINDS.iter().any(|(k, _, _, _)| k == kind),
                "Action variant '{kind}' has no ACTION_KINDS entry — the LLM manifest \
                 and the frontend would silently not know about it"
            );
        }
        for (kind, _, _, _) in ACTION_KINDS {
            assert!(
                schema_kinds.iter().any(|k| k == kind),
                "ACTION_KINDS lists '{kind}' but no such Action variant exists"
            );
        }
    }

    /// Manifest tool descriptions are fed verbatim to the LLM, so an empty one
    /// ships a nameless tool.
    #[test]
    fn manifest_descriptions_are_non_empty() {
        for (kind, _, description, _) in ACTION_KINDS {
            assert!(
                !description.trim().is_empty(),
                "'{kind}' needs a description — it becomes the LLM tool description"
            );
        }
    }

    /// Labels are the short UI names; descriptions are the long LLM prose.
    /// A label materially shorter than its description is the tell that the
    /// two positional `&str` fields haven't been swapped.
    #[test]
    fn manifest_labels_are_short_ui_names() {
        for (kind, label, description, _) in ACTION_KINDS {
            assert!(
                !label.trim().is_empty(),
                "'{kind}' needs a label — it names the palette command"
            );
            assert!(
                label.len() < description.len(),
                "'{kind}' label is not shorter than its description — \
                 label/description swapped?"
            );
            assert!(
                label.len() <= 40,
                "'{kind}' label '{label}' is too long for a command name"
            );
        }
    }

    /// The manifest registry must cover every Action variant — adding a
    /// variant without a manifest entry is a compile-adjacent error.
    #[test]
    fn manifest_registry_is_complete() {
        let samples: Vec<Action> = vec![
            Action::RenameCluster {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                cluster_id: 0,
                name: String::new(),
            },
            Action::MergeClusters {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                from_cluster_id: 0,
                into_cluster_id: 1,
            },
            Action::SplitCluster {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                cluster_id: 0,
            },
            Action::ExcludeTerm {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                term: String::new(),
            },
            Action::MarkClusterNoise {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                cluster_id: 0,
                is_noise: true,
            },
            Action::AssignClusterLabel {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                cluster_id: 0,
                label: String::new(),
            },
            Action::DefineTaxonomyCategory {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                name: String::new(),
                description: None,
                vocab_kind: None,
                parent_id: None,
                aliases: None,
            },
            Action::RedefineTaxonomyCategory {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                category_id: 0,
                description: None,
            },
            Action::FreezeTaxonomyCategory {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                category_id: 0,
                frozen: true,
            },
            Action::RenameTaxonomyCategory {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                category_id: 0,
                name: String::new(),
            },
            Action::DeleteTaxonomyCategory {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                category_id: 0,
            },
            Action::LabelDocument {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                row_id: String::new(),
                categories: Vec::new(),
            },
            Action::Recluster {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                k: None,
                language: None,
                embedder: None,
                min_cluster_size: None,
                algorithm: None,
            },
            Action::DismissInsight {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                fingerprint: String::new(),
                reason: DismissReason::Boring,
            },
            Action::PinInsight {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                fingerprint: String::new(),
                pinned: true,
            },
            Action::AnnotateInsight {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                fingerprint: String::new(),
                note: String::new(),
            },
            Action::SuppressTarget {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                target_kind: SuppressKind::Segment,
                target: String::new(),
            },
            Action::SetKpi {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                column: String::new(),
                is_kpi: true,
            },
            Action::SetColumnPolarity {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                column: String::new(),
                polarity: ColumnPolarity::HigherIsBetter,
            },
        ];
        assert_eq!(
            samples.len(),
            ACTION_KINDS.len(),
            "every Action variant needs a manifest entry"
        );
        for action in &samples {
            assert!(
                ACTION_KINDS.iter().any(|(k, _, _, _)| *k == action.kind()),
                "missing manifest entry for {}",
                action.kind()
            );
        }
    }

    #[test]
    fn action_serde_round_trip() {
        let action = Action::DismissInsight {
            scope: Scope {
                source_id: "s".to_string(),
                table: "posts".to_string(),
            },
            fingerprint: "abc".to_string(),
            reason: DismissReason::Known,
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"kind\":\"dismiss_insight\""));
        // Flatten keeps scope fields inline at the top level of the object.
        assert!(json.contains("\"source_id\":\"s\""));
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(back.kind(), "dismiss_insight");
        assert_eq!(back.scope(), ("s", "posts"));
    }
}
