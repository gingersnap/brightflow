//! Trends report: "what is changing over time?"

use std::collections::{HashMap, VecDeque};

use anyhow::Result;
use polars::prelude::*;

use crate::analysis::dedup;
use crate::analysis::distribution_shift::detect_distribution_shift;
use crate::analysis::forecast::detect_forecast_deviation;
use crate::analysis::membership::detect_membership_change;
use crate::analysis::outlier_cluster::find_outlier_clusters;
use crate::analysis::period::{aggregate_by_period_cached, extract_timestamps, get_period_labels};
use crate::analysis::seasonality::detect_seasonality;
use crate::analysis::tree::{AnalysisResult, AnalysisTree, AnalysisType, NodeData};
use crate::analysis::trend::{detect_trend, detect_trend_in_series};
use crate::data::schema::DataSchema;

use super::cache::ColumnCache;
use super::meta::{add_scored_root, measure_ref, RootMeta};
use super::payloads::{
    build_forecast_data, build_outlier_cluster_data, build_seasonality_data, build_trend_data,
    build_trend_series_data,
};
use super::pipeline::granularity_name;
use super::{latest_two_periods, AnalysisEngine, AnalysisTask};
use crate::analysis::candidates::{Aggregation, MeasureRef};

/// Loop-invariant context the trends arms share, so each arm method takes one
/// argument instead of re-threading cache/periods/granularity.
struct TrendCtx<'a> {
    cache: &'a ColumnCache,
    period_labels: &'a [Option<String>],
    gran: &'a str,
}

impl AnalysisEngine {
    /// Trends report: time-based patterns and forecasting (What's changing over time?)
    pub(super) fn run_trends_impl(
        &self,
        df: &DataFrame,
        schema: &DataSchema,
    ) -> Result<AnalysisResult> {
        let mut queue: VecDeque<AnalysisTask> = VecDeque::new();
        let mut tree = AnalysisTree::with_labels(schema.labels.clone());
        let mut first_level_count: usize = 0;

        let cache = ColumnCache::new(df, schema)?;
        let period_labels: Vec<Option<String>> = if let Some(time_col) = &schema.time_column {
            get_period_labels(df.column(time_col)?, schema.time_granularity)?
        } else {
            Vec::new()
        };
        let gran = granularity_name(schema.time_granularity);

        // The derived-series pass covers trends, change points, and
        // last-period anomalies over counts/sums/means/shares/ranks — for
        // whole-table AND per-segment slices — with typed nulls. It needs
        // periods; without a time column the legacy raw-row tasks run instead.
        let has_periods = !period_labels.is_empty();
        if has_periods {
            first_level_count += self.run_derived_series_pass(
                &cache,
                &period_labels,
                schema.time_granularity,
                &mut tree,
            );
        }

        // Queue the analyses the derived-series pass does not cover.
        for col in &schema.measure_columns {
            if !has_periods {
                queue.push_back(AnalysisTask::DetectTrend {
                    column: col.clone(),
                });
            }
            if schema.time_column.is_some() {
                queue.push_back(AnalysisTask::DetectSeasonality {
                    column: col.clone(),
                });
                queue.push_back(AnalysisTask::DetectForecastDeviation {
                    column: col.clone(),
                });
                queue.push_back(AnalysisTask::DetectDistributionShift {
                    column: col.clone(),
                });
            }
            // Concentration: per (measure, dimension) — fan out
            for seg_col in &schema.dimension_columns {
                queue.push_back(AnalysisTask::DetectConcentration {
                    column: col.clone(),
                    segment_col: seg_col.clone(),
                });
            }
        }
        // Membership: per dimension
        if schema.time_column.is_some() {
            queue.push_back(AnalysisTask::FindOutlierClusters);
            for seg_col in &schema.dimension_columns {
                queue.push_back(AnalysisTask::DetectMembershipChange {
                    segment_col: seg_col.clone(),
                });
            }
        }

        let ctx = TrendCtx {
            cache: &cache,
            period_labels: &period_labels,
            gran,
        };
        while let Some(task) = queue.pop_front() {
            first_level_count += 1;
            match task {
                AnalysisTask::DetectTrend { column } => {
                    self.trend_arm(df, &ctx, &column, &mut tree)?;
                },
                AnalysisTask::DetectSeasonality { column } => {
                    self.seasonality_arm(df, schema, &ctx, &column, &mut tree);
                },
                AnalysisTask::DetectForecastDeviation { column } => {
                    self.forecast_deviation_arm(&ctx, &column, &mut tree);
                },
                AnalysisTask::DetectConcentration {
                    column,
                    segment_col,
                } => {
                    self.add_concentration_finding(&cache, &column, &segment_col, gran, &mut tree);
                },
                AnalysisTask::DetectDistributionShift { column } => {
                    self.distribution_shift_arm(&ctx, &column, &mut tree);
                },
                AnalysisTask::DetectMembershipChange { segment_col } => {
                    self.membership_change_arm(&ctx, &segment_col, &mut tree);
                },
                AnalysisTask::FindOutlierClusters => {
                    self.outlier_clusters_arm(&ctx, &mut tree);
                },
                _ => {}, // Ignore other task types in trends
            }
        }

        // Dedup, then diversity-select the top roots
        dedup::dedup(&mut tree);
        crate::analysis::history::apply_novelty(&mut tree, &self.history, self.now_epoch);
        crate::analysis::select::select_top(&mut tree, self.select_top);

        Ok(AnalysisResult {
            tree,
            first_level_count,
            deeper_count: 0,
        })
    }

