use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use anyhow::Result;
use polars::prelude::*;

use crate::analysis::anomaly::detect_anomaly;
use crate::analysis::change_point::detect_change_point;
use crate::analysis::concentration::detect_concentration;
use crate::analysis::correlation::correlate;
use crate::analysis::dedup;
use crate::analysis::distribution_shift::detect_distribution_shift;
use crate::analysis::forecast::detect_forecast_deviation;
use crate::analysis::membership::detect_membership_change;
use crate::analysis::outlier_cluster::find_outlier_clusters;
use crate::analysis::period::{
    compare_periods_cached, extract_timestamps, find_anomalous_period_cached, get_period_labels,
};
use crate::analysis::scoring::{self, ScoringContext};
use crate::analysis::seasonality::detect_seasonality;
use crate::analysis::segment::{attribute_period_segments_cached, attribute_segment};
use crate::analysis::tree::{
    AnalysisResult, AnalysisTree, AnalysisType, NodeId, ReportType, ReviewCadence,
};
use crate::analysis::trend::detect_trend;
use crate::data::config::TimeGranularity;
use crate::data::schema::DataSchema;
use crate::debug::DebugLog;

/// Pre-extracted column data to avoid repeated DataFrame access
pub struct ColumnCache {
    pub numeric: HashMap<String, Vec<f64>>,
    pub dimension: HashMap<String, Vec<String>>,
}

impl ColumnCache {
    pub fn new(df: &DataFrame, schema: &DataSchema) -> Result<Self> {
        let mut numeric = HashMap::new();
        let mut dimension = HashMap::new();

        // Extract all numeric columns (KPIs + metrics)
        for col in &schema.measure_columns {
            if let Ok(series) = df.column(col) {
                let values: Vec<f64> = series
                    .cast(&DataType::Float64)?
                    .f64()?
                    .into_iter()
                    .flatten()
                    .collect();
                numeric.insert(col.clone(), values);
            }
        }

        // Extract all dimension columns
        for col in &schema.dimension_columns {
            if let Ok(series) = df.column(col) {
                let values: Vec<String> = series
                    .cast(&DataType::String)?
                    .str()?
                    .into_iter()
                    .map(|v| v.unwrap_or("").to_string())
                    .collect();
                dimension.insert(col.clone(), values);
            }
        }

        Ok(Self { numeric, dimension })
    }

    /// Check if a dimension has meaningful data in a specific period
    /// Returns (rows_in_period, unique_values_in_period)
    pub fn dimension_coverage_in_period(
        &self,
        segment_col: &str,
        period: &str,
        period_labels: &[Option<String>],
    ) -> (usize, usize) {
        let Some(segment_values) = self.dimension.get(segment_col) else {
            return (0, 0);
        };

        let mut values_in_period: std::collections::HashSet<&str> =
            std::collections::HashSet::new();
        let mut count = 0;

        for (seg_val, period_label) in segment_values.iter().zip(period_labels.iter()) {
            if let Some(p) = period_label {
                if p == period {
                    count += 1;
                    if !seg_val.is_empty() {
                        values_in_period.insert(seg_val);
                    }
                }
            }
        }

        (count, values_in_period.len())
    }
}

pub struct AnalysisEngine {
    z_threshold: f64,
    p_threshold: f64,
    max_depth: usize,
    scoring_ctx: ScoringContext,
}

/// Score an analysis and add as child using calibrated scoring
fn add_child_scored(
    tree: &mut AnalysisTree,
    parent_id: NodeId,
    analysis: AnalysisType,
    description: String,
    ctx: &ScoringContext,
) -> Option<NodeId> {
    let breakdown = scoring::score(&analysis, ctx);
    if !scoring::passes_floor(&breakdown, ctx) {
        return None;
    }
    let total = scoring::total(&breakdown);
    Some(tree.add_child_full(parent_id, analysis, total, breakdown, description, None))
}

enum AnalysisTask {
    DetectAnomalies {
        column: String,
    },
    DetectTrend {
        column: String,
    },
    ComparePeriods {
        column: String,
    },
    FindPeriodAnomaly {
        column: String,
    },
    DetectSeasonality {
        column: String,
    },
    DetectForecastDeviation {
        column: String,
    },
    FindOutlierClusters,
    DetectConcentration {
        column: String,
        segment_col: String,
    },
    DetectDistributionShift {
        column: String,
    },
    DetectMembershipChange {
        segment_col: String,
    },
    DetectChangePoint {
        column: String,
    },
    AttributeSegment {
        parent_id: NodeId,
        target_col: String,
        segment_col: String,
        depth: usize,
    },
    AttributePeriodSegment {
        parent_id: NodeId,
        target_col: String,
        segment_col: String,
        period: String,
        depth: usize,
    },
    SearchCorrelations {
        parent_id: NodeId,
        target_col: String,
        depth: usize,
    },
}

