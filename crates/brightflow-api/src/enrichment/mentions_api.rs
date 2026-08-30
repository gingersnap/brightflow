//! Mention-grain endpoints: the headline numbers per subject, the
//! unresolved-subject review queue, and CSV import of the imported
//! vocabularies (products, competitors).
//!
//! Every write here goes through the action bus so it is logged with the
//! human who did it and is undoable like any other curation edit; the queue
//! row's status is the one thing written directly, since it is bookkeeping
//! about the queue rather than about the vocabulary.

use axum::extract::{Path, Query, State};
use axum::Json;
use polars::prelude::*;
use serde::Deserialize;

use brightflow_engine::enrichment::mentions::mentions_table_name;
use brightflow_engine::enrichment::VocabKind;

use super::types::{
    ImportVocabularyResponse, MentionSubjectStats, MentionSummaryResponse,
    UnresolvedSubjectResponse, UpdateUnresolvedRequest,
};
use crate::actions::types::{Action, ActionStatus, Scope};
use crate::actions::{dispatch_action, Actor};
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

fn user_id(auth_session: &crate::auth::AuthSession) -> AppResult<String> {
    auth_session
        .user
        .as_ref()
        .map(|u| u.id.clone())
        .ok_or(AppError::Unauthorized)
}

/// Per-subject headline numbers from a mentions frame.
///
/// `distinct_tickets` is the headline: repeated complaints and long calls
/// inflate `mentions`. Unresolved subjects are reported under their surface
/// so the queue's signal shows up here too.
pub fn summarize_mentions(df: &DataFrame) -> PolarsResult<Vec<MentionSubjectStats>> {
    if df.height() == 0 {
        return Ok(Vec::new());
    }
    let out = df
        .clone()
        .lazy()
        .with_columns([
            coalesce(&[col("subject"), col("subject_surface")]).alias("subject_key"),
            col("subject_id").is_not_null().alias("resolved"),
            col("polarity")
                .eq(lit("negative"))
                .cast(DataType::Float64)
                .alias("neg"),
            col("incidental").cast(DataType::Float64).alias("inc"),
        ])
        .filter(col("subject_key").is_not_null())
        .group_by([col("type"), col("subject_key"), col("resolved")])
        .agg([
            len().alias("mentions"),
            col("ticket_id").n_unique().alias("distinct_tickets"),
            col("neg").mean().alias("negative_share"),
            col("inc").mean().alias("incidental_share"),
        ])
        .sort(
            ["distinct_tickets", "mentions"],
            SortMultipleOptions::default().with_order_descending(true),
        )
        .collect()?;

    let types = out.column("type")?.str()?;
    let subjects = out.column("subject_key")?.str()?;
    let resolved = out.column("resolved")?.bool()?;
    let mentions = out.column("mentions")?.cast(&DataType::Int64)?;
    let mentions = mentions.i64()?;
    let distinct = out.column("distinct_tickets")?.cast(&DataType::Int64)?;
    let distinct = distinct.i64()?;
    let negative = out.column("negative_share")?.f64()?;
    let incidental = out.column("incidental_share")?.f64()?;
    let mut rows = Vec::with_capacity(out.height());
    for i in 0..out.height() {
        rows.push(MentionSubjectStats {
            mention_type: types.get(i).unwrap_or("").to_string(),
            subject: subjects.get(i).unwrap_or("").to_string(),
            resolved: resolved.get(i).unwrap_or(false),
            mentions: mentions.get(i).unwrap_or(0),
            distinct_tickets: distinct.get(i).unwrap_or(0),
            negative_share: negative.get(i).unwrap_or(0.0),
            incidental_share: incidental.get(i).unwrap_or(0.0),
        });
    }
    Ok(rows)
}

/// `GET /api/sources/{source_id}/tables/{table}/mentions/summary`
pub async fn mention_summary(
    State(state): State<AppState>,
    Path((source_id, table)): Path<(String, String)>,
) -> AppResult<Json<MentionSummaryResponse>> {
    let store = state.require_store()?;
    let child = mentions_table_name(&table);
    if store.db().get_table(&source_id, &child).await?.is_none() {
        return Ok(Json(MentionSummaryResponse {
            mentions_table: child,
            total_mentions: 0,
            subjects: Vec::new(),
        }));
    }
    let df = store.read_table(&source_id, &child).await?;
    let subjects = summarize_mentions(&df).map_err(AppError::Polars)?;
    Ok(Json(MentionSummaryResponse {
        mentions_table: child,
        total_mentions: df.height(),
        subjects,
    }))
}

#[derive(Debug, Deserialize)]
pub struct UnresolvedQuery {
    /// open (default) | mapped | ignored | all
    pub status: Option<String>,
}

