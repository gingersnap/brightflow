//! Vocabulary snapshots for `ticket_classify` functions.
//!
//! The spec snapshots the vocabulary as (id, parent, description); names are
//! looked up live. This module is the seam between the vocabulary table and
//! the function versions: it loads a snapshot, injects it into a spec the
//! client sent (clients never author the snapshot), builds the runnable
//! [`RunSpec`] with current names, and — after any vocabulary edit — writes a
//! new version for every classify function whose snapshot no longer matches.
//! A rename never triggers a new version: it is not in the snapshot.

use std::sync::Arc;

use brightflow_engine::enrichment::mentions::SubjectResolver;
use brightflow_engine::enrichment::ticket_classify::VocabNames;
use brightflow_engine::enrichment::{FunctionSpec, VocabEntry, VocabKind};
use brightflow_store::ParquetStore;

use super::runner::RunSpec;
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

/// The classification vocabulary of one table: root categories, their
/// subcategories, and the current names of both.
pub struct VocabSnapshot {
    pub categories: Vec<VocabEntry>,
    pub subcategories: Vec<VocabEntry>,
    pub names: VocabNames,
}

pub async fn load(store: &ParquetStore, table_id: &str) -> AppResult<VocabSnapshot> {
    let rows = store.db().get_taxonomy_categories(table_id).await?;
    let mut categories = Vec::new();
    let mut subcategories = Vec::new();
    let mut names = VocabNames::new();
    for row in rows {
        let entry = VocabEntry {
            id: row.id,
            parent_id: row.parent_id,
            description: row.description.clone(),
        };
        match VocabKind::parse(&row.kind) {
            Some(VocabKind::Category) => categories.push(entry),
            Some(VocabKind::Subcategory) => subcategories.push(entry),
            _ => continue,
        }
        names.insert(row.id, row.name);
    }
    // A subcategory whose parent was deleted (RESTRICT should prevent it,
    // but the snapshot must not depend on that) is dropped rather than
    // offered under nothing.
    subcategories.retain(|s| categories.iter().any(|c| c.id == s.parent_id));
    Ok(VocabSnapshot {
        categories,
        subcategories,
        names,
    })
}

/// The extraction vocabulary of one table: products (two levels),
/// competitors, feedback categories, current names, and the name/alias
/// resolver for the imported kinds.
pub struct ExtractSnapshot {
    pub products: Vec<VocabEntry>,
    pub competitors: Vec<VocabEntry>,
    pub feedback_categories: Vec<VocabEntry>,
    pub names: VocabNames,
    pub resolver: SubjectResolver,
}

pub async fn load_extract(store: &ParquetStore, table_id: &str) -> AppResult<ExtractSnapshot> {
    let rows = store.db().get_taxonomy_categories(table_id).await?;
    let mut snap = ExtractSnapshot {
        products: Vec::new(),
        competitors: Vec::new(),
        feedback_categories: Vec::new(),
        names: VocabNames::new(),
        resolver: SubjectResolver::new(),
    };
    for row in rows {
        let entry = VocabEntry {
            id: row.id,
            parent_id: row.parent_id,
            description: row.description.clone(),
        };
        let imported = match VocabKind::parse(&row.kind) {
            Some(VocabKind::Product) => {
                snap.products.push(entry);
                true
            },
            Some(VocabKind::Competitor) => {
                snap.competitors.push(entry);
                true
            },
            Some(VocabKind::FeedbackCategory) => {
                snap.feedback_categories.push(entry);
                false
            },
            _ => continue,
        };
        if imported {
            snap.resolver.add(&row.name, row.id);
            let aliases: Vec<String> = row
                .aliases_json
                .as_deref()
                .and_then(|j| serde_json::from_str(j).ok())
                .unwrap_or_default();
            for alias in aliases {
                snap.resolver.add(&alias, row.id);
            }
        }
        snap.names.insert(row.id, row.name);
    }
    Ok(snap)
}

