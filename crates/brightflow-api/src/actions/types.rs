//! First-class curation actions (Linear AIG model).
//!
//! One `Action` enum is the single source of truth for three surfaces:
//! - the REST body of `POST /api/actions` (serde),
//! - the typed frontend union (ts-rs),
//! - the LLM tool definitions (schemars JSON Schema via the manifest).
//!
//! A human clicking "rename cluster" and an agent emitting a tool call
//! execute the exact same code path; only the recorded actor differs.

use brightflow_engine::data::config::{ColumnRole, Polarity, TimeGranularity};
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

/// A curation operation: a vocabulary edit, an insight verdict, or a column
/// semantic. Every one is logged with its actor and, where the manifest says
/// so, undoable.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
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
    /// Remove every entry of one vocabulary kind on the table, children
    /// first: `category` takes its subcategories with it, `product` its
    /// components. Frozen entries go too — this is the clean-slate action —
    /// and undo puts every row back under its original id, frozen flag
    /// included.
    ClearVocabulary {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        /// category | feedback_category | product | competitor
        vocab_kind: String,
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
        polarity: Polarity,
    },
    /// Declare what a column is for: a measure to aggregate, a dimension to
    /// slice by, the time axis, an entity id, or ignored. Explore hides
    /// ignored columns and buckets time columns; the engine analyses by role.
    SetColumnRole {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        column: String,
        role: ColumnRole,
    },
    /// Give a column a human-facing name. `None` (or blank) clears it and the
    /// UI falls back to the humanised column name.
    SetColumnLabel {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        column: String,
        #[serde(default)]
        #[ts(optional)]
        label: Option<String>,
    },
    /// Describe a column in one or two sentences, shown as help text beside
    /// its label. `None` (or blank) clears it.
    SetColumnDescription {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        column: String,
        #[serde(default)]
        #[ts(optional)]
        description: Option<String>,
    },
    /// The table's own settings — display name, description, analysis
    /// period and how many periods to compare — as one opinion at the
    /// actor's layer. A field left out is "no opinion", so a producer's
    /// value shows through; a blank string clears a text field.
    SetTableSettings {
        #[serde(flatten)]
        #[ts(flatten)]
        scope: Scope,
        #[serde(default)]
        #[ts(optional)]
        display_name: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        description: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        time_granularity: Option<TimeGranularity>,
        #[serde(default)]
        #[ts(optional)]
        comparison_periods: Option<u32>,
    },
}

/// One column's full semantic tuple as stored in `column_semantics`.
///
/// What the semantic executors snapshot before a mutation and what their
/// undo writes back; also the shape a first-time write is seeded from.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ColumnSemanticSnapshot {
    #[serde(default)]
    pub role: Option<ColumnRole>,
    #[serde(default)]
    pub is_kpi: Option<bool>,
    #[serde(default)]
    pub polarity: Option<Polarity>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

impl ColumnSemanticSnapshot {
    /// The undoable fields of one opinion row.
    pub fn from_opinion(o: &brightflow_types::ColumnOpinion) -> Self {
        Self {
            role: o.ext.role,
            is_kpi: o.ext.is_kpi,
            polarity: o.ext.polarity,
            label: o.ext.label.clone(),
            description: o.description.clone(),
        }
    }

    /// Write the snapshot's fields onto an opinion row.
    pub fn apply_to(&self, o: &mut brightflow_types::ColumnOpinion) {
        o.ext.role = self.role;
        o.ext.is_kpi = self.is_kpi;
        o.ext.polarity = self.polarity;
        o.ext.label.clone_from(&self.label);
        o.description.clone_from(&self.description);
    }
}

/// The four settings fields of one table opinion row, as undo writes them
/// back.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TableSettingsSnapshot {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub time_granularity: Option<TimeGranularity>,
    #[serde(default)]
    pub comparison_periods: Option<u32>,
}

impl TableSettingsSnapshot {
    pub fn from_opinion(o: &brightflow_types::TableOpinion) -> Self {
        Self {
            display_name: o.display_name.clone(),
            description: o.description.clone(),
            time_granularity: o.time_granularity,
            comparison_periods: o.comparison_periods,
        }
    }