/// `GET /api/sources/{source_id}/tables/{table}/unresolved-subjects`
pub async fn list_unresolved(
    State(state): State<AppState>,
    Path((source_id, table)): Path<(String, String)>,
    Query(q): Query<UnresolvedQuery>,
) -> AppResult<Json<Vec<UnresolvedSubjectResponse>>> {
    let store = state.require_store()?;
    let table_row = store
        .db()
        .get_table(&source_id, &table)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("table '{table}' not found")))?;
    let status = match q.status.as_deref() {
        None => Some("open"),
        Some("all") => None,
        Some(s @ ("open" | "mapped" | "ignored")) => Some(s),
        Some(other) => {
            return Err(AppError::BadRequest(format!(
                "unknown status '{other}' (open | mapped | ignored | all)"
            )))
        },
    };
    let rows = store
        .db()
        .list_unresolved_subjects(&table_row.id, status)
        .await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

/// `POST /api/sources/{source_id}/tables/{table}/unresolved-subjects/{id}`
///
/// `mapped` appends the surface as an alias on `mapped_to` (through the
/// action bus, so it is logged and undoable); the next materialisation of
/// the extractor resolves it. `ignored` hides it from the queue; `open`
/// reopens it.
pub async fn update_unresolved(
    State(state): State<AppState>,
    auth_session: crate::auth::AuthSession,
    Path((source_id, table, id)): Path<(String, String, i64)>,
    Json(req): Json<UpdateUnresolvedRequest>,
) -> AppResult<Json<UnresolvedSubjectResponse>> {
    let store = state.require_store()?;
    let table_row = store
        .db()
        .get_table(&source_id, &table)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("table '{table}' not found")))?;
    let row = store
        .db()
        .get_unresolved_subject(id)
        .await?
        .filter(|r| r.table_id == table_row.id)
        .ok_or_else(|| AppError::NotFound(format!("unresolved subject {id} not found")))?;
    let mapped_to = match req.status.as_str() {
        "mapped" => {
            let target = req.mapped_to.ok_or_else(|| {
                AppError::BadRequest("mapped_to is required to map a subject".to_string())
            })?;
            let entry = store
                .db()
                .get_taxonomy_category(target)
                .await?
                .filter(|e| e.table_id == table_row.id)
                .ok_or_else(|| {
                    AppError::NotFound(format!("vocabulary entry {target} not found"))
                })?;
            let kind = VocabKind::parse(&entry.kind);
            if !matches!(kind, Some(VocabKind::Product | VocabKind::Competitor)) {
                return Err(AppError::BadRequest(format!(
                    "'{}' is a {} — subjects map to products and competitors only",
                    entry.name, entry.kind
                )));
            }
            let response = dispatch_action(
                &state,
                Action::DefineTaxonomyCategory {
                    scope: Scope {
                        source_id: source_id.clone(),
                        table: table.clone(),
                    },
                    name: entry.name.clone(),
                    description: None,
                    vocab_kind: Some(entry.kind.clone()),
                    parent_id: Some(entry.parent_id),
                    aliases: Some(vec![row.surface.clone()]),
                },
                &format!("map-subject-{id}-{}", uuid::Uuid::now_v7()),
                Actor::Human {
                    user_id: user_id(&auth_session)?,
                },
            )
            .await?;
            if !matches!(response.status, ActionStatus::Applied) {
                return Err(AppError::BadRequest(format!(
                    "alias not recorded: {}",
                    response
                        .result
                        .get("error")
                        .and_then(|e| e.as_str())
                        .unwrap_or("action failed")
                )));
            }
            Some(target)
        },
        "ignored" | "open" => None,
        other => {
            return Err(AppError::BadRequest(format!(
                "unknown status '{other}' (open | mapped | ignored)"
            )))
        },
    };
    let updated = store
        .db()
        .set_unresolved_subject_status(id, &req.status, mapped_to)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("unresolved subject {id} not found")))?;
    Ok(Json(updated.into()))
}

/// One parsed import line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportLine {
    pub kind: String,
    pub parent: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub aliases: Vec<String>,
}