    /// Whole-table trend over the period-aggregated series (or raw rows when
    /// no time column exists — the only case this arm runs in practice).
    fn trend_arm(
        &self,
        df: &DataFrame,
        ctx: &TrendCtx<'_>,
        column: &str,
        tree: &mut AnalysisTree,
    ) -> Result<()> {
        // Regress on the period-aggregated series when a time column exists —
        // regressing on raw row index conflates insertion order with time and
        // inflates n.
        let period_series: Option<(Vec<String>, Vec<f64>)> = if ctx.period_labels.is_empty() {
            None
        } else {
            ctx.cache.numeric.get(column).map(|values| {
                let stats = aggregate_by_period_cached(values, ctx.period_labels);
                (
                    stats.iter().map(|s| s.period_label.clone()).collect(),
                    stats.iter().map(|s| s.mean).collect(),
                )
            })
        };
        let trend_opt = match &period_series {
            Some((_, means)) => detect_trend_in_series(column, means),
            None => detect_trend(df, column)?,
        };
        let Some(trend) = trend_opt else {
            return Ok(());
        };
        if !(trend.p_value < self.p_threshold && trend.r_squared > 0.5) {
            return Ok(());
        }
        let direction_str = match trend.direction {
            crate::analysis::tree::TrendDirection::Increasing => "increasing",
            crate::analysis::tree::TrendDirection::Decreasing => "decreasing",
        };
        let description = format!(
            "Significant {} trend in '{}': slope={:.4}, R²={:.3} (p={:.4})",
            direction_str, column, trend.slope, trend.r_squared, trend.p_value
        );
        let data = match &period_series {
            Some((labels, means)) => Some(build_trend_series_data(labels, means, trend.slope)),
            None => ctx
                .cache
                .numeric
                .get(column)
                .map(|v| build_trend_data(v, trend.slope)),
        };
        let analysis = AnalysisType::Trend {
            column: trend.column,
            direction: trend.direction,
            slope: trend.slope,
            r_squared: trend.r_squared,
            p_value: trend.p_value,
        };
        add_scored_root(
            tree,
            &self.scoring_ctx,
            analysis,
            description,
            data,
            RootMeta {
                detector: "trend_legacy",
                measure: measure_ref(column),
                agg: Aggregation::Mean,
                dimension: None,
                granularity: if ctx.period_labels.is_empty() {
                    "row"
                } else {
                    ctx.gran
                },
                why: format!(
                    "covers the whole table; a flat series would fit this well {:.1}% of the time",
                    (trend.p_value * 100.0).min(100.0)
                ),
            },
        );
        Ok(())
    }

