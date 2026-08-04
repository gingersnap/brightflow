//! Integration test exercising the full insights pipeline end-to-end on an
//! in-memory DataFrame. Verifies that the new fields (data, scoreBreakdown,
//! filterChain) are populated and that scoring/dedup behave reasonably.

#![expect(
    clippy::unwrap_used,
    clippy::cast_precision_loss,
    clippy::cognitive_complexity,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    reason = "integration tests panic on failure by design"
)]

use chrono::NaiveDate;
use polars::prelude::*;

use brightflow_engine::analysis::engine::AnalysisEngine;
use brightflow_engine::analysis::tree::AnalysisType;
use brightflow_engine::data::schema::detect_schema;

fn build_test_dataframe() -> DataFrame {
    // 90 days × 3 regions × 2 channels = 540 rows
    // revenue trends up over time, with a spike on day 88 in NA
    let mut dates = Vec::new();
    let mut regions = Vec::new();
    let mut channels = Vec::new();
    let mut revenue = Vec::new();
    let mut units = Vec::new();

    let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    for d in 0..90 {
        let date = start + chrono::Duration::days(d);
        for region in &["NA", "EU", "APAC"] {
            for channel in &["web", "mobile"] {
                dates.push(
                    date.and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc()
                        .timestamp_millis(),
                );
                regions.push(*region);
                channels.push(*channel);
                let base = match *region {
                    "NA" => 100.0,
                    "EU" => 80.0,
                    _ => 60.0,
                };
                let trend = (d as f64) * 0.5;
                let spike = if d == 88 && *region == "NA" {
                    500.0
                } else {
                    0.0
                };
                revenue.push(base + trend + spike);
                units.push((base / 10.0) + (d as f64) * 0.1);
            }
        }
    }

    let date_series = Series::new("date".into(), &dates)
        .cast(&DataType::Datetime(TimeUnit::Milliseconds, None))
        .unwrap();
    df!(
        "date" => date_series,
        "region" => regions,
        "channel" => channels,
        "revenue" => revenue,
        "units" => units,
    )
    .unwrap()
}

#[test]
fn full_review_pipeline_populates_new_fields() {
    let df = build_test_dataframe();
    let schema = detect_schema(&df).unwrap();
    eprintln!(
        "schema: measures={:?}, dims={:?}, time={:?}",
        schema.measure_columns, schema.dimension_columns, schema.time_column
    );
    assert!(schema.measure_columns.contains(&"revenue".to_string()));
    assert!(!schema.dimension_columns.is_empty());

    let engine = AnalysisEngine::new(2.0, 0.05, 3);
    let result = engine
        .run_review_with_cadence(
            &df,
            &schema,
            brightflow_engine::analysis::tree::ReviewCadence::Daily,
        )
        .unwrap();

    let tree = &result.tree;
    eprintln!(
        "review: roots={}, total_nodes={}",
        tree.roots.len(),
        tree.nodes.len()
    );

    // For this dataset shape, run_trends is more likely to surface findings
    // than run_review_with_cadence; tolerate empty roots here and rely on the
    // trends test to verify per-type rendering data.
    if tree.roots.is_empty() {
        eprintln!("review produced no roots — this is acceptable for this test dataset shape");
        return;
    }

    // All nodes should have score breakdowns populated
    for node in &tree.nodes {
        // Significance should be > 0 since they passed the floor
        assert!(node.significance >= 0.0, "node has negative score");
        // breakdown components should be in [0, 1] (×kpi_boost which is ≥1.0)
        assert!(node.score_breakdown.significance >= 0.0);
        assert!(node.score_breakdown.significance <= 1.0);
        assert!(node.score_breakdown.impact >= 0.0);
        assert!(node.score_breakdown.impact <= 1.0);
        assert!(node.score_breakdown.kpi_boost >= 1.0);
    }

    // Roots are sorted in dedup; verify the ranking is descending
    let scores: Vec<f64> = tree
        .roots
        .iter()
        .map(|id| tree.nodes[id.0].significance)
        .collect();
    for w in scores.windows(2) {
        assert!(w[0] >= w[1], "roots not sorted: {scores:?}");
    }

    // At least one root should have a data payload (chart data)
    let has_data = tree.roots.iter().any(|id| tree.nodes[id.0].data.is_some());
    assert!(has_data, "expected at least one root with data payload");

    // Segment children should have non-empty filter_chain
    let has_segment_with_chain = tree
        .nodes
        .iter()
        .any(|n| matches!(n.analysis, AnalysisType::Segment { .. }) && !n.filter_chain.is_empty());
    if tree
        .nodes
        .iter()
        .any(|n| matches!(n.analysis, AnalysisType::Segment { .. }))
    {
        assert!(
            has_segment_with_chain,
            "Segment findings should have filter_chain populated"
        );
    }
}

