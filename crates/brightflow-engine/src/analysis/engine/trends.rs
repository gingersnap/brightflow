//! Trends report: "what is changing over time?"

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use anyhow::Result;
use polars::prelude::*;

use crate::analysis::concentration::detect_concentration;
use crate::analysis::dedup;
use crate::analysis::distribution_shift::detect_distribution_shift;
use crate::analysis::forecast::detect_forecast_deviation;
use crate::analysis::membership::detect_membership_change;
use crate::analysis::outlier_cluster::find_outlier_clusters;
use crate::analysis::period::{aggregate_by_period_cached, extract_timestamps, get_period_labels};
use crate::analysis::scoring;
use crate::analysis::seasonality::detect_seasonality;
use crate::analysis::tree::{AnalysisResult, AnalysisTree, AnalysisType, NodeData};
use crate::analysis::trend::{detect_trend, detect_trend_in_series};
use crate::data::schema::DataSchema;
use crate::debug::DebugLog;

use super::cache::ColumnCache;
use super::payloads::{
    build_forecast_data, build_outlier_cluster_data, build_seasonality_data, build_trend_data,
    build_trend_series_data,
};
use super::{AnalysisEngine, AnalysisTask};

impl AnalysisEngine {
    /// Trends report: time-based patterns and forecasting (What's changing over time?)
    pub(super) fn run_trends_impl(
        &self,
        df: &DataFrame,
        schema: &DataSchema,
        debug: &DebugLog,
    ) -> Result<AnalysisResult> {
        let start_time = Instant::now();
        let mut queue: VecDeque<AnalysisTask> = VecDeque::new();
        let mut tree = AnalysisTree::new();
        let mut first_level_count: usize = 0;

        debug.section("TRENDS REPORT");

        let cache = ColumnCache::new(df, schema)?;
        let period_labels: Vec<Option<String>> = if let Some(time_col) = &schema.time_column {
            get_period_labels(df.column(time_col)?, schema.time_granularity)?
        } else {
            Vec::new()
        };

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

        while let Some(task) = queue.pop_front() {
            first_level_count += 1;
            match task {
                AnalysisTask::DetectTrend { column } => {
                    // Regress on the period-aggregated series when a time
                    // column exists — regressing on raw row index conflates
                    // insertion order with time and inflates n.
                    let period_series: Option<(Vec<String>, Vec<f64>)> = if period_labels.is_empty()
                    {
                        None
                    } else {
                        cache.numeric.get(&column).map(|values| {
                            let stats = aggregate_by_period_cached(values, &period_labels);
                            (
                                stats.iter().map(|s| s.period_label.clone()).collect(),
                                stats.iter().map(|s| s.mean).collect(),
                            )
                        })
                    };
                    let trend_opt = match &period_series {
                        Some((_, means)) => detect_trend_in_series(&column, means),
                        None => detect_trend(df, &column)?,
                    };
                    if let Some(trend) = trend_opt {
                        if trend.p_value < self.p_threshold && trend.r_squared > 0.5 {
                            let direction_str = match trend.direction {
                                crate::analysis::tree::TrendDirection::Increasing => "increasing",
                                crate::analysis::tree::TrendDirection::Decreasing => "decreasing",
                            };
                            let description = format!(
                                "Significant {} trend in '{}': slope={:.4}, R²={:.3} (p={:.4})",
                                direction_str, column, trend.slope, trend.r_squared, trend.p_value
                            );
                            let data = match &period_series {
                                Some((labels, means)) => {
                                    Some(build_trend_series_data(labels, means, trend.slope))
                                },
                                None => cache
                                    .numeric
                                    .get(&column)
                                    .map(|v| build_trend_data(v, trend.slope)),
                            };
                            let analysis = AnalysisType::Trend {
                                column: trend.column,
                                direction: trend.direction,
                                slope: trend.slope,
                                r_squared: trend.r_squared,
                                p_value: trend.p_value,
                            };
                            let breakdown = scoring::score(&analysis, &self.scoring_ctx);
                            if !scoring::passes_floor(&breakdown, &self.scoring_ctx) {
                                continue;
                            }
                            let score = scoring::total(&breakdown);
                            tree.add_root_full(analysis, score, breakdown, description, data);
                        }
                    }
                },
                AnalysisTask::DetectSeasonality { column } => {
                    let Some(metric_values) = cache.numeric.get(&column) else {
                        continue;
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

                    if timestamps_sec.len() == metric_values.len() {
                        if let Some(result) =
                            detect_seasonality(&column, metric_values, &timestamps_sec)
                        {
                            let description = format!(
                                "Seasonality detected in '{}': {} pattern (r={:.3}, p={:.4})",
                                column, result.period_name, result.autocorrelation, result.p_value
                            );
                            let data = build_seasonality_data(metric_values, &period_labels);
                            let analysis = AnalysisType::Seasonality {
                                column: result.column,
                                period_name: result.period_name,
                                autocorrelation: result.autocorrelation,
                                p_value: result.p_value,
                            };
                            let breakdown = scoring::score(&analysis, &self.scoring_ctx);
                            if !scoring::passes_floor(&breakdown, &self.scoring_ctx) {
                                continue;
                            }
                            let score = scoring::total(&breakdown);
                            tree.add_root_full(analysis, score, breakdown, description, data);
                        }
                    }
                },
                AnalysisTask::DetectForecastDeviation { column } => {
                    let Some(metric_values) = cache.numeric.get(&column) else {
                        continue;
                    };
                    let mut period_sums: HashMap<String, (f64, usize)> = HashMap::new();
                    for (val, period_opt) in metric_values.iter().zip(period_labels.iter()) {
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

                    if let Some(deviation) = detect_forecast_deviation(&column, &period_values) {
                        if deviation.deviation_percent.abs() > 20.0 && deviation.p_value < 0.05 {
                            let description = format!(
                                "Forecast deviation in '{}' for {}: actual {:.2} vs expected {:.2} ({:.1}% deviation, p={:.4})",
                                column, deviation.period, deviation.actual, deviation.expected,
                                deviation.deviation_percent, deviation.p_value
                            );
                            let data = build_forecast_data(
                                &period_values,
                                deviation.expected,
                                deviation.actual,
                            );
                            let analysis = AnalysisType::ForecastDeviation {
                                column: deviation.column,
                                period: deviation.period,
                                actual: deviation.actual,
                                expected: deviation.expected,
                                deviation_percent: deviation.deviation_percent,
                                p_value: deviation.p_value,
                            };
                            let breakdown = scoring::score(&analysis, &self.scoring_ctx);
                            if !scoring::passes_floor(&breakdown, &self.scoring_ctx) {
                                continue;
                            }
                            let score = scoring::total(&breakdown);
                            tree.add_root_full(analysis, score, breakdown, description, data);
                        }
                    }
                },
                AnalysisTask::DetectConcentration {
                    column,
                    segment_col,
                } => {
                    let Some(target_values) = cache.numeric.get(&column) else {
                        continue;
                    };
                    let Some(segment_values) = cache.dimension.get(&segment_col) else {
                        continue;
                    };
                    if let Some(result) =
                        detect_concentration(&column, &segment_col, target_values, segment_values)
                    {
                        let description = format!(
                            "Concentration in '{column}' by '{segment_col}': HHI={:.2}, top {} = {:.0}%",
                            result.hhi, result.top_n, result.top_share
                        );
                        let data = Some(NodeData::Lorenz {
                            cumulative_share: result.lorenz_share,
                            cumulative_population: result.lorenz_population,
                            gini: result.gini,
                        });
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
                            continue;
                        }
                        let score = scoring::total(&breakdown);
                        tree.add_root_full(analysis, score, breakdown, description, data);
                    }
                },
                AnalysisTask::DetectDistributionShift { column } => {
                    let Some(values) = cache.numeric.get(&column) else {
                        continue;
                    };
                    let Some((prev, curr)) = latest_two_periods(&period_labels) else {
                        continue;
                    };
                    if let Some(result) =
                        detect_distribution_shift(&column, values, &period_labels, &prev, &curr)
                    {
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
                        let breakdown = scoring::score(&analysis, &self.scoring_ctx);
                        if !scoring::passes_floor(&breakdown, &self.scoring_ctx) {
                            continue;
                        }
                        let score = scoring::total(&breakdown);
                        tree.add_root_full(analysis, score, breakdown, description, data);
                    }
                },
                AnalysisTask::DetectMembershipChange { segment_col } => {
                    let Some(seg_values) = cache.dimension.get(&segment_col) else {
                        continue;
                    };
                    let Some((prev, curr)) = latest_two_periods(&period_labels) else {
                        continue;
                    };
                    if let Some(result) = detect_membership_change(
                        &segment_col,
                        seg_values,
                        &period_labels,
                        &prev,
                        &curr,
                    ) {
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
                        let breakdown = scoring::score(&analysis, &self.scoring_ctx);
                        if !scoring::passes_floor(&breakdown, &self.scoring_ctx) {
                            continue;
                        }
                        let score = scoring::total(&breakdown);
                        tree.add_root_full(analysis, score, breakdown, description, data);
                    }
                },
                AnalysisTask::FindOutlierClusters => {
                    let clusters = find_outlier_clusters(&cache, &period_labels, self.z_threshold);
                    for cluster in clusters {
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
                            &cache,
                            &period_labels,
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
                        let breakdown = scoring::score(&analysis, &self.scoring_ctx);
                        if !scoring::passes_floor(&breakdown, &self.scoring_ctx) {
                            continue;
                        }
                        let score = scoring::total(&breakdown);
                        tree.add_root_full(analysis, score, breakdown, description, data);
                    }
                },
                _ => {}, // Ignore other task types in trends
            }
        }

        // Dedup, then diversity-select the top roots
        dedup::dedup(&mut tree);
        crate::analysis::history::apply_novelty(&mut tree, &self.history, self.now_epoch);
        crate::analysis::select::select_top(&mut tree, self.select_top);

        let total_time = start_time.elapsed();
        debug.section("TRENDS COMPLETE");
        debug.kv("Root findings", &format!("{}", tree.roots.len()));
        debug.kv("Total nodes", &format!("{}", tree.nodes.len()));
        debug.kv(
            "Total time",
            &format!("{:.2}ms", total_time.as_secs_f64() * 1000.0),
        );
        debug.flush();

        Ok(AnalysisResult {
            tree,
            first_level_count,
            deeper_count: 0,
        })
    }

    /// Drivers report: composition and driver analysis (What is driving performance?)
    #[allow(clippy::unnecessary_wraps, clippy::unused_self)] // Stub - will use self and may error when implemented
    pub(super) fn run_drivers_impl(
        &self,
        _df: &DataFrame,
        _schema: &DataSchema,
        debug: &DebugLog,
    ) -> Result<AnalysisResult> {
        let tree = AnalysisTree::new();

        debug.section("DRIVERS REPORT");
        debug.log("Drivers analysis not yet implemented");
        debug.flush();

        // TODO: Implement composition analysis, pareto, etc.
        Ok(AnalysisResult {
            tree,
            first_level_count: 0,
            deeper_count: 0,
        })
    }
}

/// Return the latest two distinct period labels (previous, current) sorted by string order.
fn latest_two_periods(period_labels: &[Option<String>]) -> Option<(String, String)> {
    let mut periods: Vec<String> = period_labels
        .iter()
        .filter_map(Clone::clone)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    periods.sort();
    if periods.len() < 2 {
        return None;
    }
    let curr = periods.last()?.clone();
    let prev = periods.get(periods.len() - 2)?.clone();
    Some((prev, curr))
}