/// Parse the import CSV: header `kind,parent,name,description,aliases`,
/// aliases separated by `|`. Parents must be defined on an earlier line (or
/// already exist), so the file is applied in order.
pub fn parse_import_csv(text: &str) -> Result<Vec<ImportLine>, String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .trim(csv::Trim::All)
        .flexible(true)
        .from_reader(text.as_bytes());
    let headers = reader
        .headers()
        .map_err(|e| format!("header: {e}"))?
        .clone();
    let idx = |name: &str| headers.iter().position(|h| h.eq_ignore_ascii_case(name));
    let (Some(k), Some(n)) = (idx("kind"), idx("name")) else {
        return Err("header must include 'kind' and 'name'".to_string());
    };
    let p = idx("parent");
    let d = idx("description");
    let a = idx("aliases");
    let mut lines = Vec::new();
    for (i, record) in reader.records().enumerate() {
        let record = record.map_err(|e| format!("line {}: {e}", i + 2))?;
        let get = |j: Option<usize>| {
            j.and_then(|j| record.get(j))
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        let kind = get(Some(k)).ok_or_else(|| format!("line {}: kind is empty", i + 2))?;
        match VocabKind::parse(&kind) {
            Some(VocabKind::Product | VocabKind::Competitor) => {},
            _ => {
                return Err(format!(
                    "line {}: kind '{kind}' is not importable (product | competitor)",
                    i + 2
                ))
            },
        }
        let name = get(Some(n)).ok_or_else(|| format!("line {}: name is empty", i + 2))?;
        lines.push(ImportLine {
            kind,
            parent: get(p),
            name,
            description: get(d),
            aliases: get(a)
                .map(|s| {
                    s.split('|')
                        .map(str::trim)
                        .filter(|x| !x.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        });
    }
    Ok(lines)
}

/// `POST /api/sources/{source_id}/tables/{table}/vocabulary/import`
///
/// Body: the CSV text. Each line becomes one `define_taxonomy_category`
/// action through the bus, in file order, so the whole import is logged
/// under the importing user and undoable line by line. Stops at the first
/// failing line and reports how far it got.
pub async fn import_vocabulary(
    State(state): State<AppState>,
    auth_session: crate::auth::AuthSession,
    Path((source_id, table)): Path<(String, String)>,
    body: String,
) -> AppResult<Json<ImportVocabularyResponse>> {
    let user = user_id(&auth_session)?;
    let store = state.require_store()?;
    let table_row = store
        .db()
        .get_table(&source_id, &table)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("table '{table}' not found")))?;
    let lines = parse_import_csv(&body).map_err(AppError::BadRequest)?;
    let batch = uuid::Uuid::now_v7();
    let mut applied = 0usize;
    for (i, line) in lines.iter().enumerate() {
        let parent_id = match &line.parent {
            None => 0,
            Some(parent_name) => store
                .db()
                .get_taxonomy_category_by_name(&table_row.id, &line.kind, 0, parent_name)
                .await?
                .map(|p| p.id)
                .ok_or_else(|| {
                    AppError::BadRequest(format!(
                        "line {}: parent '{parent_name}' is not a root {} (define it on an \
                         earlier line)",
                        i + 2,
                        line.kind
                    ))
                })?,
        };
        let response = dispatch_action(
            &state,
            Action::DefineTaxonomyCategory {
                scope: Scope {
                    source_id: source_id.clone(),
                    table: table.clone(),
                },
                name: line.name.clone(),
                description: line.description.clone(),
                vocab_kind: Some(line.kind.clone()),
                parent_id: Some(parent_id),
                aliases: Some(line.aliases.clone()),
            },
            &format!("import-{batch}-{i}"),
            Actor::Human {
                user_id: user.clone(),
            },
        )
        .await?;
        if !matches!(response.status, ActionStatus::Applied) {
            return Ok(Json(ImportVocabularyResponse {
                applied,
                total: lines.len(),
                failed_line: Some(i + 2),
                error: response
                    .result
                    .get("error")
                    .and_then(|e| e.as_str())
                    .map(str::to_string),
            }));
        }
        applied += 1;
    }
    Ok(Json(ImportVocabularyResponse {
        applied,
        total: lines.len(),
        failed_line: None,
        error: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_counts_distinct_tickets_and_keeps_unresolved_surfaces() {
        let df = df!(
            "ticket_id" => &[1_i64, 1, 2, 3, 3],
            "type" => &["product", "product", "product", "competitor", "feedback"],
            "subject_id" => &[Some(11_i64), Some(11), Some(11), None, None],
            "subject" => &[Some("Invoice screen"), Some("Invoice screen"), Some("Invoice screen"), None, None],
            "subject_surface" => &[None, None, None, Some("Unknown Corp"), None],
            "polarity" => &["negative", "negative", "positive", "positive", "negative"],
            "incidental" => &[true, true, false, true, true],
        )
        .unwrap();
        let rows = summarize_mentions(&df).unwrap();
        let invoice = rows.iter().find(|r| r.subject == "Invoice screen").unwrap();
        assert_eq!(invoice.mentions, 3);
        assert_eq!(invoice.distinct_tickets, 2);
        assert!((invoice.negative_share - 2.0 / 3.0).abs() < 1e-9);
        assert!((invoice.incidental_share - 2.0 / 3.0).abs() < 1e-9);
        assert!(invoice.resolved);
        let corp = rows.iter().find(|r| r.subject == "Unknown Corp").unwrap();
        assert!(!corp.resolved);
        assert_eq!(corp.mention_type, "competitor");
        // A feedback row with no subject is not a subject.
        assert_eq!(rows.len(), 2);
        assert!(summarize_mentions(&df.head(Some(0))).unwrap().is_empty());
    }

    #[test]
    fn import_csv_parses_hierarchy_and_aliases_and_rejects_induced_kinds() {
        let text = "kind,parent,name,description,aliases\n\
                    product,,Billing,charges and invoices,billing|payments\n\
                    product,Billing,Invoice screen,the invoices page,invoices page\n\
                    competitor,,CompetitorCo,,\n";
        let lines = parse_import_csv(text).unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].aliases, vec!["billing", "payments"]);
        assert_eq!(lines[1].parent.as_deref(), Some("Billing"));
        assert_eq!(lines[2].description, None);
        assert!(lines[2].aliases.is_empty());
        let bad = "kind,name\ncategory,Billing\n";
        assert!(parse_import_csv(bad)
            .unwrap_err()
            .contains("not importable"));
        assert!(parse_import_csv("name\nx\n").is_err());
    }
}
