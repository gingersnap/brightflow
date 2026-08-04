//! Topic-cluster curation: rename/merge/noise/label edits, term exclusion,
//! and refits — plus the label-centroid artifact refresh their undos share.

use serde_json::json;

use brightflow_engine::enrichment::{
    centroid_fingerprint, ClusteringArtifact, LabelCentroidsArtifact, ARTIFACT_VERSION,
};

use super::table_ctx;
use crate::actions::types::UndoOp;
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

pub(crate) async fn execute_rename_cluster(
    state: &AppState,
    source_id: &str,
    table: &str,
    cluster_id: i64,
    name: &str,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    edit_cluster(
        state,
        source_id,
        table,
        cluster_id,
        Some(Some(name)),
        None,
        None,
        None,
        false,
    )
    .await
}

pub(crate) async fn execute_merge_clusters(
    state: &AppState,
    source_id: &str,
    table: &str,
    from_cluster_id: i64,
    into_cluster_id: i64,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    if from_cluster_id == into_cluster_id {
        return Err(AppError::BadRequest(
            "cannot merge a cluster into itself".to_string(),
        ));
    }
    edit_cluster(
        state,
        source_id,
        table,
        from_cluster_id,
        None,
        None,
        None,
        Some(Some(into_cluster_id)),
        false,
    )
    .await
}

pub(crate) async fn execute_mark_cluster_noise(
    state: &AppState,
    source_id: &str,
    table: &str,
    cluster_id: i64,
    is_noise: bool,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    edit_cluster(
        state,
        source_id,
        table,
        cluster_id,
        None,
        None,
        Some(is_noise),
        None,
        false,
    )
    .await
}

pub(crate) async fn execute_assign_cluster_label(
    state: &AppState,
    source_id: &str,
    table: &str,
    cluster_id: i64,
    label: &str,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    edit_cluster(
        state,
        source_id,
        table,
        cluster_id,
        None,
        Some(Some(label)),
        None,
        None,
        true,
    )
    .await
}

pub(crate) async fn execute_exclude_term(
    state: &AppState,
    source_id: &str,
    table: &str,
    term: &str,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let term = term.trim().to_lowercase();
    if term.is_empty() {
        return Err(AppError::BadRequest("term must not be empty".to_string()));
    }
    store
        .db()
        .add_excluded_term(&table_id, &term, chrono::Utc::now().timestamp())
        .await?;
    Ok((
        json!({ "excluded": term }),
        Some(UndoOp::RemoveExcludedTerm {
            table_id,
            term: term.clone(),
        }),
    ))
}

pub(crate) async fn execute_split_cluster(
    state: &AppState,
    source_id: &str,
    table: &str,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    // v1 semantics: refit with one more cluster slot. Not undoable.
    let overview = crate::topics::handlers::run_recluster(state, source_id, table, None).await?;
    Ok((
        json!({ "refit": true, "k": overview.k, "note": "split refits with k+1" }),
        None,
    ))
}

/// Borrowed recluster parameters — more fields than the argument-count
/// threshold allows as bare arguments.
pub(crate) struct ReclusterArgs<'a> {
    pub k: Option<u32>,
    pub language: Option<&'a str>,
    pub embedder: Option<&'a str>,
    pub min_cluster_size: Option<u32>,
    pub algorithm: Option<&'a str>,
}

pub(crate) async fn execute_recluster(
    state: &AppState,
    source_id: &str,
    table: &str,
    args: &ReclusterArgs<'_>,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let req = crate::topics::types::ReclusterRequest {
        k: args.k.map(|v| v as usize),
        language: args.language.map(ToString::to_string),
        embedder: args.embedder.map(ToString::to_string),
        min_cluster_size: args.min_cluster_size.map(|v| v as usize),
        algorithm: args.algorithm.map(ToString::to_string),
    };
    let overview =
        crate::topics::handlers::run_recluster(state, source_id, table, Some(req)).await?;
    Ok((json!({ "refit": true, "k": overview.k }), None))
}

/// Borrowed `UndoOp::RestoreClusterEdit` fields — the widest undo variant.
pub(crate) struct RestoreClusterEditArgs<'a> {
    pub table_id: &'a str,
    pub centroid_fingerprint: &'a str,
    pub centroid_json: &'a str,
    pub cluster_id: Option<i64>,
    pub custom_name: &'a Option<String>,
    pub label: &'a Option<String>,
    pub is_noise: bool,
    pub merged_into: Option<i64>,
    pub delete_row: bool,
    pub refresh_labels: bool,
}

