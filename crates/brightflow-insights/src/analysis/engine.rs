use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use anyhow::Result;
use polars::prelude::*;

use crate::analysis::anomaly::detect_anomaly;
use crate::analysis::correlation::correlate;
use crate::analysis::forecast::detect_forecast_deviation;
use crate::analysis::outlier_cluster::find_outlier_clusters;
use crate::analysis::period::{
    compare_periods_cached, extract_timestamps, find_anomalous_period_cached, get_period_labels,
};
use crate::analysis::seasonality::detect_seasonality;
use crate::analysis::segment::{attribute_period_segments_cached, attribute_segment};
use crate::analysis::tree::{AnalysisTree, AnalysisType, NodeId, ReportType, ReviewCadence};
use crate::analysis::trend::detect_trend;
use crate::data::config::TimeGranularity;
use crate::data::schema::DataSchema;
use crate::debug::{AnalysisOutcome, DebugLog};

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
        for col in schema
            .kpi_columns
            .iter()
            .chain(schema.metric_columns.iter())
        {
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
        }
    }

    /// Run review report: anomaly detection with attribution
    pub fn run_review(&self, df: &DataFrame, schema: &DataSchema) -> Result<AnalysisTree> {
        self.run_review_with_cadence(df, schema, ReviewCadence::Daily, &DebugLog::disabled())
    }

    /// Run review report with specific cadence
    pub fn run_review_with_cadence(
        &self,
        df: &DataFrame,
        schema: &DataSchema,
        cadence: ReviewCadence,
        debug: &DebugLog,
    ) -> Result<AnalysisTree> {
        self.run_review_cadence_impl(df, schema, cadence, debug)
    }

    /// Run trends report: time-based patterns and forecasting
    pub fn run_trends(&self, df: &DataFrame, schema: &DataSchema) -> Result<AnalysisTree> {
        self.run_report(df, schema, ReportType::Trends, &DebugLog::disabled())
    }

    /// Run drivers report: composition and driver analysis
    pub fn run_drivers(&self, df: &DataFrame, schema: &DataSchema) -> Result<AnalysisTree> {
        self.run_report(df, schema, ReportType::Drivers, &DebugLog::disabled())
    }

    /// Run a specific report type with debug logging
    pub fn run_report(
        &self,
        df: &DataFrame,
        schema: &DataSchema,
        report_type: ReportType,
        debug: &DebugLog,
    ) -> Result<AnalysisTree> {
        match report_type {
            ReportType::Review => self.run_review_impl(df, schema, debug),
            ReportType::Trends => self.run_trends_impl(df, schema, debug),
            ReportType::Drivers => self.run_drivers_impl(df, schema, debug),
        }
    }

    #[allow(dead_code)]
    pub fn run(&self, df: &DataFrame, schema: &DataSchema) -> Result<AnalysisTree> {
        self.run_with_debug(df, schema, &DebugLog::disabled())
    }

    #[allow(dead_code)]
    pub fn run_with_debug(
        &self,
        df: &DataFrame,
        schema: &DataSchema,
        debug: &DebugLog,
    ) -> Result<AnalysisTree> {
        let start_time = Instant::now();
        let mut queue: VecDeque<AnalysisTask> = VecDeque::new();
        let mut tree = AnalysisTree::new();

        // Log configuration
        debug.section("BRIGHTFLOW ANALYSIS DEBUG LOG");
        debug.subsection("Configuration");
        debug.kv("z_threshold", &format!("{}", self.z_threshold));
        debug.kv("p_threshold", &format!("{}", self.p_threshold));
        debug.kv("max_depth", &format!("{}", self.max_depth));

        debug.subsection("Schema");
        debug.kv("KPI columns", &format!("{:?}", schema.kpi_columns));
        debug.kv("Metric columns", &format!("{:?}", schema.metric_columns));
        debug.kv(
            "Dimension columns",
            &format!("{:?}", schema.dimension_columns),
        );
        debug.kv("Time column", &format!("{:?}", schema.time_column));
        debug.kv(
            "Time granularity",
            &format!("{:?}", schema.time_granularity),
        );

        // Pre-extract all columns once to avoid repeated DataFrame access
        let cache = ColumnCache::new(df, schema)?;

        // Pre-compute period labels once to avoid repeated date parsing
        let period_labels: Vec<Option<String>> = if let Some(time_col) = &schema.time_column {
            get_period_labels(df.column(time_col)?, schema.time_granularity)?
        } else {
            Vec::new()
        };

        let setup_time = start_time.elapsed();

        debug.subsection("Data Summary");
        debug.kv("Rows", &format!("{}", df.height()));
        debug.kv(
            "Numeric columns cached",
            &format!("{}", cache.numeric.len()),
        );
        debug.kv(
            "Dimension columns cached",
            &format!("{}", cache.dimension.len()),
        );
        if !period_labels.is_empty() {
            let unique_periods: std::collections::HashSet<_> =
                period_labels.iter().flatten().collect();
            debug.kv("Unique periods", &format!("{}", unique_periods.len()));
        }
        debug.kv(
            "Setup time",
            &format!("{:.2}ms", setup_time.as_secs_f64() * 1000.0),
        );

        // Analyze KPIs first (higher priority), then metrics
        debug.subsection("Queuing Initial Tasks");
        for col in &schema.kpi_columns {
            debug.task("queue", &format!("DetectAnomalies({col})"));
            queue.push_back(AnalysisTask::DetectAnomalies {
                column: col.clone(),
            });
            debug.task("queue", &format!("DetectTrend({col})"));
            queue.push_back(AnalysisTask::DetectTrend {
                column: col.clone(),
            });

            // Add period-based analysis if time column exists
            if schema.time_column.is_some() {
                debug.task("queue", &format!("ComparePeriods({col})"));
                queue.push_back(AnalysisTask::ComparePeriods {
                    column: col.clone(),
                });
                debug.task("queue", &format!("FindPeriodAnomaly({col})"));
                queue.push_back(AnalysisTask::FindPeriodAnomaly {
                    column: col.clone(),
                });
                debug.task("queue", &format!("DetectSeasonality({col})"));
                queue.push_back(AnalysisTask::DetectSeasonality {
                    column: col.clone(),
                });
                debug.task("queue", &format!("DetectForecastDeviation({col})"));
                queue.push_back(AnalysisTask::DetectForecastDeviation {
                    column: col.clone(),
                });
            }
        }
        for col in &schema.metric_columns {
            debug.task("queue", &format!("DetectAnomalies({col})"));
            queue.push_back(AnalysisTask::DetectAnomalies {
                column: col.clone(),
            });
            debug.task("queue", &format!("DetectTrend({col})"));
            queue.push_back(AnalysisTask::DetectTrend {
                column: col.clone(),
            });

            // Add period-based analysis for metrics too
            if schema.time_column.is_some() {
                debug.task("queue", &format!("ComparePeriods({col})"));
                queue.push_back(AnalysisTask::ComparePeriods {
                    column: col.clone(),
                });
                debug.task("queue", &format!("DetectSeasonality({col})"));
                queue.push_back(AnalysisTask::DetectSeasonality {
                    column: col.clone(),
                });
                debug.task("queue", &format!("DetectForecastDeviation({col})"));
                queue.push_back(AnalysisTask::DetectForecastDeviation {
                    column: col.clone(),
                });
            }
        }

        // Queue outlier clustering (runs after initial anomaly detection)
        if schema.time_column.is_some() {
            debug.task("queue", "FindOutlierClusters");
            queue.push_back(AnalysisTask::FindOutlierClusters);
        }

        debug.section("PROCESSING ANALYSIS QUEUE");

        while let Some(task) = queue.pop_front() {
            match task {
                AnalysisTask::DetectAnomalies { column } => {
                    if let Some(anomaly) = detect_anomaly(df, &column)? {
                        let values = vec![
                            ("value", format!("{:.4}", anomaly.value)),
                            ("mean", format!("{:.4}", anomaly.mean)),
                            ("std_dev", format!("{:.4}", anomaly.std_dev)),
                            ("z_score", format!("{:.4}", anomaly.z_score)),
                            ("threshold", format!("|z| > {}", self.z_threshold)),
                        ];

                        if anomaly.z_score.abs() > self.z_threshold {
                            debug.analysis(
                                "DetectAnomalies",
                                &column,
                                AnalysisOutcome::triggered(
                                    values,
                                    format!(
                                        "|z|={:.2} > threshold={}",
                                        anomaly.z_score.abs(),
                                        self.z_threshold
                                    ),
                                ),
                            );

                            let description = format!(
                                "Anomaly detected in '{}': latest value {:.2} is {:.1} std devs {} the mean ({:.2})",
                                column,
                                anomaly.value,
                                anomaly.z_score.abs(),
                                if anomaly.z_score > 0.0 { "above" } else { "below" },
                                anomaly.mean
                            );

                            let node_id = tree.add_root(
                                AnalysisType::Anomaly {
                                    column: anomaly.column.clone(),
                                    value: anomaly.value,
                                    mean: anomaly.mean,
                                    std_dev: anomaly.std_dev,
                                    z_score: anomaly.z_score,
                                },
                                anomaly.z_score.abs(),
                                description,
                            );

                            for cat_col in &schema.dimension_columns {
                                debug.task(
                                    "spawn",
                                    &format!("AttributeSegment({column}, {cat_col})"),
                                );
                                queue.push_back(AnalysisTask::AttributeSegment {
                                    parent_id: node_id,
                                    target_col: column.clone(),
                                    segment_col: cat_col.clone(),
                                    depth: 1,
                                });
                            }
                        } else {
                            debug.analysis(
                                "DetectAnomalies",
                                &column,
                                AnalysisOutcome::below_threshold(
                                    values,
                                    format!(
                                        "|z|={:.2} <= threshold={}",
                                        anomaly.z_score.abs(),
                                        self.z_threshold
                                    ),
                                ),
                            );
                        }
                    } else {
                        debug.analysis("DetectAnomalies", &column, AnalysisOutcome::NoData);
                    }
                },

                AnalysisTask::ComparePeriods { column } => {
                    let Some(metric_values) = cache.numeric.get(&column) else {
                        debug.analysis("ComparePeriods", &column, AnalysisOutcome::NoData);
                        continue;
                    };
                    if let Some(comparison) =
                        compare_periods_cached(&column, metric_values, &period_labels)
                    {
                        // Report if statistically significant OR if change is large (>50%)
                        let is_significant = comparison.p_value < self.p_threshold
                            && comparison.change_percent.abs() > 10.0;
                        let is_large_change = comparison.change_percent.abs() > 50.0;

                        let values = vec![
                            ("current_period", comparison.current_period.clone()),
                            ("previous_period", comparison.previous_period.clone()),
                            ("current_value", format!("{:.4}", comparison.current_value)),
                            (
                                "previous_value",
                                format!("{:.4}", comparison.previous_value),
                            ),
                            (
                                "change_percent",
                                format!("{:.2}%", comparison.change_percent),
                            ),
                            ("p_value", format!("{:.6}", comparison.p_value)),
                            (
                                "is_significant",
                                format!(
                                    "{} (p<{} && |change|>10%)",
                                    is_significant, self.p_threshold
                                ),
                            ),
                            (
                                "is_large_change",
                                format!("{is_large_change} (|change|>50%)"),
                            ),
                        ];

                        if is_significant || is_large_change {
                            debug.analysis(
                                "ComparePeriods",
                                &column,
                                AnalysisOutcome::triggered(
                                    values,
                                    if is_large_change {
                                        "large change >50%"
                                    } else {
                                        "statistically significant"
                                    },
                                ),
                            );

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

                            let node_id = tree.add_root(
                                AnalysisType::PeriodComparison {
                                    column: comparison.column.clone(),
                                    current_period: comparison.current_period.clone(),
                                    previous_period: comparison.previous_period.clone(),
                                    current_value: comparison.current_value,
                                    previous_value: comparison.previous_value,
                                    change_percent: comparison.change_percent,
                                    p_value: comparison.p_value,
                                },
                                comparison.change_percent.abs() / comparison.p_value,
                                description,
                            );

                            // Spawn period-aware segment attribution to explain the change
                            for cat_col in &schema.dimension_columns {
                                // Smart filtering: only attribute to dimensions with data in this period
                                let (rows_in_period, unique_values) = cache
                                    .dimension_coverage_in_period(
                                        cat_col,
                                        &comparison.current_period,
                                        &period_labels,
                                    );

                                if rows_in_period < 2 || unique_values < 2 {
                                    debug.log(&format!(
                                        "  [skip] AttributePeriodSegment({}, {}, {}) - insufficient data: {} rows, {} unique values",
                                        column, cat_col, comparison.current_period, rows_in_period, unique_values
                                    ));
                                    continue;
                                }

                                debug.task(
                                    "spawn",
                                    &format!(
                                        "AttributePeriodSegment({}, {}, {})",
                                        column, cat_col, comparison.current_period
                                    ),
                                );
                                queue.push_back(AnalysisTask::AttributePeriodSegment {
                                    parent_id: node_id,
                                    target_col: column.clone(),
                                    segment_col: cat_col.clone(),
                                    period: comparison.current_period.clone(),
                                    depth: 1,
                                });
                            }
                        } else {
                            debug.analysis(
                                "ComparePeriods",
                                &column,
                                AnalysisOutcome::below_threshold(
                                    values,
                                    format!(
                                        "change={:.1}% not significant enough",
                                        comparison.change_percent
                                    ),
                                ),
                            );
                        }
                    } else {
                        debug.analysis("ComparePeriods", &column, AnalysisOutcome::NoData);
                    }
                },

                AnalysisTask::FindPeriodAnomaly { column } => {
                    let Some(metric_values) = cache.numeric.get(&column) else {
                        debug.analysis("FindPeriodAnomaly", &column, AnalysisOutcome::NoData);
                        continue;
                    };
                    if let Some(anomaly) =
                        find_anomalous_period_cached(&column, metric_values, &period_labels)
                    {
                        // Report if statistically significant OR if change is large (>50%)
                        let is_significant = anomaly.p_value < self.p_threshold
                            && anomaly.change_percent.abs() > 20.0;
                        let is_large_change = anomaly.change_percent.abs() > 50.0;

                        let values = vec![
                            ("anomalous_period", anomaly.current_period.clone()),
                            ("period_value", format!("{:.4}", anomaly.current_value)),
                            (
                                "other_periods_mean",
                                format!("{:.4}", anomaly.previous_value),
                            ),
                            ("change_percent", format!("{:.2}%", anomaly.change_percent)),
                            ("p_value", format!("{:.6}", anomaly.p_value)),
                            (
                                "is_significant",
                                format!(
                                    "{} (p<{} && |change|>20%)",
                                    is_significant, self.p_threshold
                                ),
                            ),
                            (
                                "is_large_change",
                                format!("{is_large_change} (|change|>50%)"),
                            ),
                        ];

                        if is_significant || is_large_change {
                            debug.analysis(
                                "FindPeriodAnomaly",
                                &column,
                                AnalysisOutcome::triggered(
                                    values,
                                    if is_large_change {
                                        "large change >50%"
                                    } else {
                                        "statistically significant"
                                    },
                                ),
                            );

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

                            let node_id = tree.add_root(
                                AnalysisType::PeriodAnomaly {
                                    column: anomaly.column.clone(),
                                    period: anomaly.current_period.clone(),
                                    period_value: anomaly.current_value,
                                    other_periods_mean: anomaly.previous_value,
                                    change_percent: anomaly.change_percent,
                                    p_value: anomaly.p_value,
                                },
                                anomaly.change_percent.abs() / anomaly.p_value,
                                description,
                            );

                            // Spawn period-aware segment attribution to explain this period
                            for cat_col in &schema.dimension_columns {
                                // Smart filtering: only attribute to dimensions with data in this period
                                let (rows_in_period, unique_values) = cache
                                    .dimension_coverage_in_period(
                                        cat_col,
                                        &anomaly.current_period,
                                        &period_labels,
                                    );

                                if rows_in_period < 2 || unique_values < 2 {
                                    debug.log(&format!(
                                        "  [skip] AttributePeriodSegment({}, {}, {}) - insufficient data: {} rows, {} unique values",
                                        column, cat_col, anomaly.current_period, rows_in_period, unique_values
                                    ));
                                    continue;
                                }

                                debug.task(
                                    "spawn",
                                    &format!(
                                        "AttributePeriodSegment({}, {}, {})",
                                        column, cat_col, anomaly.current_period
                                    ),
                                );
                                queue.push_back(AnalysisTask::AttributePeriodSegment {
                                    parent_id: node_id,
                                    target_col: column.clone(),
                                    segment_col: cat_col.clone(),
                                    period: anomaly.current_period.clone(),
                                    depth: 1,
                                });
                            }
                        } else {
                            debug.analysis(
                                "FindPeriodAnomaly",
                                &column,
                                AnalysisOutcome::below_threshold(
                                    values,
                                    format!(
                                        "change={:.1}% not significant enough",
                                        anomaly.change_percent
                                    ),
                                ),
                            );
                        }
                    } else {
                        debug.analysis("FindPeriodAnomaly", &column, AnalysisOutcome::NoData);
                    }
                },

                AnalysisTask::AttributePeriodSegment {
                    parent_id,
                    target_col,
                    segment_col,
                    period,
                    depth,
                } => {
                    if depth >= self.max_depth {
                        debug.log(&format!(
                            "  [skip] AttributePeriodSegment({}, {}) - max depth {} reached",
                            target_col, segment_col, self.max_depth
                        ));
                        continue;
                    }

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

                    if segments.is_empty() {
                        debug.segment(&target_col, &segment_col, "(none)", AnalysisOutcome::NoData);
                    }

                    for attr in segments {
                        let values = vec![
                            ("segment_value", attr.segment_value.clone()),
                            ("change_percent", format!("{:.2}%", attr.change_percent)),
                            ("contribution", format!("{:.4}", attr.contribution)),
                            ("contribution_pct", format!("{:.2}%", attr.contribution_pct)),
                            ("p_value", format!("{:.6}", attr.p_value)),
                        ];

                        if attr.p_value < self.p_threshold || attr.contribution.abs() > 0.0 {
                            debug.segment(
                                &target_col,
                                &segment_col,
                                &attr.segment_value,
                                AnalysisOutcome::triggered(values, "has contribution"),
                            );

                            let description = format!(
                                "In {}: '{}' = '{}' was {:.0}% {}, contributing {:.0}% of total change",
                                period,
                                segment_col,
                                attr.segment_value,
                                attr.change_percent.abs(),
                                if attr.change_percent > 0.0 { "higher" } else { "lower" },
                                attr.contribution_pct.abs()
                            );
                            tree.add_child(
                                parent_id,
                                AnalysisType::Segment {
                                    target_column: attr.target_column,
                                    segment_column: attr.segment_column,
                                    segment_value: attr.segment_value,
                                    contribution: attr.contribution,
                                    change_percent: attr.change_percent,
                                    contribution_pct: attr.contribution_pct,
                                    p_value: attr.p_value,
                                },
                                attr.contribution_pct.abs(), // Sort by contribution percentage
                                description,
                            );
                        } else {
                            debug.segment(
                                &target_col,
                                &segment_col,
                                &attr.segment_value,
                                AnalysisOutcome::below_threshold(
                                    values,
                                    "no significant contribution",
                                ),
                            );
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
                        debug.log(&format!(
                            "  [skip] AttributeSegment({}, {}) - max depth {} reached",
                            target_col, segment_col, self.max_depth
                        ));
                        continue;
                    }

                    if let Some(attr) = attribute_segment(df, &target_col, &segment_col)? {
                        let values = vec![
                            ("segment_value", attr.segment_value.clone()),
                            ("contribution", format!("{:.4}", attr.contribution)),
                            ("change_percent", format!("{:.2}%", attr.change_percent)),
                            ("p_value", format!("{:.6}", attr.p_value)),
                            ("threshold", format!("p < {}", self.p_threshold)),
                        ];

                        if attr.p_value < self.p_threshold {
                            debug.segment(
                                &target_col,
                                &segment_col,
                                &attr.segment_value,
                                AnalysisOutcome::triggered(
                                    values,
                                    format!("p={:.4} < {}", attr.p_value, self.p_threshold),
                                ),
                            );

                            let description = format!(
                                "Segment '{}' = '{}' contributes {:.2} to '{}' (p={:.4})",
                                segment_col,
                                attr.segment_value,
                                attr.contribution,
                                target_col,
                                attr.p_value
                            );

                            let node_id = tree.add_child(
                                parent_id,
                                AnalysisType::Segment {
                                    target_column: attr.target_column.clone(),
                                    segment_column: attr.segment_column.clone(),
                                    segment_value: attr.segment_value.clone(),
                                    contribution: attr.contribution,
                                    change_percent: attr.change_percent,
                                    contribution_pct: attr.contribution_pct,
                                    p_value: attr.p_value,
                                },
                                1.0 / attr.p_value,
                                description,
                            );

                            debug.task("spawn", &format!("SearchCorrelations({target_col})"));
                            queue.push_back(AnalysisTask::SearchCorrelations {
                                parent_id: node_id,
                                target_col: target_col.clone(),
                                depth: depth + 1,
                            });
                        } else {
                            debug.segment(
                                &target_col,
                                &segment_col,
                                &attr.segment_value,
                                AnalysisOutcome::below_threshold(
                                    values,
                                    format!("p={:.4} >= {}", attr.p_value, self.p_threshold),
                                ),
                            );
                        }
                    } else {
                        debug.segment(&target_col, &segment_col, "(none)", AnalysisOutcome::NoData);
                    }
                },

                AnalysisTask::SearchCorrelations {
                    parent_id,
                    target_col,
                    depth,
                } => {
                    if depth >= self.max_depth {
                        debug.log(&format!(
                            "  [skip] SearchCorrelations({}) - max depth {} reached",
                            target_col, self.max_depth
                        ));
                        continue;
                    }

                    for other_col in &schema.analyzable_columns() {
                        if other_col != &target_col {
                            if let Some(corr) = correlate(df, &target_col, other_col)? {
                                let values = vec![
                                    ("column_a", target_col.clone()),
                                    ("column_b", other_col.clone()),
                                    ("r_value", format!("{:.4}", corr.r_value)),
                                    ("p_value", format!("{:.6}", corr.p_value)),
                                    ("threshold", format!("p<{} && |r|>0.5", self.p_threshold)),
                                ];

                                if corr.p_value < self.p_threshold && corr.r_value.abs() > 0.5 {
                                    debug.analysis(
                                        "Correlation",
                                        &format!("{target_col} vs {other_col}"),
                                        AnalysisOutcome::triggered(
                                            values,
                                            format!("r={:.3}, p={:.4}", corr.r_value, corr.p_value),
                                        ),
                                    );

                                    let description = format!(
                                        "Correlation between '{}' and '{}': r={:.3} (p={:.4})",
                                        target_col, other_col, corr.r_value, corr.p_value
                                    );

                                    tree.add_child(
                                        parent_id,
                                        AnalysisType::Correlation {
                                            column_a: corr.column_a,
                                            column_b: corr.column_b,
                                            r_value: corr.r_value,
                                            p_value: corr.p_value,
                                        },
                                        corr.r_value.abs() / corr.p_value,
                                        description,
                                    );
                                } else {
                                    debug.analysis(
                                        "Correlation",
                                        &format!("{target_col} vs {other_col}"),
                                        AnalysisOutcome::below_threshold(
                                            values,
                                            if corr.r_value.abs() <= 0.5 {
                                                format!("|r|={:.3} <= 0.5", corr.r_value.abs())
                                            } else {
                                                format!(
                                                    "p={:.4} >= {}",
                                                    corr.p_value, self.p_threshold
                                                )
                                            },
                                        ),
                                    );
                                }
                            }
                        }
                    }
                },

                AnalysisTask::DetectTrend { column } => {
                    if let Some(trend) = detect_trend(df, &column)? {
                        let direction_str = match trend.direction {
                            crate::analysis::tree::TrendDirection::Increasing => "increasing",
                            crate::analysis::tree::TrendDirection::Decreasing => "decreasing",
                        };

                        let values = vec![
                            ("direction", direction_str.to_string()),
                            ("slope", format!("{:.6}", trend.slope)),
                            ("r_squared", format!("{:.4}", trend.r_squared)),
                            ("p_value", format!("{:.6}", trend.p_value)),
                            ("threshold", format!("p<{} && R²>0.5", self.p_threshold)),
                        ];

                        if trend.p_value < self.p_threshold && trend.r_squared > 0.5 {
                            debug.analysis(
                                "DetectTrend",
                                &column,
                                AnalysisOutcome::triggered(
                                    values,
                                    format!("R²={:.3}, p={:.4}", trend.r_squared, trend.p_value),
                                ),
                            );

                            let description = format!(
                                "Significant {} trend in '{}': slope={:.4}, R²={:.3} (p={:.4})",
                                direction_str, column, trend.slope, trend.r_squared, trend.p_value
                            );

                            tree.add_root(
                                AnalysisType::Trend {
                                    column: trend.column,
                                    direction: trend.direction,
                                    slope: trend.slope,
                                    r_squared: trend.r_squared,
                                    p_value: trend.p_value,
                                },
                                trend.r_squared / trend.p_value,
                                description,
                            );
                        } else {
                            debug.analysis(
                                "DetectTrend",
                                &column,
                                AnalysisOutcome::below_threshold(
                                    values,
                                    if trend.r_squared <= 0.5 {
                                        format!("R²={:.3} <= 0.5 (weak fit)", trend.r_squared)
                                    } else {
                                        format!(
                                            "p={:.4} >= {} (not significant)",
                                            trend.p_value, self.p_threshold
                                        )
                                    },
                                ),
                            );
                        }
                    } else {
                        debug.analysis("DetectTrend", &column, AnalysisOutcome::NoData);
                    }
                },

                AnalysisTask::DetectSeasonality { column } => {
                    let Some(metric_values) = cache.numeric.get(&column) else {
                        debug.analysis("DetectSeasonality", &column, AnalysisOutcome::NoData);
                        continue;
                    };

                    // Extract timestamps from time column using the period module's extract function
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
                        debug.analysis("DetectSeasonality", &column, AnalysisOutcome::NoData);
                        continue;
                    }

                    if let Some(result) =
                        detect_seasonality(&column, metric_values, &timestamps_sec)
                    {
                        let values = vec![
                            ("period_name", result.period_name.clone()),
                            ("period_days", format!("{}", result.period_days)),
                            ("autocorrelation", format!("{:.4}", result.autocorrelation)),
                            ("p_value", format!("{:.6}", result.p_value)),
                            ("threshold", "|r| > 0.5 && p < 0.05".to_string()),
                        ];

                        // Trigger criteria: |r| > 0.5 and p < 0.05 (already enforced in detect_seasonality)
                        debug.analysis(
                            "DetectSeasonality",
                            &column,
                            AnalysisOutcome::triggered(
                                values,
                                format!("r={:.3}, p={:.4}", result.autocorrelation, result.p_value),
                            ),
                        );

                        let description = format!(
                            "Seasonality detected in '{}': {} pattern (r={:.3}, p={:.4})",
                            column, result.period_name, result.autocorrelation, result.p_value
                        );

                        tree.add_root(
                            AnalysisType::Seasonality {
                                column: result.column,
                                period_name: result.period_name,
                                autocorrelation: result.autocorrelation,
                                p_value: result.p_value,
                            },
                            result.autocorrelation.abs() / result.p_value,
                            description,
                        );
                    } else {
                        debug.analysis(
                            "DetectSeasonality",
                            &column,
                            AnalysisOutcome::below_threshold(
                                vec![("note", "No significant seasonality found".to_string())],
                                "No pattern with |r| > 0.5 and p < 0.05",
                            ),
                        );
                    }
                },

                AnalysisTask::DetectForecastDeviation { column } => {
                    let Some(metric_values) = cache.numeric.get(&column) else {
                        debug.analysis("DetectForecastDeviation", &column, AnalysisOutcome::NoData);
                        continue;
                    };

                    // Aggregate values by period
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

                    // Sort by period name to ensure chronological order
                    period_values.sort_by(|a, b| a.0.cmp(&b.0));

                    if let Some(deviation) = detect_forecast_deviation(&column, &period_values) {
                        let values = vec![
                            ("period", deviation.period.clone()),
                            ("actual", format!("{:.4}", deviation.actual)),
                            ("expected", format!("{:.4}", deviation.expected)),
                            (
                                "deviation_percent",
                                format!("{:.2}%", deviation.deviation_percent),
                            ),
                            ("p_value", format!("{:.6}", deviation.p_value)),
                            ("threshold", "|deviation| > 20% && p < 0.05".to_string()),
                        ];

                        // Trigger criteria: |deviation| > 20% and p < 0.05
                        if deviation.deviation_percent.abs() > 20.0 && deviation.p_value < 0.05 {
                            debug.analysis(
                                "DetectForecastDeviation",
                                &column,
                                AnalysisOutcome::triggered(
                                    values,
                                    format!(
                                        "deviation={:.1}%, p={:.4}",
                                        deviation.deviation_percent, deviation.p_value
                                    ),
                                ),
                            );

                            let description = format!(
                                "Forecast deviation in '{}' for {}: actual {:.2} vs expected {:.2} ({:.1}% deviation, p={:.4})",
                                column, deviation.period, deviation.actual, deviation.expected,
                                deviation.deviation_percent, deviation.p_value
                            );

                            tree.add_root(
                                AnalysisType::ForecastDeviation {
                                    column: deviation.column,
                                    period: deviation.period,
                                    actual: deviation.actual,
                                    expected: deviation.expected,
                                    deviation_percent: deviation.deviation_percent,
                                    p_value: deviation.p_value,
                                },
                                deviation.deviation_percent.abs() / deviation.p_value,
                                description,
                            );
                        } else {
                            debug.analysis(
                                "DetectForecastDeviation",
                                &column,
                                AnalysisOutcome::below_threshold(
                                    values,
                                    format!(
                                        "deviation={:.1}% or p={:.4} below threshold",
                                        deviation.deviation_percent.abs(),
                                        deviation.p_value
                                    ),
                                ),
                            );
                        }
                    } else {
                        debug.analysis("DetectForecastDeviation", &column, AnalysisOutcome::NoData);
                    }
                },

                AnalysisTask::FindOutlierClusters => {
                    let clusters = find_outlier_clusters(&cache, &period_labels, self.z_threshold);

                    if clusters.is_empty() {
                        debug.analysis(
                            "FindOutlierClusters",
                            "all",
                            AnalysisOutcome::below_threshold(
                                vec![("note", "No outlier clusters found".to_string())],
                                "No periods with 2+ same-direction outliers",
                            ),
                        );
                    } else {
                        for cluster in clusters {
                            let values = vec![
                                ("period", cluster.period.clone()),
                                ("columns", cluster.columns.join(", ")),
                                ("direction", cluster.direction.clone()),
                                ("cluster_size", format!("{}", cluster.columns.len())),
                            ];

                            debug.analysis(
                                "FindOutlierClusters",
                                &cluster.period,
                                AnalysisOutcome::triggered(
                                    values,
                                    format!(
                                        "{} columns in {} cluster",
                                        cluster.columns.len(),
                                        cluster.direction
                                    ),
                                ),
                            );

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

                            tree.add_root(
                                AnalysisType::OutlierCluster {
                                    period: cluster.period,
                                    columns: cluster.columns.clone(),
                                    direction: cluster.direction,
                                    cluster_size: cluster.columns.len(),
                                },
                                cluster.columns.len() as f64,
                                description,
                            );
                        }
                    }
                },
            }
        }

        // Final summary
        let total_time = start_time.elapsed();
        debug.section("ANALYSIS COMPLETE");
        debug.kv("Root findings", &format!("{}", tree.roots.len()));
        debug.kv("Total nodes", &format!("{}", tree.nodes.len()));
        debug.kv(
            "Setup time",
            &format!("{:.2}ms", setup_time.as_secs_f64() * 1000.0),
        );
        debug.kv(
            "Analysis time",
            &format!(
                "{:.2}ms",
                total_time.saturating_sub(setup_time).as_secs_f64() * 1000.0
            ),
        );
        debug.kv(
            "Total time",
            &format!("{:.2}ms", total_time.as_secs_f64() * 1000.0),
        );
        debug.flush();

        Ok(tree)
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
    ) -> Result<AnalysisTree> {
        let start_time = Instant::now();
        let mut queue: VecDeque<AnalysisTask> = VecDeque::new();
        let mut tree = AnalysisTree::new();

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
            return Ok(tree);
        }

        // Safe: we checked len >= 2 above
        let Some(current_period) = unique_periods.last().cloned() else {
            return Ok(tree);
        };
        let Some(previous_period) = unique_periods.get(unique_periods.len() - 2).cloned() else {
            return Ok(tree);
        };

        debug.kv("Current period", &current_period);
        debug.kv("Previous period", &previous_period);

        // Compare each KPI between current and previous period
        for col in schema
            .kpi_columns
            .iter()
            .chain(schema.metric_columns.iter())
        {
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

                let node_id = tree.add_root(
                    AnalysisType::PeriodComparison {
                        column: col.clone(),
                        current_period: current_period.clone(),
                        previous_period: previous_period.clone(),
                        current_value: current_mean,
                        previous_value: previous_mean,
                        change_percent,
                        p_value,
                    },
                    change_percent.abs() / p_value.max(0.0001),
                    description,
                );

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
                        tree.add_child(
                            parent_id,
                            AnalysisType::Segment {
                                target_column: attr.target_column,
                                segment_column: attr.segment_column,
                                segment_value: attr.segment_value,
                                contribution: attr.contribution,
                                change_percent: attr.change_percent,
                                contribution_pct: attr.contribution_pct,
                                p_value: attr.p_value,
                            },
                            attr.contribution_pct.abs(),
                            description,
                        );
                    }
                }
            }
        }

        let total_time = start_time.elapsed();
        debug.section("REVIEW COMPLETE");
        debug.kv("Root findings", &format!("{}", tree.roots.len()));
        debug.kv("Total nodes", &format!("{}", tree.nodes.len()));
        debug.kv(
            "Total time",
            &format!("{:.2}ms", total_time.as_secs_f64() * 1000.0),
        );
        debug.flush();

        Ok(tree)
    }

    /// Review report: anomaly detection with attribution (How are we doing? What happened? Why?)
    /// Fallback for when no time column exists
    fn run_review_impl(
        &self,
        df: &DataFrame,
        schema: &DataSchema,
        debug: &DebugLog,
    ) -> Result<AnalysisTree> {
        let start_time = Instant::now();
        let mut queue: VecDeque<AnalysisTask> = VecDeque::new();
        let mut tree = AnalysisTree::new();

        debug.section("REVIEW REPORT");
        debug.subsection("Configuration");
        debug.kv("z_threshold", &format!("{}", self.z_threshold));
        debug.kv("p_threshold", &format!("{}", self.p_threshold));

        let cache = ColumnCache::new(df, schema)?;
        let setup_time = start_time.elapsed();

        // Queue anomaly detection for KPIs and metrics
        for col in schema
            .kpi_columns
            .iter()
            .chain(schema.metric_columns.iter())
        {
            queue.push_back(AnalysisTask::DetectAnomalies {
                column: col.clone(),
            });
        }

        while let Some(task) = queue.pop_front() {
            match task {
                AnalysisTask::DetectAnomalies { column } => {
                    if let Some(anomaly) = detect_anomaly(df, &column)? {
                        if anomaly.z_score.abs() > self.z_threshold {
                            let description = format!(
                                "Anomaly detected in '{}': latest value {:.2} is {:.1} std devs {} the mean ({:.2})",
                                column, anomaly.value, anomaly.z_score.abs(),
                                if anomaly.z_score > 0.0 { "above" } else { "below" }, anomaly.mean
                            );

                            let node_id = tree.add_root(
                                AnalysisType::Anomaly {
                                    column: anomaly.column.clone(),
                                    value: anomaly.value,
                                    mean: anomaly.mean,
                                    std_dev: anomaly.std_dev,
                                    z_score: anomaly.z_score,
                                },
                                anomaly.z_score.abs(),
                                description,
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
                            let node_id = tree.add_child(
                                parent_id,
                                AnalysisType::Segment {
                                    target_column: attr.target_column.clone(),
                                    segment_column: attr.segment_column.clone(),
                                    segment_value: attr.segment_value.clone(),
                                    contribution: attr.contribution,
                                    change_percent: attr.change_percent,
                                    contribution_pct: attr.contribution_pct,
                                    p_value: attr.p_value,
                                },
                                1.0 / attr.p_value,
                                description,
                            );
                            queue.push_back(AnalysisTask::SearchCorrelations {
                                parent_id: node_id,
                                target_col: target_col.clone(),
                                depth: depth + 1,
                            });
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
                    for other_col in &schema.analyzable_columns() {
                        if other_col != &target_col {
                            if let Some(corr) = correlate(df, &target_col, other_col)? {
                                if corr.p_value < self.p_threshold && corr.r_value.abs() > 0.5 {
                                    let description = format!(
                                        "Correlation between '{}' and '{}': r={:.3} (p={:.4})",
                                        target_col, other_col, corr.r_value, corr.p_value
                                    );
                                    tree.add_child(
                                        parent_id,
                                        AnalysisType::Correlation {
                                            column_a: corr.column_a,
                                            column_b: corr.column_b,
                                            r_value: corr.r_value,
                                            p_value: corr.p_value,
                                        },
                                        corr.r_value.abs() / corr.p_value,
                                        description,
                                    );
                                }
                            }
                        }
                    }
                },
                _ => {}, // Ignore other task types in review
            }
        }

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
        drop((cache, setup_time));

        Ok(tree)
    }

    /// Trends report: time-based patterns and forecasting (What's changing over time?)
    fn run_trends_impl(
        &self,
        df: &DataFrame,
        schema: &DataSchema,
        debug: &DebugLog,
    ) -> Result<AnalysisTree> {
        let start_time = Instant::now();
        let mut queue: VecDeque<AnalysisTask> = VecDeque::new();
        let mut tree = AnalysisTree::new();

        debug.section("TRENDS REPORT");

        let cache = ColumnCache::new(df, schema)?;
        let period_labels: Vec<Option<String>> = if let Some(time_col) = &schema.time_column {
            get_period_labels(df.column(time_col)?, schema.time_granularity)?
        } else {
            Vec::new()
        };

        // Queue trend analyses for KPIs and metrics
        for col in schema
            .kpi_columns
            .iter()
            .chain(schema.metric_columns.iter())
        {
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
            }
        }
        if schema.time_column.is_some() {
            queue.push_back(AnalysisTask::FindOutlierClusters);
        }

        while let Some(task) = queue.pop_front() {
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
                            tree.add_root(
                                AnalysisType::Trend {
                                    column: trend.column,
                                    direction: trend.direction,
                                    slope: trend.slope,
                                    r_squared: trend.r_squared,
                                    p_value: trend.p_value,
                                },
                                trend.r_squared / trend.p_value,
                                description,
                            );
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
                            tree.add_root(
                                AnalysisType::PeriodComparison {
                                    column: comparison.column,
                                    current_period: comparison.current_period,
                                    previous_period: comparison.previous_period,
                                    current_value: comparison.current_value,
                                    previous_value: comparison.previous_value,
                                    change_percent: comparison.change_percent,
                                    p_value: comparison.p_value,
                                },
                                comparison.change_percent.abs() / comparison.p_value,
                                description,
                            );
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
                            tree.add_root(
                                AnalysisType::PeriodAnomaly {
                                    column: anomaly.column,
                                    period: anomaly.current_period,
                                    period_value: anomaly.current_value,
                                    other_periods_mean: anomaly.previous_value,
                                    change_percent: anomaly.change_percent,
                                    p_value: anomaly.p_value,
                                },
                                anomaly.change_percent.abs() / anomaly.p_value,
                                description,
                            );
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
                            tree.add_root(
                                AnalysisType::Seasonality {
                                    column: result.column,
                                    period_name: result.period_name,
                                    autocorrelation: result.autocorrelation,
                                    p_value: result.p_value,
                                },
                                result.autocorrelation.abs() / result.p_value,
                                description,
                            );
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
                            tree.add_root(
                                AnalysisType::ForecastDeviation {
                                    column: deviation.column,
                                    period: deviation.period,
                                    actual: deviation.actual,
                                    expected: deviation.expected,
                                    deviation_percent: deviation.deviation_percent,
                                    p_value: deviation.p_value,
                                },
                                deviation.deviation_percent.abs() / deviation.p_value,
                                description,
                            );
                        }
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
                        tree.add_root(
                            AnalysisType::OutlierCluster {
                                period: cluster.period,
                                columns: cluster.columns.clone(),
                                direction: cluster.direction,
                                cluster_size: cluster.columns.len(),
                            },
                            cluster.columns.len() as f64,
                            description,
                        );
                    }
                },
                _ => {}, // Ignore other task types in trends
            }
        }

        let total_time = start_time.elapsed();
        debug.section("TRENDS COMPLETE");
        debug.kv("Root findings", &format!("{}", tree.roots.len()));
        debug.kv("Total nodes", &format!("{}", tree.nodes.len()));
        debug.kv(
            "Total time",
            &format!("{:.2}ms", total_time.as_secs_f64() * 1000.0),
        );
        debug.flush();

        Ok(tree)
    }

    /// Drivers report: composition and driver analysis (What is driving performance?)
    #[allow(clippy::unnecessary_wraps, clippy::unused_self)] // Stub - will use self and may error when implemented
    fn run_drivers_impl(
        &self,
        _df: &DataFrame,
        _schema: &DataSchema,
        debug: &DebugLog,
    ) -> Result<AnalysisTree> {
        let tree = AnalysisTree::new();

        debug.section("DRIVERS REPORT");
        debug.log("Drivers analysis not yet implemented");
        debug.flush();

        // TODO: Implement composition analysis, pareto, etc.
        Ok(tree)
    }
}
