//! End-to-end: insights runs against a real temp-dir store.
//!
//! Covers the Phase-4 contract: a post-sync auto-run records an
//! `insight_runs` row but never writes `insight_history` ("shown" means a
//! human saw it); a manual run records both.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::Json;
use polars::prelude::*;

use brightflow_api::insights::handlers::run_trends;
use brightflow_api::insights::types::TrendsRequest;
use brightflow_api::state::AppState;
use brightflow_store::ParquetStore;

const SOURCE: &str = "test-source";
const TABLE: &str = "sales";

/// 6 weeks of daily data × 2 regions: EU dominates and spikes in the final
/// week — plenty for trends, dominance, and concentration detectors.
fn sales_dataframe() -> DataFrame {
    let mut dates: Vec<String> = Vec::new();
    let mut regions: Vec<&str> = Vec::new();
    let mut revenue: Vec<f64> = Vec::new();
    let start = chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
    for day in 0..42i64 {
        let date = (start + chrono::Duration::days(day))
            .format("%Y-%m-%d")
            .to_string();
        for region in ["EU", "NA"] {
            for k in 0..3 {
                dates.push(date.clone());
                regions.push(region);
                let base = if region == "EU" { 500.0 } else { 50.0 };
                let spike = if day >= 35 && region == "EU" {
                    400.0
                } else {
                    0.0
                };
                revenue.push(base + f64::from(k) + spike);
            }
        }
    }
    df!(
        "date" => dates,
        "region" => regions,
        "revenue" => revenue,
    )
    .unwrap()
}

async fn state_with_planted_table(dir: &std::path::Path) -> AppState {
    let db_path = dir.join("meta.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
    let store = ParquetStore::new(dir.join("store"), &db_url).await.unwrap();

    // Write the fixture parquet and ingest it.
    let parquet_path = dir.join("sales.parquet");
    let mut df = sales_dataframe();
    let file = std::fs::File::create(&parquet_path).unwrap();
    ParquetWriter::new(file).finish(&mut df).unwrap();
    store
        .ingest_parquet(SOURCE, TABLE, &parquet_path, None)
        .await
        .unwrap();

    AppState::with_store(store).await
}

/// Poll until the store shows `want` insight-run rows (writes are
/// fire-and-forget spawns).
async fn wait_for_runs(state: &AppState, want: usize) -> Vec<brightflow_store::InsightRunRow> {
    let store = state.store().unwrap();
    let table = store
        .db()
        .get_table(SOURCE, TABLE)
        .await
        .unwrap()
        .expect("table exists");
    for _ in 0..100 {
        let rows = store.db().list_insight_runs(&table.id, 50).await.unwrap();
        if rows.len() >= want {
            return rows;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("insight_runs never reached {want} rows");
}

#[tokio::test(flavor = "multi_thread")]
async fn post_sync_records_run_but_not_history() {
    let dir = tempfile::tempdir().unwrap();
    let state = state_with_planted_table(dir.path()).await;
    let store = Arc::clone(state.store().unwrap());
    let table = store
        .db()
        .get_table(SOURCE, TABLE)
        .await
        .unwrap()
        .expect("table exists");

    // ── Post-sync auto-run ────────────────────────────────────────────────
    brightflow_api::insights::auto::post_sync(state.clone(), SOURCE.to_string(), TABLE.to_string())
        .await;
    let runs = wait_for_runs(&state, 1).await;
    assert_eq!(runs[0].triggered_by, "post_sync");
    assert_eq!(runs[0].report_type, "trends");
    assert!(runs[0].finding_count > 0, "planted spike must be found");

    // Auto-runs must NOT decay novelty: no history rows yet.
    let history = store.db().get_insight_history(&table.id).await.unwrap();
    assert!(
        history.is_empty(),
        "post-sync run wrote insight_history: {history:?}"
    );

    // ── Manual run ────────────────────────────────────────────────────────
    let response = run_trends(
        State(state.clone()),
        Json(TrendsRequest {
            source_id: SOURCE.to_string(),
            dataset_id: TABLE.to_string(),
            config: brightflow_api::insights::types::EngineConfig::default(),
        }),
    )
    .await
    .expect("manual trends run succeeds");
    assert!(response.0.finding_count > 0);

    let all_runs = wait_for_runs(&state, 2).await;
    assert!(all_runs.iter().any(|r| r.triggered_by == "manual"));

    // Manual runs DO record shown insights (also a spawned write — poll).
    let mut recorded = false;
    for _ in 0..100 {
        if !store
            .db()
            .get_insight_history(&table.id)
            .await
            .unwrap()
            .is_empty()
        {
            recorded = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(recorded, "manual run must write insight_history");
}