pub(crate) async fn undo_restore_cluster_edit(
    state: &AppState,
    args: &RestoreClusterEditArgs<'_>,
) -> AppResult<()> {
    let store = state.require_store()?;
    let store = std::sync::Arc::clone(store);
    if args.delete_row {
        let edits = store.db().get_cluster_edits(args.table_id).await?;
        if let Some(row) = edits
            .iter()
            .find(|e| e.centroid_fingerprint == args.centroid_fingerprint)
        {
            store.db().delete_cluster_edit(row.id).await?;
        }
    } else {
        store
            .db()
            .upsert_cluster_edit(
                args.table_id,
                args.centroid_fingerprint,
                args.centroid_json,
                args.cluster_id,
                Some(args.custom_name.as_deref()),
                Some(args.label.as_deref()),
                Some(args.is_noise),
                Some(args.merged_into),
                chrono::Utc::now().timestamp(),
            )
            .await?;
    }
    if args.refresh_labels {
        // Best-effort: label artifact refresh needs source/table which we
        // can recover from the table id via the tables catalog.
        if let Err(e) = refresh_label_artifact_by_table_id(state, args.table_id).await {
            tracing::warn!("label artifact refresh after undo failed: {e}");
        }
    }
    Ok(())
}

pub(crate) async fn undo_remove_excluded_term(
    state: &AppState,
    table_id: &str,
    term: &str,
) -> AppResult<()> {
    let store = state.require_store()?;
    store.db().remove_excluded_term(table_id, term).await?;
    Ok(())
}

/// Core cluster-edit executor: attaches the edit to the cluster's centroid
/// snapshot and captures the previous row for undo.
///
/// `Option<Option<T>>` fields are deliberate tri-states: outer None = leave
/// unchanged, `Some(None)` = clear, `Some(Some(v))` = set.
#[allow(clippy::too_many_arguments, clippy::option_option)]
async fn edit_cluster(
    state: &AppState,
    source_id: &str,
    table: &str,
    cluster_id: i64,
    custom_name: Option<Option<&str>>,
    label: Option<Option<&str>>,
    is_noise: Option<bool>,
    merged_into: Option<Option<i64>>,
    refresh_labels: bool,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let clustering = load_clustering(state, source_id, table)?;
    let idx = usize::try_from(cluster_id)
        .ok()
        .filter(|i| *i < clustering.centroids.len())
        .ok_or_else(|| AppError::BadRequest(format!("cluster {cluster_id} out of range")))?;
    let centroid = &clustering.centroids[idx];
    let fp = centroid_fingerprint(centroid);
    let centroid_json = serde_json::to_string(centroid)
        .map_err(|e| AppError::Internal(format!("centroid serialize: {e}")))?;

    // Capture the previous row for undo
    let previous = store
        .db()
        .get_cluster_edits(&table_id)
        .await?
        .into_iter()
        .find(|e| e.centroid_fingerprint == fp);
    let undo = match &previous {
        Some(p) => UndoOp::RestoreClusterEdit {
            table_id: table_id.clone(),
            centroid_fingerprint: fp.clone(),
            centroid_json: p.centroid_json.clone(),
            cluster_id: p.cluster_id,
            custom_name: p.custom_name.clone(),
            label: p.label.clone(),
            is_noise: p.is_noise,
            merged_into: p.merged_into,
            delete_row: false,
            refresh_labels,
        },
        None => UndoOp::RestoreClusterEdit {
            table_id: table_id.clone(),
            centroid_fingerprint: fp.clone(),
            centroid_json: centroid_json.clone(),
            cluster_id: Some(cluster_id),
            custom_name: None,
            label: None,
            is_noise: false,
            merged_into: None,
            delete_row: true,
            refresh_labels,
        },
    };

    let row = store
        .db()
        .upsert_cluster_edit(
            &table_id,
            &fp,
            &centroid_json,
            Some(cluster_id),
            custom_name,
            label,
            is_noise,
            merged_into,
            chrono::Utc::now().timestamp(),
        )
        .await?;

    if refresh_labels {
        refresh_label_artifact(state, source_id, table, &table_id).await?;
    }

    Ok((
        json!({
            "editId": row.id,
            "clusterId": cluster_id,
            "customName": row.custom_name,
            "label": row.label,
            "isNoise": row.is_noise,
            "mergedInto": row.merged_into,
        }),
        Some(undo),
    ))
}

fn load_clustering(
    state: &AppState,
    source_id: &str,
    table: &str,
) -> AppResult<ClusteringArtifact> {
    let root = state
        .paths
        .as_ref()
        .map(brightflow_core::WorkspacePaths::root)
        .ok_or_else(|| AppError::Internal("workspace paths unavailable".to_string()))?;
    let dir = brightflow_engine::embedding::topics_artifact_dir(&root, source_id, table);
    ClusteringArtifact::load(&dir)
        .ok()
        .filter(|c| c.artifact_version == ARTIFACT_VERSION)
        .ok_or_else(|| {
            AppError::BadRequest("no current cluster fit — run recluster first".to_string())
        })
}

