//! Drivers report: "what makes up each KPI, and what drove its latest change?"
//!
//! Roots are `PeriodComparison` nodes (one per measure) whose children are the
//! ranked `Segment` drivers of the latest period-over-period delta — no new
//! `AnalysisType` variants, so every exhaustive match elsewhere keeps working.
//! A composition sub-pass (Concentration + TopDominance) makes the report
//! self-contained; on tables without a time column it IS the report.

use anyhow::Result;
use polars::prelude::*;

use crate::analysis::candidates::{
    Aggregation, DimensionIndex, EnumerationBudget, FilterSpec, MeasureRef, Provenance,
};
use crate::analysis::concentration::detect_concentration;
use crate::analysis::drivers::{decompose_latest_delta, DeltaDecomposition, RankedDriver};
use crate::analysis::period::get_period_labels;
use crate::analysis::scoring;
use crate::analysis::tree::{
    format_value, AnalysisResult, AnalysisTree, AnalysisType, FilterStep, NodeData, NodeId,
    ScoreBreakdown,
};
use crate::data::schema::DataSchema;

use super::cache::ColumnCache;
use super::meta::{measure_ref, set_legacy_meta};
use super::pipeline::granularity_name;
use super::AnalysisEngine;

/// Roots below this significance are noise, same floor as the derived pass.
const MIN_ROOT_SIGNIFICANCE: f64 = 0.5;

impl AnalysisEngine {
    /// Drivers report: composition and driver analysis (What is driving performance?)
    pub(super) fn run_drivers_impl(
        &self,
        df: &DataFrame,
        schema: &DataSchema,
    ) -> Result<AnalysisResult> {
        let mut tree = AnalysisTree::new();
        let mut first_level_count: usize = 0;
        let mut deeper_count: usize = 0;

        let cache = ColumnCache::new(df, schema)?;
        let period_labels: Vec<Option<String>> = if let Some(time_col) = &schema.time_column {
            get_period_labels(df.column(time_col)?, schema.time_granularity)?
        } else {
            Vec::new()
        };
        let gran = if period_labels.is_empty() {
            "all"
        } else {
            granularity_name(schema.time_granularity)
        };

        // The index powers both the delta decomposition (needs real periods)
        // and the composition sub-pass (period-agnostic: a single synthetic
        // period covers the CSV-without-dates case gracefully).
        let index_labels: Vec<Option<String>> = if period_labels.is_empty() {
            vec![Some("all".to_string()); df.height()]
        } else {
            period_labels.clone()
        };
        let budget = EnumerationBudget::default();
        let index = DimensionIndex::build(&cache, &index_labels, &budget);

        // KPIs first; fall back to every measure when none are flagged.
        let measures: Vec<String> = {
            let kpis: Vec<String> = schema
                .measure_columns
                .iter()
                .filter(|c| self.scoring_ctx.kpi_columns.contains(*c))
                .cloned()
                .collect();
            if kpis.is_empty() {
                schema.measure_columns.clone()
            } else {
                kpis
            }
        };

        // ── Delta decomposition per measure (needs ≥2 real periods) ─────────
        if period_labels.is_empty() {
        } else {
            for measure in &measures {
                first_level_count += 1;
                let Some((prev_period, curr_period, decomposition)) =
                    decompose_latest_delta(&index, measure)
                else {
                    continue;
                };
                deeper_count += decomposition.drivers.len();
                self.add_driver_root(
                    &mut tree,
                    measure,
                    &prev_period,
                    &curr_period,
                    &decomposition,
                    gran,
                );
            }
        }

        // ── Composition sub-pass: Concentration + TopDominance ──────────────
        let mut dominance_measures: Vec<MeasureRef> = vec![MeasureRef::RowCount];
        for m in &measures {
            dominance_measures.push(MeasureRef::Column(m.clone()));
        }
        for dim in &schema.dimension_columns {
            for measure in &dominance_measures {
                first_level_count += 1;
                self.detect_dominance(&index, dim, measure, gran, &mut tree);
            }
            for measure in &measures {
                first_level_count += 1;
                self.detect_drivers_concentration(&cache, measure, dim, gran, &mut tree);
            }
        }

        // Novelty decay + diverse top-N, but NO dedup: dedup exists to clean
        // up noisy scans, and both its passes break this report's deliberate
        // structure — story grouping folds the decomposition root into the
        // same-column Concentration root, and lattice collapse drops ranked
        // driver children (a contribution child rarely scores 1.2× its root).
        // The report emits exactly one root per (measure) and per
        // (measure, dimension) by construction, so there is nothing to dedup.
        crate::analysis::history::apply_novelty(&mut tree, &self.history, self.now_epoch);
        crate::analysis::select::select_top(&mut tree, self.select_top);

        Ok(AnalysisResult {
            tree,
            first_level_count,
            deeper_count,
        })
    }