/// Overwrite the snapshot fields of a built-in spec with the table's current
/// vocabulary. Other kinds pass through untouched.
pub async fn inject(
    store: &ParquetStore,
    table_id: &str,
    spec: &mut FunctionSpec,
) -> AppResult<()> {
    match spec {
        FunctionSpec::TicketClassify(tc) => {
            let snap = load(store, table_id).await?;
            tc.categories = snap.categories;
            tc.subcategories = snap.subcategories;
        },
        FunctionSpec::TicketExtract(te) => {
            let snap = load_extract(store, table_id).await?;
            te.products = snap.products;
            te.competitors = snap.competitors;
            te.feedback_categories = snap.feedback_categories;
        },
        FunctionSpec::LlmPrompt(_) | FunctionSpec::TopicModel(_) | FunctionSpec::Classifier(_) => {
        },
    }
    Ok(())
}

/// Build the runnable form of a stored spec. Names are loaded here, at run
/// start, so a run sees one consistent naming.
pub async fn run_spec_for(
    store: &ParquetStore,
    table_id: &str,
    spec: FunctionSpec,
) -> AppResult<RunSpec> {
    match spec {
        FunctionSpec::LlmPrompt(llm) => Ok(RunSpec::LlmPrompt(llm)),
        FunctionSpec::TicketClassify(tc) => {
            let snap = load(store, table_id).await?;
            Ok(RunSpec::TicketClassify {
                spec: tc,
                names: Arc::new(snap.names),
            })
        },
        FunctionSpec::TicketExtract(te) => {
            let snap = load_extract(store, table_id).await?;
            Ok(RunSpec::TicketExtract {
                spec: te,
                names: Arc::new(snap.names),
                resolver: Arc::new(snap.resolver),
            })
        },
        FunctionSpec::TopicModel(_) | FunctionSpec::Classifier(_) => Err(AppError::BadRequest(
            "runs are only available for llm_prompt, ticket_classify and ticket_extract \
             functions (topics run via recluster)"
                .to_string(),
        )),
    }
}

/// After a vocabulary edit: re-snapshot every built-in function on the table.
///
/// A function gets a new version only when its snapshot no longer matches.
/// Returns the ids of the functions bumped. Never fails the edit that called
/// it — a snapshot that lags is caught on the next run, a lost edit is not.
pub async fn refresh_snapshots(state: &AppState, table_id: &str) -> Vec<String> {
    let Some(store) = state.store() else {
        return Vec::new();
    };
    let mut bumped = Vec::new();
    let result: AppResult<()> = async {
        let snap = load(store, table_id).await?;
        let extract = load_extract(store, table_id).await?;
        let functions = store.db().list_enrichment_functions(table_id).await?;
        for function in functions
            .iter()
            .filter(|f| f.kind == "ticket_classify" || f.kind == "ticket_extract")
        {
            let Some(version) = store
                .db()
                .get_enrichment_function_version(&function.id, function.current_version)
                .await?
            else {
                continue;
            };
            let refreshed = match serde_json::from_str::<FunctionSpec>(&version.config_json) {
                Ok(FunctionSpec::TicketClassify(mut tc)) => {
                    if tc.vocabulary_matches(&snap.categories, &snap.subcategories) {
                        continue;
                    }
                    tc.categories.clone_from(&snap.categories);
                    tc.subcategories.clone_from(&snap.subcategories);
                    FunctionSpec::TicketClassify(tc)
                },
                Ok(FunctionSpec::TicketExtract(mut te)) => {
                    if te.vocabulary_matches(
                        &extract.products,
                        &extract.competitors,
                        &extract.feedback_categories,
                    ) {
                        continue;
                    }
                    te.products.clone_from(&extract.products);
                    te.competitors.clone_from(&extract.competitors);
                    te.feedback_categories
                        .clone_from(&extract.feedback_categories);
                    FunctionSpec::TicketExtract(te)
                },
                _ => continue,
            };
            let config_json = serde_json::to_string(&refreshed).map_err(AppError::Json)?;
            store
                .db()
                .update_enrichment_function_config(&function.id, &config_json)
                .await?;
            bumped.push(function.id.clone());
        }
        Ok(())
    }
    .await;
    if let Err(e) = result {
        tracing::warn!("vocabulary snapshot refresh for table {table_id} failed: {e}");
    }
    bumped
}