/// Rebuild `labels.bin` from SQLite (source of truth): every cluster edit
/// with a label contributes its centroid; duplicate labels average.
async fn refresh_label_artifact(
    state: &AppState,
    source_id: &str,
    table: &str,
    table_id: &str,
) -> AppResult<()> {
    let store = state.require_store()?;
    let clustering = load_clustering(state, source_id, table)?;
    let edits = store.db().get_cluster_edits(table_id).await?;

    let entries: Vec<(&str, &[f32])> = edits
        .iter()
        .filter_map(|edit| {
            let (Some(label), Some(cid), false) = (&edit.label, edit.cluster_id, edit.orphaned)
            else {
                return None;
            };
            if label.is_empty() {
                return None;
            }
            let centroid = usize::try_from(cid)
                .ok()
                .and_then(|i| clustering.centroids.get(i))?;
            Some((label.as_str(), centroid.as_slice()))
        })
        .collect();
    let centroids = average_label_centroids(entries);

    let root = state
        .paths
        .as_ref()
        .map(brightflow_core::WorkspacePaths::root)
        .ok_or_else(|| AppError::Internal("workspace paths unavailable".to_string()))?;
    let dir = brightflow_engine::embedding::topics_artifact_dir(&root, source_id, table);
    let artifact = LabelCentroidsArtifact {
        centroids,
        embedding_model_id: clustering.embedding_model_id.clone(),
    };
    artifact
        .save(&dir)
        .map_err(|e| AppError::Internal(format!("label artifact save: {e}")))?;
    Ok(())
}

/// One L2-normalized centroid per label. Duplicate labels (several clusters
/// assigned the same label) average their centroids before normalizing, so a
/// label's centroid stays comparable to row embeddings via cosine.
fn average_label_centroids<'a>(
    entries: impl IntoIterator<Item = (&'a str, &'a [f32])>,
) -> std::collections::HashMap<String, Vec<f32>> {
    let mut accum: std::collections::HashMap<String, (Vec<f32>, u32)> =
        std::collections::HashMap::new();
    for (label, centroid) in entries {
        let entry = accum
            .entry(label.to_string())
            .or_insert_with(|| (vec![0.0; centroid.len()], 0));
        for (a, v) in entry.0.iter_mut().zip(centroid.iter()) {
            *a += v;
        }
        entry.1 += 1;
    }
    accum
        .into_iter()
        .map(|(label, (mut sum, count))| {
            let mut norm = 0.0f32;
            #[allow(clippy::cast_precision_loss)]
            let count_f = count as f32;
            for v in &mut sum {
                *v /= count_f;
                norm = v.mul_add(*v, norm);
            }
            let norm = norm.sqrt().max(1e-12);
            for v in &mut sum {
                *v /= norm;
            }
            (label, sum)
        })
        .collect()
}

/// Undo path helper: recover (source_id, table) from a table id.
async fn refresh_label_artifact_by_table_id(state: &AppState, table_id: &str) -> AppResult<()> {
    let store = state.require_store()?;
    let tables = store
        .list_tables()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    for t in tables {
        if let Ok(Some(row)) = store.db().get_table(&t.source_id, &t.name).await {
            if row.id == table_id {
                return refresh_label_artifact(state, &t.source_id, &t.name, table_id).await;
            }
        }
    }
    Err(AppError::NotFound(format!("table id {table_id} not found")))
}

#[cfg(test)]
mod tests {
    use super::average_label_centroids;

    #[test]
    fn duplicate_labels_average_then_normalize() {
        let a = [1.0_f32, 0.0];
        let b = [0.0_f32, 1.0];
        let out = average_label_centroids([("payments", &a[..]), ("payments", &b[..])]);
        let c = &out["payments"];
        // avg = [0.5, 0.5] → normalized = [1/√2, 1/√2]
        let expected = 1.0 / 2.0_f32.sqrt();
        assert!((c[0] - expected).abs() < 1e-6);
        assert!((c[1] - expected).abs() < 1e-6);
    }

    #[test]
    fn single_label_is_normalized() {
        let long = [3.0_f32, 4.0];
        let out = average_label_centroids([("bugs", &long[..])]);
        let c = &out["bugs"];
        assert!((c[0] - 0.6).abs() < 1e-6);
        assert!((c[1] - 0.8).abs() < 1e-6);
        let norm: f32 = c.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    #[test]
    fn empty_input_yields_empty_map() {
        assert!(average_label_centroids(std::iter::empty::<(&str, &[f32])>()).is_empty());
    }
}