impl AnalysisEngine {
    pub fn new(z_threshold: f64, p_threshold: f64, max_depth: usize) -> Self {
        Self {
            z_threshold,
            p_threshold,
            max_depth,
            scoring_ctx: ScoringContext::new(),
        }
    }

    pub fn with_scoring_ctx(mut self, ctx: ScoringContext) -> Self {
        self.scoring_ctx = ctx;
        self
    }

    /// Run review report: anomaly detection with attribution
    pub fn run_review(&self, df: &DataFrame, schema: &DataSchema) -> Result<AnalysisResult> {
        self.run_review_with_cadence(df, schema, ReviewCadence::Daily, &DebugLog::disabled())
    }

    /// Run review report with specific cadence
    pub fn run_review_with_cadence(
        &self,
        df: &DataFrame,
        schema: &DataSchema,
        cadence: ReviewCadence,
        debug: &DebugLog,
    ) -> Result<AnalysisResult> {
        self.run_review_cadence_impl(df, schema, cadence, debug)
    }

    /// Run trends report: time-based patterns and forecasting
    pub fn run_trends(&self, df: &DataFrame, schema: &DataSchema) -> Result<AnalysisResult> {
        self.run_report(df, schema, ReportType::Trends, &DebugLog::disabled())
    }

    /// Run drivers report: composition and driver analysis
    pub fn run_drivers(&self, df: &DataFrame, schema: &DataSchema) -> Result<AnalysisResult> {
        self.run_report(df, schema, ReportType::Drivers, &DebugLog::disabled())
    }

    /// Run a specific report type with debug logging
    pub fn run_report(
        &self,
        df: &DataFrame,
        schema: &DataSchema,
        report_type: ReportType,
        debug: &DebugLog,
    ) -> Result<AnalysisResult> {
        match report_type {
            ReportType::Review => self.run_review_impl(df, schema, debug),
            ReportType::Trends => self.run_trends_impl(df, schema, debug),
            ReportType::Drivers => self.run_drivers_impl(df, schema, debug),
        }
    }

    /// Convert ReviewCadence to TimeGranularity
    fn cadence_to_granularity(cadence: ReviewCadence) -> TimeGranularity {
        match cadence {
            ReviewCadence::Daily => TimeGranularity::Day,
            ReviewCadence::Weekly => TimeGranularity::Week,
            ReviewCadence::Monthly => TimeGranularity::Month,
        }
    }

