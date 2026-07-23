//! Integration tests for the Drivers report: planted-delta attribution,
//! sign-cancellation surfacing, and fingerprint hygiene across passes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stderr,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    clippy::panic
)]

use std::collections::HashSet;

use chrono::NaiveDate;
use polars::prelude::*;

use brightflow_engine::analysis::engine::AnalysisEngine;
use brightflow_engine::analysis::tree::{AnalysisTree, AnalysisType};
use brightflow_engine::data::schema::detect_schema;

/// 8 weekly periods × 3 regions; in the final week EU revenue halves while
/// the other regions stay flat — EU must surface as the top driver.
fn eu_drop_dataframe() -> DataFrame {
    let mut dates = Vec::new();
    let mut regions = Vec::new();
    let mut revenue = Vec::new();

    let start = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
    for week in 0..8 {
        for day in 0..7 {
            let date = start + chrono::Duration::days(week * 7 + day);
            for region in &["EU", "NA", "APAC"] {
                // A little deterministic spread so variances are non-zero.
                for k in 0..3 {
                    dates.push(
                        date.and_hms_opt(0, 0, 0)
                            .unwrap()
                            .and_utc()
                            .timestamp_millis(),
                    );
                    regions.push(*region);
                    let base = match *region {
                        "EU" => 100.0,
                        "NA" => 60.0,
                        _ => 40.0,
                    };
                    // Continuous jitter keeps revenue high-cardinality so
                    // detect_schema classifies it as a measure.
                    let jitter = f64::from(k) - 1.0 + day as f64 * 0.013 + week as f64 * 0.07;
                    let value = if week == 7 && *region == "EU" {
                        base / 2.0 + jitter
                    } else {
                        base + jitter
                    };
                    revenue.push(value);
                }
            }
        }
    }

    let date_series = Series::new("date".into(), &dates)
        .cast(&DataType::Datetime(TimeUnit::Milliseconds, None))
        .unwrap();
    df!(
        "date" => date_series,
        "region" => regions,
        "revenue" => revenue,
    )
    .unwrap()
}

/// Final week: EU halves while NA doubles by the same absolute amount —
/// the net total barely moves.
fn offsetting_dataframe() -> DataFrame {
    let mut dates = Vec::new();
    let mut regions = Vec::new();
    let mut revenue = Vec::new();

    let start = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
    for week in 0..8 {
        for day in 0..7 {
            let date = start + chrono::Duration::days(week * 7 + day);
            for region in &["EU", "NA", "APAC"] {
                for k in 0..3 {
                    dates.push(
                        date.and_hms_opt(0, 0, 0)
                            .unwrap()
                            .and_utc()
                            .timestamp_millis(),
                    );
                    regions.push(*region);
                    let jitter = f64::from(k) - 1.0 + day as f64 * 0.013 + week as f64 * 0.07;
                    let value = match (*region, week) {
                        ("EU", 7) => 40.0 + jitter,
                        ("NA", 7) => 100.0 + jitter,
                        ("EU", _) => 80.0 + jitter,
                        ("NA", _) => 60.0 + jitter,
                        _ => 50.0 + jitter,
                    };
                    revenue.push(value);
                }
            }
        }
    }

    let date_series = Series::new("date".into(), &dates)
        .cast(&DataType::Datetime(TimeUnit::Milliseconds, None))
        .unwrap();
    df!(
        "date" => date_series,
        "region" => regions,
        "revenue" => revenue,
    )
    .unwrap()
}

fn drivers_roots(tree: &AnalysisTree) -> Vec<&brightflow_engine::analysis::tree::AnalysisNode> {
    tree.roots
        .iter()
        .filter_map(|id| tree.nodes.get(id.0))
        .collect()
}