    pub fn apply_to(&self, o: &mut brightflow_types::TableOpinion) {
        o.display_name.clone_from(&self.display_name);
        o.description.clone_from(&self.description);
        o.time_granularity = self.time_granularity;
        o.comparison_periods = self.comparison_periods;
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
            Self::DefineTaxonomyCategory { .. } => "define_taxonomy_category",
            Self::RenameTaxonomyCategory { .. } => "rename_taxonomy_category",
            Self::RedefineTaxonomyCategory { .. } => "redefine_taxonomy_category",
            Self::FreezeTaxonomyCategory { .. } => "freeze_taxonomy_category",
            Self::DeleteTaxonomyCategory { .. } => "delete_taxonomy_category",
            Self::ClearVocabulary { .. } => "clear_vocabulary",
            Self::DismissInsight { .. } => "dismiss_insight",
            Self::PinInsight { .. } => "pin_insight",
            Self::AnnotateInsight { .. } => "annotate_insight",
            Self::SuppressTarget { .. } => "suppress_target",
            Self::SetKpi { .. } => "set_kpi",
            Self::SetColumnPolarity { .. } => "set_column_polarity",
            Self::SetColumnRole { .. } => "set_column_role",
            Self::SetColumnLabel { .. } => "set_column_label",
            Self::SetColumnDescription { .. } => "set_column_description",
            Self::SetTableSettings { .. } => "set_table_settings",
        }
    }

    /// (source_id, table) scope of the action.
    pub fn scope(&self) -> (&str, &str) {
        let scope = match self {
            Self::DefineTaxonomyCategory { scope, .. }
            | Self::RenameTaxonomyCategory { scope, .. }
            | Self::RedefineTaxonomyCategory { scope, .. }
            | Self::FreezeTaxonomyCategory { scope, .. }
            | Self::DeleteTaxonomyCategory { scope, .. }
            | Self::ClearVocabulary { scope, .. }
            | Self::DismissInsight { scope, .. }
            | Self::PinInsight { scope, .. }
            | Self::AnnotateInsight { scope, .. }
            | Self::SuppressTarget { scope, .. }
            | Self::SetKpi { scope, .. }
            | Self::SetColumnPolarity { scope, .. }
            | Self::SetColumnRole { scope, .. }
            | Self::SetColumnLabel { scope, .. }
            | Self::SetColumnDescription { scope, .. }
            | Self::SetTableSettings { scope, .. } => scope,
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
    /// Undo of every column-semantic action: put the actor's opinion row
    /// back as it was, or delete it when the action created it. Rows logged
    /// before layers existed have no `provenance` and restore at the user
    /// layer.
    RestoreColumnSemantic {
        source_id: String,
        table: String,
        column: String,
        snapshot: ColumnSemanticSnapshot,
        #[serde(default)]
        provenance: Option<brightflow_types::Provenance>,
        #[serde(default = "default_true")]
        existed: bool,
    },
    /// Undo of set_table_settings: put the actor's table row back, or
    /// delete it when the action created it.
    RestoreTableSettings {
        source_id: String,
        table: String,
        snapshot: TableSettingsSnapshot,
        provenance: brightflow_types::Provenance,
        existed: bool,
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
    RestoreTaxonomyFrozen { category_id: i64, frozen: bool },
    /// Undo of delete: recreate the entry under its original id.
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
    /// Undo of clear: every deleted row, parents before children, recreated
    /// under its original id for the same reason as above.
    RecreateVocabulary {
        rows: Vec<brightflow_store::TaxonomyCategoryRow>,
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
        "clear_vocabulary",
        "Clear vocabulary",
        "Delete every entry of one vocabulary kind on the table, children first; undo restores them all",
        true,
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
    (
        "set_column_role",
        "Set column role",
        "Declare what a column is for: measure (a number to aggregate), \
         dimension (a category to slice by), time (the time axis), entity \
         (an identifier such as a user or account), or ignored (hidden from \
         analysis and from Explore). Setting a KPI column to anything but \
         measure also clears its KPI flag.",
        true,
    ),
    (
        "set_column_label",
        "Rename column",
        "Give a column a short human-facing name shown everywhere instead of \
         its raw name. Omit or blank the label to clear it.",
        true,
    ),
    (
        "set_column_description",
        "Describe column",
        "Attach a one- or two-sentence description to a column, shown as help \
         text beside its label. Omit or blank the description to clear it.",
        true,
    ),
    (
        "set_table_settings",
        "Table settings",
        "Set the table's display name, description, analysis period \
         (day, week, month, quarter or year) and how many periods to compare. \
         Give only the fields to change; a blank string clears a text field.",
        true,
    ),
];

const fn default_true() -> bool {
    true
}

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
            Action::ClearVocabulary {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                vocab_kind: String::new(),
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
                polarity: Polarity::HigherIsBetter,
            },
            Action::SetColumnRole {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                column: String::new(),
                role: ColumnRole::Dimension,
            },
            Action::SetColumnLabel {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                column: String::new(),
                label: None,
            },
            Action::SetColumnDescription {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                column: String::new(),
                description: None,
            },
            Action::SetTableSettings {
                scope: Scope {
                    source_id: String::new(),
                    table: String::new(),
                },
                display_name: None,
                description: None,
                time_granularity: None,
                comparison_periods: None,
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