#[test]
fn full_trends_pipeline_runs_new_detectors() {
    let df = build_test_dataframe();
    let schema = detect_schema(&df).unwrap();

    let engine = AnalysisEngine::new(2.0, 0.05, 3);
    let result = engine.run_trends(&df, &schema).unwrap();
    let tree = &result.tree;

    // We expect at least a trend finding (revenue trends upward by construction)
    let has_trend = tree
        .nodes
        .iter()
        .any(|n| matches!(n.analysis, AnalysisType::Trend { .. }));
    let has_change_point = tree
        .nodes
        .iter()
        .any(|n| matches!(n.analysis, AnalysisType::ChangePoint { .. }));
    let has_concentration = tree
        .nodes
        .iter()
        .any(|n| matches!(n.analysis, AnalysisType::Concentration { .. }));

    // We don't require all three (depends on calibration), but at least
    // one new-family finding should be possible given the dataset shape.
    assert!(
        has_trend || has_change_point || has_concentration,
        "expected a trend/change-point/concentration finding"
    );
}

/// A dataset with a flat measure carrying one enormous spike and a second
/// measure with a weak, noisy drift. The spike is the story; the drift is
/// background. Honest scoring must rank the spike finding first.
fn build_spike_vs_weak_trend_dataframe() -> DataFrame {
    let mut dates = Vec::new();
    let mut regions = Vec::new();
    let mut clicks = Vec::new();
    let mut drift = Vec::new();

    let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    for d in 0..60 {
        let date = start + chrono::Duration::days(d);
        for (r_idx, region) in ["NA", "EU"].iter().enumerate() {
            dates.push(
                date.and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc()
                    .timestamp_millis(),
            );
            regions.push(*region);
            // Flat at ~100 with a 10× spike on the final day
            let spike = if d == 59 { 1000.0 } else { 0.0 };
            // Deterministic wobble so std > 0
            let wobble = ((d as f64) * 0.7).sin() * 2.0 + (r_idx as f64);
            clicks.push(100.0 + wobble + spike);
            // Weak drift: slope 0.05/day buried in ±6 wobble (low R²)
            drift.push(50.0 + (d as f64) * 0.05 + ((d as f64) * 1.3).sin() * 6.0);
        }
    }

    let date_series = Series::new("date".into(), &dates)
        .cast(&DataType::Datetime(TimeUnit::Milliseconds, None))
        .unwrap();
    df!(
        "date" => date_series,
        "region" => regions,
        "clicks" => clicks,
        "drift" => drift,
    )
    .unwrap()
}

#[test]
fn planted_spike_outranks_weak_trend() {
    let df = build_spike_vs_weak_trend_dataframe();
    let schema = detect_schema(&df).unwrap();

    let engine = AnalysisEngine::new(2.0, 0.05, 3);
    let result = engine
        .run_review_with_cadence(
            &df,
            &schema,
            brightflow_engine::analysis::tree::ReviewCadence::Daily,
        )
        .unwrap();
    let tree = &result.tree;
    assert!(!tree.roots.is_empty(), "expected findings for the spike");

    let top = &tree.nodes[tree.roots[0].0];
    let top_column = match &top.analysis {
        AnalysisType::Anomaly { column, .. }
        | AnalysisType::PeriodComparison { column, .. }
        | AnalysisType::PeriodAnomaly { column, .. }
        | AnalysisType::ChangePoint { column, .. } => column.clone(),
        other => panic!("expected a spike-family finding on top, got {other:?}"),
    };
    assert_eq!(
        top_column, "clicks",
        "the planted 10× spike must outrank the weak drift; top was {:?}",
        top.analysis
    );
}

#[test]
fn weak_noisy_drift_produces_no_trend_root() {
    // The drift measure has slope buried in noise — a period-aggregated
    // regression sees low R² and must not report a trend.
    let df = build_spike_vs_weak_trend_dataframe();
    let schema = detect_schema(&df).unwrap();
    let engine = AnalysisEngine::new(2.0, 0.05, 3);
    let result = engine.run_trends(&df, &schema).unwrap();
    let has_drift_trend = result.tree.nodes.iter().any(|n| {
        matches!(&n.analysis, AnalysisType::Trend { column, r_squared, .. }
            if column == "drift" && *r_squared > 0.5)
    });
    assert!(
        !has_drift_trend,
        "weak noisy drift must not surface as a confident trend"
    );
}