    /// Review report with cadence awareness
    fn run_review_cadence_impl(
        &self,
        df: &DataFrame,
        schema: &DataSchema,
        cadence: ReviewCadence,
        debug: &DebugLog,
    ) -> Result<AnalysisResult> {
        let start_time = Instant::now();
        let mut queue: VecDeque<AnalysisTask> = VecDeque::new();
        let mut tree = AnalysisTree::new();
        let mut first_level_count: usize = 0;
        let mut deeper_count: usize = 0;

        debug.section(&format!("{} REVIEW", cadence.title().to_uppercase()));

        let cache = ColumnCache::new(df, schema)?;

        // Get period labels at the cadence granularity
        let granularity = Self::cadence_to_granularity(cadence);
        let period_labels: Vec<Option<String>> = if let Some(time_col) = &schema.time_column {
            get_period_labels(df.column(time_col)?, granularity)?
        } else {
            // No time column - fall back to simple anomaly detection
            return self.run_review_impl(df, schema, debug);
        };

        // Find unique periods and get the latest two
        let mut unique_periods: Vec<String> = period_labels
            .iter()
            .filter_map(Clone::clone)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        unique_periods.sort();

        if unique_periods.len() < 2 {
            debug.log("Not enough periods for comparison");
            return Ok(AnalysisResult {
                tree,
                first_level_count: 0,
                deeper_count: 0,
            });
        }

        // Safe: we checked len >= 2 above
        let Some(current_period) = unique_periods.last().cloned() else {
            return Ok(AnalysisResult {
                tree,
                first_level_count: 0,
                deeper_count: 0,
            });
        };
        let Some(previous_period) = unique_periods.get(unique_periods.len() - 2).cloned() else {
            return Ok(AnalysisResult {
                tree,
                first_level_count: 0,
                deeper_count: 0,
            });
        };

        debug.kv("Current period", &current_period);
        debug.kv("Previous period", &previous_period);

        // Compare each KPI between current and previous period
        for col in &schema.measure_columns {
            let Some(metric_values) = cache.numeric.get(col) else {
                continue;
            };

            // Calculate stats for current and previous periods
            let mut current_values: Vec<f64> = Vec::new();
            let mut previous_values: Vec<f64> = Vec::new();

            for (val, period_opt) in metric_values.iter().zip(period_labels.iter()) {
                if let Some(period) = period_opt {
                    if period == &current_period {
                        current_values.push(*val);
                    } else if period == &previous_period {
                        previous_values.push(*val);
                    }
                }
            }

            if current_values.is_empty() || previous_values.is_empty() {
                continue;
            }

            let current_mean = crate::stats::significance::mean(&current_values);
            let previous_mean = crate::stats::significance::mean(&previous_values);
            let current_std = crate::stats::significance::std_dev(&current_values);
            let previous_std = crate::stats::significance::std_dev(&previous_values);

            if previous_mean == 0.0 {
                continue;
            }

            first_level_count += 1;
            let change_percent = ((current_mean - previous_mean) / previous_mean) * 100.0;
            let p_value = crate::stats::significance::p_value_welch_t_test(
                current_mean,
                current_std,
                current_values.len(),
                previous_mean,
                previous_std,
                previous_values.len(),
            );

            // Report if statistically significant OR large change
            let is_significant = p_value < self.p_threshold && change_percent.abs() > 10.0;
            let is_large_change = change_percent.abs() > 50.0;

            if is_significant || is_large_change {
                let direction = if change_percent > 0.0 { "up" } else { "down" };
                let description = format!(
                    "'{}' is {} {:.1}% in {} vs {} (p={:.4})",
                    col,
                    direction,
                    change_percent.abs(),
                    current_period,
                    previous_period,
                    p_value
                );

                let analysis = AnalysisType::PeriodComparison {
                    column: col.clone(),
                    current_period: current_period.clone(),
                    previous_period: previous_period.clone(),
                    current_value: current_mean,
                    previous_value: previous_mean,
                    change_percent,
                    p_value,
                };
                let data = build_period_comparison_data(
                    metric_values,
                    &period_labels,
                    &current_period,
                    &previous_period,
                );
                let breakdown = scoring::score(&analysis, &self.scoring_ctx);
                if !scoring::passes_floor(&breakdown, &self.scoring_ctx) {
                    continue;
                }
                let score = scoring::total(&breakdown);
                let node_id = tree.add_root_full(analysis, score, breakdown, description, data);

                // Attribute to segments
                for seg_col in &schema.dimension_columns {
                    queue.push_back(AnalysisTask::AttributePeriodSegment {
                        parent_id: node_id,
                        target_col: col.clone(),
                        segment_col: seg_col.clone(),
                        period: current_period.clone(),
                        depth: 1,
                    });
                }
            }
        }

        // Process attribution queue
        while let Some(task) = queue.pop_front() {
            if let AnalysisTask::AttributePeriodSegment {
                parent_id,
                target_col,
                segment_col,
                period,
                depth,
            } = task
            {
                if depth >= self.max_depth {
                    continue;
                }

                deeper_count += 1;

                let Some(target_values) = cache.numeric.get(&target_col) else {
                    continue;
                };
                let Some(segment_values) = cache.dimension.get(&segment_col) else {
                    continue;
                };

                let segments = attribute_period_segments_cached(
                    &target_col,
                    &segment_col,
                    target_values,
                    segment_values,
                    &period,
                    &period_labels,
                );

                for attr in segments {
                    if attr.p_value < self.p_threshold || attr.contribution.abs() > 0.0 {
                        let description =
                            format!(
                            "In {}: '{}' = '{}' was {:.0}% {}, contributing {:.0}% of total change",
                            period,
                            segment_col,
                            attr.segment_value,
                            attr.change_percent.abs(),
                            if attr.change_percent > 0.0 { "higher" } else { "lower" },
                            attr.contribution_pct.abs()
                        );
                        let analysis = AnalysisType::Segment {
                            target_column: attr.target_column.clone(),
                            segment_column: attr.segment_column.clone(),
                            segment_value: attr.segment_value.clone(),
                            contribution: attr.contribution,
                            change_percent: attr.change_percent,
                            contribution_pct: attr.contribution_pct,
                            p_value: attr.p_value,
                        };
                        if let Some(child_id) = add_child_scored(
                            &mut tree,
                            parent_id,
                            analysis,
                            description,
                            &self.scoring_ctx,
                        ) {
                            // Extend the filter chain for this segment finding
                            let parent_chain = tree.nodes[parent_id.0].filter_chain.clone();
                            let mut chain = parent_chain;
                            chain.push(crate::analysis::tree::FilterStep {
                                column: attr.segment_column.clone(),
                                op: "eq".to_string(),
                                value: attr.segment_value.clone(),
                            });
                            tree.set_filter_chain(child_id, chain);
                        }
                    }
                }
            }
        }

        // Dedup before reporting
        dedup::dedup(&mut tree);

        let total_time = start_time.elapsed();
        debug.section("REVIEW COMPLETE");
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
            deeper_count,
        })
    }

    /// Review report: anomaly detection with attribution (How are we doing? What happened? Why?)
    /// Fallback for when no time column exists
    fn run_review_impl(
        &self,
        df: &DataFrame,
        schema: &DataSchema,
        debug: &DebugLog,
    ) -> Result<AnalysisResult> {
        let start_time = Instant::now();
        let mut queue: VecDeque<AnalysisTask> = VecDeque::new();
        let mut tree = AnalysisTree::new();
        let mut first_level_count: usize = 0;
        let mut deeper_count: usize = 0;

        debug.section("REVIEW REPORT");
        debug.subsection("Configuration");
        debug.kv("z_threshold", &format!("{}", self.z_threshold));
        debug.kv("p_threshold", &format!("{}", self.p_threshold));

        let cache = ColumnCache::new(df, schema)?;
        let setup_time = start_time.elapsed();

        // Queue anomaly detection for KPIs and metrics
        for col in &schema.measure_columns {
            queue.push_back(AnalysisTask::DetectAnomalies {
                column: col.clone(),
            });
        }

        while let Some(task) = queue.pop_front() {
            match task {
                AnalysisTask::DetectAnomalies { column } => {
                    first_level_count += 1;
                    if let Some(anomaly) = detect_anomaly(df, &column)? {
                        if anomaly.z_score.abs() > self.z_threshold {
                            let description = format!(
                                "Anomaly detected in '{}': latest value {:.2} is {:.1} std devs {} the mean ({:.2})",
                                column, anomaly.value, anomaly.z_score.abs(),
                                if anomaly.z_score > 0.0 { "above" } else { "below" }, anomaly.mean
                            );

                            let analysis = AnalysisType::Anomaly {
                                column: anomaly.column.clone(),
                                value: anomaly.value,
                                mean: anomaly.mean,
                                std_dev: anomaly.std_dev,
                                z_score: anomaly.z_score,
                            };
                            let series_data = cache.numeric.get(&column).map(|values| {
                                build_anomaly_series(values, anomaly.mean, anomaly.std_dev)
                            });
                            let breakdown = scoring::score(&analysis, &self.scoring_ctx);
                            if !scoring::passes_floor(&breakdown, &self.scoring_ctx) {
                                continue;
                            }
                            let score = scoring::total(&breakdown);
                            let node_id = tree.add_root_full(
                                analysis,
                                score,
                                breakdown,
                                description,
                                series_data,
                            );

                            // Spawn attribution tasks
                            for cat_col in &schema.dimension_columns {
                                queue.push_back(AnalysisTask::AttributeSegment {
                                    parent_id: node_id,
                                    target_col: column.clone(),
                                    segment_col: cat_col.clone(),
                                    depth: 1,
                                });
                            }
                        }
                    }
                },
                AnalysisTask::AttributeSegment {
                    parent_id,
                    target_col,
                    segment_col,
                    depth,
                } => {
                    if depth >= self.max_depth {
                        continue;
                    }
                    deeper_count += 1;
                    if let Some(attr) = attribute_segment(df, &target_col, &segment_col)? {
                        if attr.p_value < self.p_threshold {
                            let description = format!(
                                "Segment '{}' = '{}' contributes {:.2} to '{}' (p={:.4})",
                                segment_col,
                                attr.segment_value,
                                attr.contribution,
                                target_col,
                                attr.p_value
                            );
                            let analysis = AnalysisType::Segment {
                                target_column: attr.target_column.clone(),
                                segment_column: attr.segment_column.clone(),
                                segment_value: attr.segment_value.clone(),
                                contribution: attr.contribution,
                                change_percent: attr.change_percent,
                                contribution_pct: attr.contribution_pct,
                                p_value: attr.p_value,
                            };
                            if let Some(node_id) = add_child_scored(
                                &mut tree,
                                parent_id,
                                analysis,
                                description,
                                &self.scoring_ctx,
                            ) {
                                let parent_chain = tree.nodes[parent_id.0].filter_chain.clone();
                                let mut chain = parent_chain;
                                chain.push(crate::analysis::tree::FilterStep {
                                    column: attr.segment_column.clone(),
                                    op: "eq".to_string(),
                                    value: attr.segment_value.clone(),
                                });
                                tree.set_filter_chain(node_id, chain);
                                queue.push_back(AnalysisTask::SearchCorrelations {
                                    parent_id: node_id,
                                    target_col: target_col.clone(),
                                    depth: depth + 1,
                                });
                            }
                        }
                    }
                },
                AnalysisTask::SearchCorrelations {
                    parent_id,
                    target_col,
                    depth,
                } => {
                    if depth >= self.max_depth {
                        continue;
                    }
                    deeper_count += 1;
                    for other_col in &schema.analyzable_columns() {
                        if other_col != &target_col {
                            if let Some(corr) = correlate(df, &target_col, other_col)? {
                                if corr.p_value < self.p_threshold && corr.r_value.abs() > 0.5 {
                                    let description = format!(
                                        "Correlation between '{}' and '{}': r={:.3} (p={:.4})",
                                        target_col, other_col, corr.r_value, corr.p_value
                                    );
                                    let xs = cache.numeric.get(&target_col).cloned();
                                    let ys = cache.numeric.get(other_col).cloned();
                                    let scatter_data = match (xs, ys) {
                                        (Some(x), Some(y)) => Some(build_scatter_data(
                                            &x,
                                            &y,
                                            &target_col,
                                            other_col,
                                            corr.r_value,
                                        )),
                                        _ => None,
                                    };
                                    let analysis = AnalysisType::Correlation {
                                        column_a: corr.column_a,
                                        column_b: corr.column_b,
                                        r_value: corr.r_value,
                                        p_value: corr.p_value,
                                    };
                                    let breakdown = scoring::score(&analysis, &self.scoring_ctx);
                                    if !scoring::passes_floor(&breakdown, &self.scoring_ctx) {
                                        continue;
                                    }
                                    let score = scoring::total(&breakdown);
                                    tree.add_child_full(
                                        parent_id,
                                        analysis,
                                        score,
                                        breakdown,
                                        description,
                                        scatter_data,
                                    );
                                }
                            }
                        }
                    }
                },
                _ => {}, // Ignore other task types in review
            }
        }

        // Dedup before reporting
        dedup::dedup(&mut tree);

        let total_time = start_time.elapsed();
        debug.section("REVIEW COMPLETE");
        debug.kv("Root findings", &format!("{}", tree.roots.len()));
        debug.kv("Total nodes", &format!("{}", tree.nodes.len()));
        debug.kv(
            "Total time",
            &format!("{:.2}ms", total_time.as_secs_f64() * 1000.0),
        );
        debug.flush();

        // Suppress unused variable warning
        let _ = setup_time;

        Ok(AnalysisResult {
            tree,
            first_level_count,
            deeper_count,
        })
    }

    /// Trends report: time-based patterns and forecasting (What's changing over time?)
    fn run_trends_impl(
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

        // Queue trend analyses for KPIs and metrics
        for col in &schema.measure_columns {
            queue.push_back(AnalysisTask::DetectTrend {
                column: col.clone(),
            });
            if schema.time_column.is_some() {
                queue.push_back(AnalysisTask::ComparePeriods {
                    column: col.clone(),
                });
                queue.push_back(AnalysisTask::FindPeriodAnomaly {
                    column: col.clone(),
                });
                queue.push_back(AnalysisTask::DetectSeasonality {
                    column: col.clone(),
                });
                queue.push_back(AnalysisTask::DetectForecastDeviation {
                    column: col.clone(),
                });
                queue.push_back(AnalysisTask::DetectDistributionShift {
                    column: col.clone(),
                });
                queue.push_back(AnalysisTask::DetectChangePoint {
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
                    if let Some(trend) = detect_trend(df, &column)? {
                        if trend.p_value < self.p_threshold && trend.r_squared > 0.5 {
                            let direction_str = match trend.direction {
                                crate::analysis::tree::TrendDirection::Increasing => "increasing",
                                crate::analysis::tree::TrendDirection::Decreasing => "decreasing",
                            };
                            let description = format!(
                                "Significant {} trend in '{}': slope={:.4}, R²={:.3} (p={:.4})",
                                direction_str, column, trend.slope, trend.r_squared, trend.p_value
                            );
                            let data = cache
                                .numeric
                                .get(&column)
                                .map(|v| build_trend_data(v, trend.slope));
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
                AnalysisTask::ComparePeriods { column } => {
                    let Some(metric_values) = cache.numeric.get(&column) else {
                        continue;
                    };
                    if let Some(comparison) =
                        compare_periods_cached(&column, metric_values, &period_labels)
                    {
                        let is_significant = comparison.p_value < self.p_threshold
                            && comparison.change_percent.abs() > 10.0;
                        let is_large_change = comparison.change_percent.abs() > 50.0;
                        if is_significant || is_large_change {
                            let direction = if comparison.change_percent > 0.0 {
                                "up"
                            } else {
                                "down"
                            };
                            let description = format!(
                                "'{}' is {} {:.1}% in {} vs {} (p={:.4})",
                                column,
                                direction,
                                comparison.change_percent.abs(),
                                comparison.current_period,
                                comparison.previous_period,
                                comparison.p_value
                            );
                            let data = build_period_comparison_data(
                                metric_values,
                                &period_labels,
                                &comparison.current_period,
                                &comparison.previous_period,
                            );
                            let analysis = AnalysisType::PeriodComparison {
                                column: comparison.column,
                                current_period: comparison.current_period,
                                previous_period: comparison.previous_period,
                                current_value: comparison.current_value,
                                previous_value: comparison.previous_value,
                                change_percent: comparison.change_percent,
                                p_value: comparison.p_value,
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
                AnalysisTask::FindPeriodAnomaly { column } => {
                    let Some(metric_values) = cache.numeric.get(&column) else {
                        continue;
                    };
                    if let Some(anomaly) =
                        find_anomalous_period_cached(&column, metric_values, &period_labels)
                    {
                        let is_significant = anomaly.p_value < self.p_threshold
                            && anomaly.change_percent.abs() > 20.0;
                        let is_large_change = anomaly.change_percent.abs() > 50.0;
                        if is_significant || is_large_change {
                            let direction = if anomaly.change_percent > 0.0 {
                                "above"
                            } else {
                                "below"
                            };
                            let description = format!(
                                "'{}' in {} was {:.1}% {} other periods (p={:.4})",
                                column,
                                anomaly.current_period,
                                anomaly.change_percent.abs(),
                                direction,
                                anomaly.p_value
                            );
                            let data = build_period_anomaly_data(
                                metric_values,
                                &period_labels,
                                &anomaly.current_period,
                            );
                            let analysis = AnalysisType::PeriodAnomaly {
                                column: anomaly.column,
                                period: anomaly.current_period,
                                period_value: anomaly.current_value,
                                other_periods_mean: anomaly.previous_value,
                                change_percent: anomaly.change_percent,
                                p_value: anomaly.p_value,
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
                        };
                        let breakdown = scoring::score(&analysis, &self.scoring_ctx);
                        if !scoring::passes_floor(&breakdown, &self.scoring_ctx) {
                            continue;
                        }
                        let score = scoring::total(&breakdown);
                        tree.add_root_full(analysis, score, breakdown, description, data);
                    }
                },
                AnalysisTask::DetectChangePoint { column } => {
                    let Some(values) = cache.numeric.get(&column) else {
                        continue;
                    };
                    let mut by_period: HashMap<String, (f64, usize)> = HashMap::new();
                    for (val, p) in values.iter().zip(period_labels.iter()) {
                        if let Some(label) = p {
                            let entry = by_period.entry(label.clone()).or_insert((0.0, 0));
                            entry.0 += val;
                            entry.1 += 1;
                        }
                    }
                    let mut period_means: Vec<(String, f64)> = by_period
                        .into_iter()
                        .map(|(p, (s, n))| (p, s / n as f64))
                        .collect();
                    period_means.sort_by(|a, b| a.0.cmp(&b.0));
                    if let Some(result) = detect_change_point(&column, &period_means) {
                        let description = format!(
                            "Change-point in '{column}' at {}: {:.2} → {:.2} (p={:.4})",
                            result.period, result.before_mean, result.after_mean, result.p_value
                        );
                        // Build SeriesWithFit-like data: full series + step-fit overlay
                        let labels: Vec<String> =
                            period_means.iter().map(|(p, _)| p.clone()).collect();
                        let vals: Vec<f64> = period_means.iter().map(|(_, v)| *v).collect();
                        let cp_idx = labels
                            .iter()
                            .position(|p| p == &result.period)
                            .unwrap_or(labels.len() / 2);
                        let fit: Vec<f64> = vals
                            .iter()
                            .enumerate()
                            .map(|(i, _)| {
                                if i <= cp_idx {
                                    result.before_mean
                                } else {
                                    result.after_mean
                                }
                            })
                            .collect();
                        let data = Some(NodeData::SeriesWithFit {
                            labels,
                            values: vals,
                            fit,
                        });
                        let analysis = AnalysisType::ChangePoint {
                            column: result.column,
                            period: result.period,
                            before_mean: result.before_mean,
                            after_mean: result.after_mean,
                            cusum: result.cusum,
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

        // Dedup before reporting
        dedup::dedup(&mut tree);

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
    fn run_drivers_impl(
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

// ─── Helpers ──────────────────────────────────────────────────────────────────

use crate::analysis::tree::{NamedSeries, NodeData};

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

// ─── Data payload builders ────────────────────────────────────────────────────

/// LTTB-ish downsample to ≤MAX_POINTS (simple stride sampling — good enough for
/// glance-charts; replace with LTTB if precision matters later)
const MAX_POINTS: usize = 200;

fn downsample(values: &[f64]) -> (Vec<String>, Vec<f64>) {
    let n = values.len();
    if n <= MAX_POINTS {
        let labels = (0..n).map(|i| i.to_string()).collect();
        return (labels, values.to_vec());
    }
    let stride = n / MAX_POINTS;
    let mut out = Vec::with_capacity(MAX_POINTS);
    let mut labels = Vec::with_capacity(MAX_POINTS);
    let mut i = 0;
    while i < n {
        out.push(values[i]);
        labels.push(i.to_string());
        i += stride.max(1);
    }
    (labels, out)
}

fn build_anomaly_series(values: &[f64], mean: f64, std_dev: f64) -> NodeData {
    let (labels, vals) = downsample(values);
    let band_low = vec![2.0_f64.mul_add(-std_dev, mean); vals.len()];
    let band_high = vec![2.0_f64.mul_add(std_dev, mean); vals.len()];
    let marker = vals.len().saturating_sub(1);
    NodeData::Series {
        labels,
        values: vals,
        band_low: Some(band_low),
        band_high: Some(band_high),
        marker_index: Some(marker),
    }
}

fn build_trend_data(values: &[f64], slope: f64) -> NodeData {
    let (labels, vals) = downsample(values);
    let n = vals.len();
    if n == 0 {
        return NodeData::SeriesWithFit {
            labels,
            values: vals,
            fit: Vec::new(),
        };
    }
    // Recompute simple linear fit on the (possibly downsampled) series
    let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let mean_y: f64 = vals.iter().sum::<f64>() / n as f64;
    let mean_x = (n as f64 - 1.0) / 2.0;
    let intercept = slope.mul_add(-mean_x, mean_y);
    let fit: Vec<f64> = xs.iter().map(|x| slope.mul_add(*x, intercept)).collect();
    NodeData::SeriesWithFit {
        labels,
        values: vals,
        fit,
    }
}

fn build_period_comparison_data(
    values: &[f64],
    period_labels: &[Option<String>],
    current_period: &str,
    previous_period: &str,
) -> Option<NodeData> {
    let mut current_sum = 0.0;
    let mut current_n = 0_i32;
    let mut previous_sum = 0.0;
    let mut previous_n = 0_i32;
    for (val, period) in values.iter().zip(period_labels.iter()) {
        match period {
            Some(p) if p == current_period => {
                current_sum += val;
                current_n += 1;
            },
            Some(p) if p == previous_period => {
                previous_sum += val;
                previous_n += 1;
            },
            _ => {},
        }
    }
    if current_n == 0 || previous_n == 0 {
        return None;
    }
    Some(NodeData::PairedBars {
        labels: vec![previous_period.to_string(), current_period.to_string()],
        previous: vec![previous_sum / f64::from(previous_n)],
        current: vec![current_sum / f64::from(current_n)],
    })
}

fn build_period_anomaly_data(
    values: &[f64],
    period_labels: &[Option<String>],
    anomalous_period: &str,
) -> Option<NodeData> {
    let mut by_period: HashMap<String, (f64, usize)> = HashMap::new();
    for (val, period) in values.iter().zip(period_labels.iter()) {
        if let Some(p) = period {
            let entry = by_period.entry(p.clone()).or_insert((0.0, 0));
            entry.0 += val;
            entry.1 += 1;
        }
    }
    if by_period.is_empty() {
        return None;
    }
    let mut sorted: Vec<(String, f64)> = by_period
        .into_iter()
        .map(|(p, (s, n))| (p, s / n as f64))
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let labels: Vec<String> = sorted.iter().map(|(p, _)| p.clone()).collect();
    let vals: Vec<f64> = sorted.iter().map(|(_, v)| *v).collect();
    let marker = labels.iter().position(|p| p == anomalous_period);
    Some(NodeData::Series {
        labels,
        values: vals,
        band_low: None,
        band_high: None,
        marker_index: marker,
    })
}

fn build_seasonality_data(values: &[f64], period_labels: &[Option<String>]) -> Option<NodeData> {
    let mut by_period: HashMap<String, (f64, usize)> = HashMap::new();
    for (val, period) in values.iter().zip(period_labels.iter()) {
        if let Some(p) = period {
            let entry = by_period.entry(p.clone()).or_insert((0.0, 0));
            entry.0 += val;
            entry.1 += 1;
        }
    }
    if by_period.is_empty() {
        return None;
    }
    let mut sorted: Vec<(String, f64)> = by_period
        .into_iter()
        .map(|(p, (s, n))| (p, s / n as f64))
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let labels: Vec<String> = sorted.iter().map(|(p, _)| p.clone()).collect();
    let vals: Vec<f64> = sorted.iter().map(|(_, v)| *v).collect();
    Some(NodeData::Series {
        labels,
        values: vals,
        band_low: None,
        band_high: None,
        marker_index: None,
    })
}

fn build_forecast_data(
    period_values: &[(String, f64)],
    expected: f64,
    actual: f64,
) -> Option<NodeData> {
    if period_values.is_empty() {
        return None;
    }
    let labels: Vec<String> = period_values.iter().map(|(p, _)| p.clone()).collect();
    let history: Vec<f64> = period_values.iter().map(|(_, v)| *v).collect();
    // Simple PI: ±20% of expected — replace with proper PI from forecast detector
    let pi = expected.abs() * 0.2;
    Some(NodeData::Forecast {
        labels,
        history,
        expected,
        actual,
        pi_low: expected - pi,
        pi_high: expected + pi,
    })
}

fn build_outlier_cluster_data(
    cache: &ColumnCache,
    period_labels: &[Option<String>],
    target_period: &str,
    columns: &[String],
) -> Option<NodeData> {
    let mut series_list: Vec<NamedSeries> = Vec::new();
    let mut common_labels: Vec<String> = Vec::new();
    let mut marker_idx: Option<usize> = None;

    for col in columns {
        let Some(values) = cache.numeric.get(col) else {
            continue;
        };
        let mut by_period: HashMap<String, (f64, usize)> = HashMap::new();
        for (val, period) in values.iter().zip(period_labels.iter()) {
            if let Some(p) = period {
                let entry = by_period.entry(p.clone()).or_insert((0.0, 0));
                entry.0 += val;
                entry.1 += 1;
            }
        }
        if by_period.is_empty() {
            continue;
        }
        let mut sorted: Vec<(String, f64)> = by_period
            .into_iter()
            .map(|(p, (s, n))| (p, s / n as f64))
            .collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        if common_labels.is_empty() {
            common_labels = sorted.iter().map(|(p, _)| p.clone()).collect();
            marker_idx = common_labels.iter().position(|p| p == target_period);
        }
        let vals: Vec<f64> = sorted.iter().map(|(_, v)| *v).collect();
        series_list.push(NamedSeries {
            name: col.clone(),
            values: vals,
        });
    }

    if series_list.is_empty() || common_labels.is_empty() {
        return None;
    }

    Some(NodeData::Multi {
        labels: common_labels,
        series: series_list,
        marker_index: marker_idx.unwrap_or(0),
    })
}

fn build_scatter_data(xs: &[f64], ys: &[f64], x_label: &str, y_label: &str, r: f64) -> NodeData {
    // Downsample if too large
    let take = xs.len().min(ys.len()).min(MAX_POINTS);
    let stride = (xs.len() / take).max(1);
    let x: Vec<f64> = xs.iter().step_by(stride).take(take).copied().collect();
    let y: Vec<f64> = ys.iter().step_by(stride).take(take).copied().collect();
    // Compute simple OLS fit
    let n = x.len() as f64;
    let mean_x = x.iter().sum::<f64>() / n;
    let mean_y = y.iter().sum::<f64>() / n;
    let mut num = 0.0;
    let mut den = 0.0;
    for (xi, yi) in x.iter().zip(y.iter()) {
        num += (xi - mean_x) * (yi - mean_y);
        den += (xi - mean_x).powi(2);
    }
    let (slope, intercept) = if (den - 0.0).abs() > f64::EPSILON {
        let s = num / den;
        (Some(s), Some(s.mul_add(-mean_x, mean_y)))
    } else {
        (None, None)
    };
    let _ = r; // r already part of analysis
    NodeData::Scatter {
        x,
        y,
        x_label: x_label.to_string(),
        y_label: y_label.to_string(),
        fit_slope: slope,
        fit_intercept: intercept,
    }
}