    /// Root `PeriodComparison` + ranked `Segment` children for one measure.
    fn add_driver_root(
        &self,
        tree: &mut AnalysisTree,
        measure: &str,
        prev_period: &str,
        curr_period: &str,
        decomposition: &DeltaDecomposition,
        gran: &str,
    ) {
        if decomposition.drivers.is_empty() {
            return;
        }
        let delta_total = decomposition.delta_total();
        if delta_total == 0.0 && !decomposition.offsetting {
            return;
        }

        // Root evidence = the strongest driver's slice-level Welch test: a
        // total can only have moved for a reason at least one slice carries.
        let significance = decomposition
            .drivers
            .iter()
            .map(|d| d.significance.score)
            .fold(0.0_f64, f64::max);
        if significance < MIN_ROOT_SIGNIFICANCE {
            return;
        }

        let change_percent = if decomposition.total_prev.abs() > 1e-12 {
            delta_total / decomposition.total_prev.abs() * 100.0
        } else {
            0.0
        };
        // Impact: net movement relative to the previous total — or the gross
        // movement share when the story is offsetting segments.
        let moved = if decomposition.offsetting {
            decomposition.gross_movement
        } else {
            delta_total.abs()
        };
        let impact = if decomposition.total_prev.abs() > 1e-12 {
            (moved / decomposition.total_prev.abs()).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let kpi_boost = if self.scoring_ctx.kpi_columns.contains(measure) {
            1.5
        } else {
            1.0
        };
        let breakdown = ScoreBreakdown {
            significance,
            impact,
            novelty: 1.0,
            kpi_boost,
        };
        let score = scoring::total(&breakdown);

        let p_value = decomposition
            .drivers
            .iter()
            .map(|d| d.p_value)
            .fold(1.0_f64, f64::min);
        let analysis = AnalysisType::PeriodComparison {
            column: measure.to_string(),
            current_period: curr_period.to_string(),
            previous_period: prev_period.to_string(),
            current_value: decomposition.total_curr,
            previous_value: decomposition.total_prev,
            change_percent,
            p_value,
        };
        let description = format!(
            "Drivers of {measure}: {} → {} ({change_percent:+.1}%) from {prev_period} to {curr_period}",
            format_value(decomposition.total_prev),
            format_value(decomposition.total_curr),
        );

        let why = if decomposition.offsetting {
            "the total held roughly flat while segments moved in opposite directions — the shifts below cancelled each other out".to_string()
        } else {
            let top = &decomposition.drivers[0];
            format!(
                "{}=\"{}\" alone explains {:.0}% of the change",
                top.dimension,
                top.value,
                top.contribution_pct.abs().min(999.0)
            )
        };

        let data = driver_bars(decomposition, measure);
        let node_id = tree.add_root_full(analysis, score, breakdown, description, data);
        set_legacy_meta(
            tree,
            node_id,
            "drivers",
            measure_ref(measure),
            Aggregation::Sum,
            None,
            gran,
            why,
        );

        for driver in &decomposition.drivers {
            self.add_driver_child(tree, node_id, measure, driver, gran);
        }
    }

    #[allow(clippy::unused_self)] // method for symmetry with add_driver_root
    fn add_driver_child(
        &self,
        tree: &mut AnalysisTree,
        parent_id: NodeId,
        measure: &str,
        driver: &RankedDriver,
        gran: &str,
    ) {
        let breakdown = ScoreBreakdown {
            significance: driver.significance.score,
            impact: (driver.contribution_pct.abs() / 100.0).clamp(0.0, 1.0),
            novelty: 1.0,
            kpi_boost: 1.0,
        };
        let score = scoring::total(&breakdown);
        let analysis = AnalysisType::Segment {
            target_column: measure.to_string(),
            segment_column: driver.dimension.clone(),
            segment_value: driver.value.clone(),
            contribution: driver.delta,
            change_percent: driver.change_percent,
            contribution_pct: driver.contribution_pct,
            p_value: driver.p_value,
        };
        let description = format!(
            "{}=\"{}\": {} → {} (Δ {}, {:.0}% of the movement)",
            driver.dimension,
            driver.value,
            format_value(driver.prev_value),
            format_value(driver.curr_value),
            format_value(driver.delta),
            driver.contribution_pct.abs()
        );
        let child_id =
            tree.add_child_full(parent_id, analysis, score, breakdown, description, None);
        let provenance = Provenance {
            measure: measure_ref(measure),
            aggregation: Aggregation::Sum,
            filters: vec![FilterSpec {
                column: driver.dimension.clone(),
                value: driver.value.clone(),
            }],
            derivations: Vec::new(),
            granularity: gran.to_string(),
        };
        tree.set_insight_meta(
            child_id,
            format!(
                "moved {} of the total's {} shift",
                format_value(driver.delta.abs()),
                format_value((driver.curr_value - driver.prev_value).abs())
            ),
            provenance.steps(),
            provenance.depth(),
            provenance.fingerprint("drivers"),
        );
        tree.set_filter_chain(
            child_id,
            vec![FilterStep {
                column: driver.dimension.clone(),
                op: "eq".to_string(),
                value: driver.value.clone(),
            }],
        );
    }

    /// Concentration finding for one (measure, dimension), same shape as the
    /// Trends pass so fingerprints and dedup line up across reports.
    fn detect_drivers_concentration(
        &self,
        cache: &ColumnCache,
        column: &str,
        segment_col: &str,
        gran: &str,
        tree: &mut AnalysisTree,
    ) {
        let Some(target_values) = cache.numeric.get(column) else {
            return;
        };
        let Some(segment_values) = cache.dimension.get(segment_col) else {
            return;
        };
        let Some(result) = detect_concentration(column, segment_col, target_values, segment_values)
        else {
            return;
        };
        let description = format!(
            "Concentration in '{column}' by '{segment_col}': HHI={:.2}, top {} = {:.0}%",
            result.hhi, result.top_n, result.top_share
        );
        let data = Some(NodeData::Lorenz {
            cumulative_share: result.lorenz_share,
            cumulative_population: result.lorenz_population,
            gini: result.gini,
        });
        let (n_segments, top_n, top_share) = (result.n_segments, result.top_n, result.top_share);
        let analysis = AnalysisType::Concentration {
            column: result.column,
            segment_column: result.segment_column,
            hhi: result.hhi,
            top_n: result.top_n,
            top_share: result.top_share,
            hhi_delta: None,
            n_segments: result.n_segments,
            n_rows: result.n_rows,
        };
        let breakdown = scoring::score(&analysis, &self.scoring_ctx);
        if !scoring::passes_floor(&breakdown, &self.scoring_ctx) {
            return;
        }
        let score = scoring::total(&breakdown);
        let node_id = tree.add_root_full(analysis, score, breakdown, description, data);
        set_legacy_meta(
            tree,
            node_id,
            "concentration",
            measure_ref(column),
            Aggregation::Sum,
            Some(segment_col),
            gran,
            format!(
                "covers the whole table; an even split across {n_segments} values would put the top {top_n} far below {top_share:.0}%"
            ),
        );
    }
}

/// SegmentBars over the best-covering dimension: bar per driver value of the
/// dimension that carries the most absolute movement.
fn driver_bars(decomposition: &DeltaDecomposition, measure: &str) -> Option<NodeData> {
    use std::collections::HashMap;
    let mut moved_by_dim: HashMap<&str, f64> = HashMap::new();
    for d in &decomposition.drivers {
        *moved_by_dim.entry(d.dimension.as_str()).or_insert(0.0) += d.delta.abs();
    }
    let best_dim = moved_by_dim
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))?
        .0
        .to_string();
    let bars: Vec<&RankedDriver> = decomposition
        .drivers
        .iter()
        .filter(|d| d.dimension == best_dim)
        .collect();
    if bars.is_empty() {
        return None;
    }
    Some(NodeData::SegmentBars {
        labels: bars.iter().map(|d| d.value.clone()).collect(),
        values: bars.iter().map(|d| d.delta).collect(),
        contributions_pct: bars.iter().map(|d| d.contribution_pct).collect(),
        value_label: Some(format!("Change in {measure}")),
    })
}