#[test]
fn eu_revenue_drop_surfaces_as_top_driver() {
    let df = eu_drop_dataframe();
    let schema = detect_schema(&df).unwrap();
    let engine = AnalysisEngine::new(2.0, 0.05, 3);
    let result = engine.run_drivers(&df, &schema).unwrap();
    let tree = &result.tree;

    // A PeriodComparison root about revenue must exist…
    let revenue_root = drivers_roots(tree)
        .into_iter()
        .find(|n| matches!(&n.analysis, AnalysisType::PeriodComparison { column, .. } if column == "revenue"))
        .expect("drivers report must produce a revenue PeriodComparison root");
    assert!(
        !revenue_root.fingerprint.is_empty(),
        "drivers roots carry fingerprints"
    );
    assert!(!revenue_root.why.is_empty(), "drivers roots carry why");

    // …whose top child attributes the drop to region=EU with >50% of the change.
    let children: Vec<_> = revenue_root
        .children
        .iter()
        .filter_map(|id| tree.nodes.get(id.0))
        .collect();
    assert!(!children.is_empty(), "root must have driver children");
    let top = children
        .iter()
        .max_by(|a, b| {
            a.significance
                .partial_cmp(&b.significance)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap();
    let AnalysisType::Segment {
        segment_column,
        segment_value,
        contribution_pct,
        ..
    } = &top.analysis
    else {
        panic!("driver children are Segment nodes, got {:?}", top.analysis);
    };
    assert_eq!(segment_column, "region");
    assert_eq!(segment_value, "EU");
    assert!(
        contribution_pct.abs() > 50.0,
        "EU should explain most of the drop, got {contribution_pct}"
    );
    assert!(
        !top.fingerprint.is_empty(),
        "driver children carry fingerprints"
    );
    assert_eq!(
        top.filter_chain.first().map(|f| f.value.as_str()),
        Some("EU"),
        "driver children carry filter chains"
    );
}

#[test]
fn offsetting_movements_still_surface() {
    let df = offsetting_dataframe();
    let schema = detect_schema(&df).unwrap();
    let engine = AnalysisEngine::new(2.0, 0.05, 3);
    let result = engine.run_drivers(&df, &schema).unwrap();
    let tree = &result.tree;

    let revenue_root = drivers_roots(tree)
        .into_iter()
        .find(|n| matches!(&n.analysis, AnalysisType::PeriodComparison { column, .. } if column == "revenue"))
        .expect("offsetting case must still produce a revenue root");
    assert!(
        revenue_root.why.contains("opposite directions"),
        "offsetting phrasing expected, got: {}",
        revenue_root.why
    );
    // Both sides of the cancellation appear as children.
    let child_values: Vec<String> = revenue_root
        .children
        .iter()
        .filter_map(|id| tree.nodes.get(id.0))
        .filter_map(|n| match &n.analysis {
            AnalysisType::Segment { segment_value, .. } => Some(segment_value.clone()),
            _ => None,
        })
        .collect();
    assert!(child_values.iter().any(|v| v == "EU"), "{child_values:?}");
    assert!(child_values.iter().any(|v| v == "NA"), "{child_values:?}");
}

/// No time column: the report degrades to composition (Concentration /
/// TopDominance) instead of erroring or returning nothing.
#[test]
fn csv_without_time_column_gets_composition_report() {
    let df = df!(
        "region" => ["EU", "EU", "EU", "EU", "EU", "EU", "NA", "APAC", "LATAM", "MEA"],
        "revenue" => [500.0, 480.0, 510.0, 490.0, 505.0, 495.0, 50.0, 30.0, 20.0, 10.0],
    )
    .unwrap();
    let schema = detect_schema(&df).unwrap();
    let engine = AnalysisEngine::new(2.0, 0.05, 3);
    let result = engine.run_drivers(&df, &schema).unwrap();
    assert!(
        !result.tree.roots.is_empty(),
        "composition-only report must still find the EU concentration"
    );
}

/// Legacy detector fingerprints must never collide with derived-pass ones:
/// run trends (which contains both passes) on data rich enough to fire both
/// and assert all non-empty fingerprints are unique per (detector story).
#[test]
fn legacy_and_derived_fingerprints_do_not_collide() {
    let df = eu_drop_dataframe();
    let schema = detect_schema(&df).unwrap();
    let engine = AnalysisEngine::new(2.0, 0.05, 3);

    let trends = engine.run_trends(&df, &schema).unwrap();
    let drivers = engine.run_drivers(&df, &schema).unwrap();

    // Within one tree, node fingerprints may legitimately repeat only for the
    // same story (dedup removes true duplicates from roots). Across the two
    // passes, distinct detector strings must keep the sets from colliding on
    // different story kinds: collect (kind_name, fingerprint) pairs and check
    // a fingerprint never maps to two different analysis kinds.
    let mut kind_by_fp: std::collections::HashMap<String, &'static str> =
        std::collections::HashMap::new();
    let mut collisions = HashSet::new();
    for tree in [&trends.tree, &drivers.tree] {
        for node in &tree.nodes {
            if node.fingerprint.is_empty() {
                continue;
            }
            let kind = node.analysis.kind_name();
            if let Some(prev) = kind_by_fp.insert(node.fingerprint.clone(), kind) {
                if prev != kind {
                    collisions.insert(node.fingerprint.clone());
                }
            }
        }
    }
    assert!(
        collisions.is_empty(),
        "fingerprints shared across analysis kinds: {collisions:?}"
    );
}