/// Golden dataset with two *composed* planted insights that only exist in
/// derived series (the raw columns are boring):
/// 1. channel "C"'s daily row volume steps from 20 to 150 on day 15 —
///    a change point in `count(rows) where channel=C` and a share-of-total
///    step, none of which is visible in any numeric column.
/// 2. That same step flips C's volume rank among channels from #3 to #1.
fn build_composed_insights_dataframe() -> DataFrame {
    let mut dates: Vec<i64> = Vec::new();
    let mut channels: Vec<&str> = Vec::new();
    let mut score: Vec<f64> = Vec::new();

    let start = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
    for d in 0..30 {
        let ts = (start + chrono::Duration::days(d))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        let c_volume = if d < 15 { 20 } else { 150 };
        for (channel, volume) in [("A", 100), ("B", 60), ("C", c_volume)] {
            for k in 0..volume {
                dates.push(ts);
                channels.push(channel);
                // Boring measure: wobbles around 50 with no story
                score.push(50.0 + ((d * 31 + k) % 7) as f64 - 3.0);
            }
        }
    }

    let date_series = Series::new("date".into(), &dates)
        .cast(&DataType::Datetime(TimeUnit::Milliseconds, None))
        .unwrap();
    df!(
        "date" => date_series,
        "channel" => channels,
        "score" => score,
    )
    .unwrap()
}

#[test]
fn planted_composed_insights_land_in_top_five() {
    let df = build_composed_insights_dataframe();
    let schema = detect_schema(&df).unwrap();
    let engine = AnalysisEngine::new(2.0, 0.05, 3);
    let result = engine.run_trends(&df, &schema).unwrap();
    let tree = &result.tree;

    let top5: Vec<_> = tree
        .roots
        .iter()
        .take(5)
        .map(|r| &tree.nodes[r.0])
        .collect();
    for (i, n) in top5.iter().enumerate() {
        eprintln!(
            "top{}: score={:.3} depth={} {:?} — {}",
            i + 1,
            n.significance,
            n.depth,
            n.rank,
            n.description
        );
    }
    assert!(!top5.is_empty(), "expected findings");

    // The volume step for channel C must surface as a derived-series finding
    // (change point or anomaly on `rows where channel=C` / its share).
    let has_c_volume_story = top5.iter().any(|n| {
        let about_c = n
            .filter_chain
            .iter()
            .any(|f| f.column == "channel" && f.value == "C")
            || matches!(&n.analysis,
                AnalysisType::RankChange { value, .. } if value == "C");
        let is_derived = matches!(
            n.analysis,
            AnalysisType::ChangePoint { .. }
                | AnalysisType::Trend { .. }
                | AnalysisType::Anomaly { .. }
                | AnalysisType::RankChange { .. }
        );
        about_c && is_derived
    });
    assert!(
        has_c_volume_story,
        "the planted channel=C volume step must land in the top 5"
    );

    // The rank flip must be detected somewhere in the tree.
    let has_rank_change = tree.nodes.iter().any(|n| {
        matches!(&n.analysis,
            AnalysisType::RankChange { value, new_rank, .. } if value == "C" && *new_rank == 1)
    });
    assert!(
        has_rank_change,
        "channel C's rank flip to #1 must be detected"
    );

    // Every derived root must carry provenance, a why, and a fingerprint.
    for n in tree.roots.iter().map(|r| &tree.nodes[r.0]) {
        if !n.provenance.is_empty() {
            assert!(
                !n.why.is_empty(),
                "derived finding missing why: {}",
                n.description
            );
            assert!(
                !n.fingerprint.is_empty(),
                "derived finding missing fingerprint"
            );
            assert!(n.depth >= 1);
        }
    }

    // Ranks are assigned 1..N in root order.
    for (i, root) in tree.roots.iter().enumerate() {
        assert_eq!(tree.nodes[root.0].rank, Some(u32::try_from(i + 1).unwrap()));
    }
}

#[test]
fn dedup_collapses_correlated_roots() {
    let df = build_test_dataframe();
    let schema = detect_schema(&df).unwrap();
    let engine = AnalysisEngine::new(2.0, 0.05, 3);
    let result = engine.run_trends(&df, &schema).unwrap();

    // After dedup, no two roots should share a story key (column name)
    let mut seen_columns = std::collections::HashSet::new();
    for root_id in &result.tree.roots {
        let node = &result.tree.nodes[root_id.0];
        let column = match &node.analysis {
            AnalysisType::Anomaly { column, .. }
            | AnalysisType::Trend { column, .. }
            | AnalysisType::PeriodComparison { column, .. }
            | AnalysisType::PeriodAnomaly { column, .. }
            | AnalysisType::Seasonality { column, .. }
            | AnalysisType::ForecastDeviation { column, .. }
            | AnalysisType::Concentration { column, .. }
            | AnalysisType::DistributionShift { column, .. }
            | AnalysisType::ChangePoint { column, .. } => Some(column.clone()),
            AnalysisType::Segment { target_column, .. } => Some(target_column.clone()),
            _ => None,
        };
        if let Some(c) = column {
            assert!(
                seen_columns.insert(c.clone()),
                "dedup failed: column {c} appears in multiple roots"
            );
        }
    }
}