    /// Autocorrelation-based seasonality over the raw timestamped values.
    fn seasonality_arm(
        &self,
        df: &DataFrame,
        schema: &DataSchema,
        ctx: &TrendCtx<'_>,
        column: &str,
        tree: &mut AnalysisTree,
    ) {
        let Some(metric_values) = ctx.cache.numeric.get(column) else {
            return;
        };
        let timestamps_sec: Vec<i64> = if let Some(time_col) = &schema.time_column {
            if let Ok(col) = df.column(time_col) {
                extract_timestamps(col)
                    .unwrap_or_default()
                    .into_iter()
                    .flatten()
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        if timestamps_sec.len() != metric_values.len() {
            return;
        }
        let Some(result) = detect_seasonality(column, metric_values, &timestamps_sec) else {
            return;
        };
        let result_p = result.p_value;
        let description = format!(
            "Seasonality detected in '{}': {} pattern (r={:.3}, p={:.4})",
            column, result.period_name, result.autocorrelation, result.p_value
        );
        let data = build_seasonality_data(metric_values, ctx.period_labels);
        let analysis = AnalysisType::Seasonality {
            column: result.column,
            period_name: result.period_name,
            autocorrelation: result.autocorrelation,
            p_value: result.p_value,
        };
        // Lag (period_name) deliberately excluded from the fingerprint:
        // "revenue is seasonal" is one story even if the detected lag wobbles
        // between runs.
        add_scored_root(
            tree,
            &self.scoring_ctx,
            analysis,
            description,
            data,
            RootMeta {
                detector: "seasonality",
                measure: measure_ref(column),
                agg: Aggregation::Mean,
                dimension: None,
                granularity: ctx.gran,
                why: format!(
                    "covers the whole table; an unpatterned series would repeat this strongly {:.1}% of the time",
                    (result_p * 100.0).min(100.0)
                ),
            },
        );
    }

    /// Last period against the trend line through its predecessors.
    fn forecast_deviation_arm(&self, ctx: &TrendCtx<'_>, column: &str, tree: &mut AnalysisTree) {
        let Some(metric_values) = ctx.cache.numeric.get(column) else {
            return;
        };
        let mut period_sums: HashMap<String, (f64, usize)> = HashMap::new();
        for (val, period_opt) in metric_values.iter().zip(ctx.period_labels.iter()) {
            if let Some(period) = period_opt {
                let entry = period_sums.entry(period.clone()).or_insert((0.0, 0));
                entry.0 += val;
                entry.1 += 1;
            }
        }
        let mut period_values: Vec<(String, f64)> = period_sums
            .into_iter()
            .map(|(period, (sum, count))| (period, sum / count as f64))
            .collect();
        period_values.sort_by(|a, b| a.0.cmp(&b.0));

        let Some(deviation) = detect_forecast_deviation(column, &period_values) else {
            return;
        };
        if !(deviation.deviation_percent.abs() > 20.0 && deviation.p_value < 0.05) {
            return;
        }
        let (deviation_expected, deviation_p) = (deviation.expected, deviation.p_value);
        let description = format!(
            "Forecast deviation in '{}' for {}: actual {:.2} vs expected {:.2} ({:.1}% deviation, p={:.4})",
            column, deviation.period, deviation.actual, deviation.expected,
            deviation.deviation_percent, deviation.p_value
        );
        let data = build_forecast_data(&period_values, deviation.expected, deviation.actual);
        let analysis = AnalysisType::ForecastDeviation {
            column: deviation.column,
            period: deviation.period,
            actual: deviation.actual,
            expected: deviation.expected,
            deviation_percent: deviation.deviation_percent,
            p_value: deviation.p_value,
        };
        add_scored_root(
            tree,
            &self.scoring_ctx,
            analysis,
            description,
            data,
            RootMeta {
                detector: "forecast_deviation",
                measure: measure_ref(column),
                agg: Aggregation::Mean,
                dimension: None,
                granularity: ctx.gran,
                why: format!(
                    "covers the whole table; the trend through past periods predicted {} — a miss this large happens {:.1}% of the time",
                    crate::analysis::tree::format_value(deviation_expected),
                    (deviation_p * 100.0).min(100.0)
                ),
            },
        );
    }

    /// KS test of the latest period's distribution against the previous one.
    fn distribution_shift_arm(&self, ctx: &TrendCtx<'_>, column: &str, tree: &mut AnalysisTree) {
        let Some(values) = ctx.cache.numeric.get(column) else {
            return;
        };
        let Some((prev, curr)) = latest_two_periods(ctx.period_labels) else {
            return;
        };
        let Some(result) =
            detect_distribution_shift(column, values, ctx.period_labels, &prev, &curr)
        else {
            return;
        };
        let shift_p = result.p_value;
        let description = format!(
            "Distribution of '{column}' shifted from {prev} to {curr} (KS={:.2}, p={:.4})",
            result.ks_statistic, result.p_value
        );
        let data = Some(NodeData::HistogramPair {
            bin_edges: result.bin_edges,
            previous: result.previous_hist,
            current: result.current_hist,
        });
        let analysis = AnalysisType::DistributionShift {
            column: result.column,
            previous_period: result.previous_period,
            current_period: result.current_period,
            ks_statistic: result.ks_statistic,
            p_value: result.p_value,
        };
        add_scored_root(
            tree,
            &self.scoring_ctx,
            analysis,
            description,
            data,
            RootMeta {
                detector: "distribution_shift",
                measure: measure_ref(column),
                agg: Aggregation::Mean,
                dimension: None,
                granularity: ctx.gran,
                why: format!(
                    "covers the whole table; identical distributions would drift this far {:.1}% of the time",
                    (shift_p * 100.0).min(100.0)
                ),
            },
        );
    }

    /// Values of a dimension appearing/disappearing between the latest two
    /// periods.
    fn membership_change_arm(
        &self,
        ctx: &TrendCtx<'_>,
        segment_col: &str,
        tree: &mut AnalysisTree,
    ) {
        let Some(seg_values) = ctx.cache.dimension.get(segment_col) else {
            return;
        };
        let Some((prev, curr)) = latest_two_periods(ctx.period_labels) else {
            return;
        };
        let Some(result) =
            detect_membership_change(segment_col, seg_values, ctx.period_labels, &prev, &curr)
        else {
            return;
        };
        let (member_added, member_removed, member_prev) =
            (result.added.len(), result.removed.len(), result.prev_size);
        let description = format!(
            "{segment_col} membership: +{} new / -{} disappeared between {prev} and {curr}",
            result.added.len(),
            result.removed.len()
        );
        let data = Some(NodeData::MembershipDiff {
            added: result.added.clone(),
            removed: result.removed.clone(),
        });
        let analysis = AnalysisType::MembershipChange {
            segment_column: result.segment_column,
            previous_period: result.previous_period,
            current_period: result.current_period,
            added_count: result.added.len(),
            removed_count: result.removed.len(),
            prev_size: result.prev_size,
            curr_size: result.curr_size,
        };
        add_scored_root(
            tree,
            &self.scoring_ctx,
            analysis,
            description,
            data,
            RootMeta {
                detector: "membership_change",
                measure: MeasureRef::RowCount,
                agg: Aggregation::Count,
                dimension: Some(segment_col),
                granularity: ctx.gran,
                why: format!(
                    "{member_added} joined and {member_removed} left, out of {member_prev} values in the previous period"
                ),
            },
        );
    }

    /// Several tracked columns moving together in one period.
    fn outlier_clusters_arm(&self, ctx: &TrendCtx<'_>, tree: &mut AnalysisTree) {
        let clusters = find_outlier_clusters(ctx.cache, ctx.period_labels, self.z_threshold);
        for cluster in clusters {
            let (cluster_size, cluster_tested) = (cluster.columns.len(), cluster.columns_tested);
            let cluster_cols = cluster.columns.join(",");
            let columns_str = cluster.columns.join(", ");
            let description = format!(
                "Outlier cluster in {}: {} {} in {}",
                cluster.period,
                cluster.direction,
                columns_str,
                if cluster.common_segments.is_empty() {
                    "all segments".to_string()
                } else {
                    cluster
                        .common_segments
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            );
            let data = build_outlier_cluster_data(
                ctx.cache,
                ctx.period_labels,
                &cluster.period,
                &cluster.columns,
            );
            let analysis = AnalysisType::OutlierCluster {
                period: cluster.period,
                columns: cluster.columns.clone(),
                direction: cluster.direction,
                cluster_size: cluster.columns.len(),
                columns_tested: cluster.columns_tested,
                n_periods: cluster.n_periods,
            };
            add_scored_root(
                tree,
                &self.scoring_ctx,
                analysis,
                description,
                data,
                RootMeta {
                    detector: "outlier_cluster",
                    measure: MeasureRef::Column(cluster_cols.clone()),
                    agg: Aggregation::Mean,
                    dimension: None,
                    granularity: ctx.gran,
                    why: format!(
                        "{cluster_size} of {cluster_tested} tracked columns moved together in one period — unrelated columns rarely do"
                    ),
                },
            );
        }
    }
}
