//! Review reports: "what changed lately, and why?"

use std::collections::VecDeque;

use anyhow::Result;
use polars::prelude::*;

use crate::analysis::anomaly::{detect_anomaly, detect_anomaly_in_series};
use crate::analysis::correlation::correlate;
use crate::analysis::dedup;
use crate::analysis::period::{aggregate_by_period_cached, get_period_labels};
use crate::analysis::scoring;
use crate::analysis::segment::{attribute_period_segments_cached, attribute_segment};
use crate::analysis::tree::{AnalysisResult, AnalysisTree, AnalysisType, NodeId, ReviewCadence};
use crate::data::config::TimeGranularity;
use crate::data::schema::DataSchema;

use super::cache::ColumnCache;
use super::meta::{add_scored_root, measure_ref, RootMeta};
use super::payloads::{
    build_anomaly_period_series, build_anomaly_series, build_period_comparison_data,
    build_scatter_data,
};
use super::pipeline::granularity_name;
use super::{add_child_scored, AnalysisEngine, AnalysisTask};
use crate::analysis::candidates::Aggregation;

/// Loop-invariant context for the cadence review arms.
struct CadenceCtx<'a> {
    cache: &'a ColumnCache,
    period_labels: &'a [Option<String>],
    granularity: TimeGranularity,
    current_period: &'a str,
    previous_period: &'a str,
}

/// Loop-invariant context for the timeless-review arms.
struct ReviewCtx<'a> {
    df: &'a DataFrame,
    schema: &'a DataSchema,
    cache: &'a ColumnCache,
}

/// One queued attribution: which parent to explain, over which (target,
/// segment) pair, at what drill depth.
#[derive(Clone, Copy)]
struct AttributionTask<'a> {
    parent_id: NodeId,
    target_col: &'a str,
    segment_col: &'a str,
    depth: usize,
}

impl AnalysisEngine {
    /// Review report with cadence awareness
    pub(super) fn run_review_cadence_impl(
        &self,
        df: &DataFrame,
        schema: &DataSchema,
        cadence: ReviewCadence,
    ) -> Result<AnalysisResult> {
        let mut queue: VecDeque<AnalysisTask> = VecDeque::new();
        let mut tree = AnalysisTree::new();
        let mut first_level_count: usize = 0;
        let mut deeper_count: usize = 0;

        let cache = ColumnCache::new(df, schema)?;

        // Get period labels at the cadence granularity
        let granularity = Self::cadence_to_granularity(cadence);
        let period_labels: Vec<Option<String>> = if let Some(time_col) = &schema.time_column {
            get_period_labels(df.column(time_col)?, granularity)?
        } else {
            // No time column - fall back to simple anomaly detection
            return self.run_review_impl(df, schema);
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

        let ctx = CadenceCtx {
            cache: &cache,
            period_labels: &period_labels,
            granularity,
            current_period: &current_period,
            previous_period: &previous_period,
        };

        first_level_count += self.period_comparison_pass(schema, &ctx, &mut tree, &mut queue);
        first_level_count += self.history_anomaly_pass(schema, &ctx, &mut tree, &mut queue);

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

                self.attribute_period_segment_arm(
                    &ctx,
                    parent_id,
                    &target_col,
                    &segment_col,
                    &period,
                    &mut tree,
                );
            }
        }

        // Dedup, then diversity-select the top roots
        dedup::dedup(&mut tree);
        crate::analysis::history::apply_novelty(&mut tree, &self.history, self.now_epoch);
        crate::analysis::select::select_top(&mut tree, self.select_top);

        Ok(AnalysisResult {
            tree,
            first_level_count,
            deeper_count,
        })
    }

    /// Compare each measure between the current and previous period; queue
    /// segment attribution for the movers. Returns the number of comparisons
    /// actually made (the first-level count contribution).
    fn period_comparison_pass(
        &self,
        schema: &DataSchema,
        ctx: &CadenceCtx<'_>,
        tree: &mut AnalysisTree,
        queue: &mut VecDeque<AnalysisTask>,
    ) -> usize {
        let mut first_level_count = 0;
        for col in &schema.measure_columns {
            let Some(metric_values) = ctx.cache.numeric.get(col) else {
                continue;
            };

            // Calculate stats for current and previous periods
            let mut current_values: Vec<f64> = Vec::new();
            let mut previous_values: Vec<f64> = Vec::new();

            for (val, period_opt) in metric_values.iter().zip(ctx.period_labels.iter()) {
                if let Some(period) = period_opt {
                    if period.as_str() == ctx.current_period {
                        current_values.push(*val);
                    } else if period.as_str() == ctx.previous_period {
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

            if !(is_significant || is_large_change) {
                continue;
            }
            let direction = if change_percent > 0.0 { "up" } else { "down" };
            let description = format!(
                "'{}' is {} {:.1}% in {} vs {} (p={:.4})",
                col,
                direction,
                change_percent.abs(),
                ctx.current_period,
                ctx.previous_period,
                p_value
            );

            let analysis = AnalysisType::PeriodComparison {
                column: col.clone(),
                current_period: ctx.current_period.to_string(),
                previous_period: ctx.previous_period.to_string(),
                current_value: current_mean,
                previous_value: previous_mean,
                change_percent,
                p_value,
            };
            let data = build_period_comparison_data(
                metric_values,
                ctx.period_labels,
                ctx.current_period,
                ctx.previous_period,
            );
            let Some(node_id) = add_scored_root(
                tree,
                &self.scoring_ctx,
                analysis,
                description,
                data,
                RootMeta {
                    detector: "period_comparison",
                    measure: measure_ref(col),
                    agg: Aggregation::Mean,
                    dimension: None,
                    granularity: granularity_name(ctx.granularity),
                    why: format!(
                        "covers the whole table; two identical periods would differ this much only {:.1}% of the time",
                        (p_value * 100.0).min(100.0)
                    ),
                },
            ) else {
                continue;
            };

            // Attribute to segments
            for seg_col in &schema.dimension_columns {
                queue.push_back(AnalysisTask::AttributePeriodSegment {
                    parent_id: node_id,
                    target_col: col.clone(),
                    segment_col: seg_col.clone(),
                    period: ctx.current_period.to_string(),
                    depth: 1,
                });
            }
        }
        first_level_count
    }

    /// Test the latest period against the full period history (the pairwise
    /// comparison only sees the previous period — a gradual drift or a spike
    /// vs a long stable baseline shows up here). Row volume gets the same
    /// test: "post count spiked today" is a review-grade story. Returns the
    /// first-level count contribution.
    fn history_anomaly_pass(
        &self,
        schema: &DataSchema,
        ctx: &CadenceCtx<'_>,
        tree: &mut AnalysisTree,
        queue: &mut VecDeque<AnalysisTask>,
    ) -> usize {
        let mut first_level_count = 0;
        let mut history_series: Vec<(String, Vec<String>, Vec<f64>)> = Vec::new();
        for col in &schema.measure_columns {
            let Some(metric_values) = ctx.cache.numeric.get(col) else {
                continue;
            };
            let stats = aggregate_by_period_cached(metric_values, ctx.period_labels);
            history_series.push((
                col.clone(),
                stats.iter().map(|s| s.period_label.clone()).collect(),
                stats.iter().map(|s| s.mean).collect(),
            ));
        }
        {
            // Per-period row counts as a synthetic "rows" measure
            let mut counts: std::collections::HashMap<String, f64> =
                std::collections::HashMap::new();
            for p in ctx.period_labels.iter().flatten() {
                *counts.entry(p.clone()).or_insert(0.0) += 1.0;
            }
            let mut sorted: Vec<(String, f64)> = counts.into_iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            history_series.push((
                "rows".to_string(),
                sorted.iter().map(|(p, _)| p.clone()).collect(),
                sorted.iter().map(|(_, v)| *v).collect(),
            ));
        }
        for (col, labels, means) in &history_series {
            let col = col.as_str();
            if means.len() < 4 {
                continue;
            }
            first_level_count += 1;
            let Some(anomaly) = detect_anomaly_in_series(col, means) else {
                continue;
            };
            if anomaly.z_score.abs() <= self.z_threshold {
                continue;
            }
            let description = format!(
                "'{}' in {} is {:.1} std devs {} its historical period mean ({:.2} vs {:.2})",
                col,
                ctx.current_period,
                anomaly.z_score.abs(),
                if anomaly.z_score > 0.0 {
                    "above"
                } else {
                    "below"
                },
                anomaly.value,
                anomaly.mean
            );
            let analysis = AnalysisType::Anomaly {
                column: anomaly.column.clone(),
                value: anomaly.value,
                mean: anomaly.mean,
                std_dev: anomaly.std_dev,
                z_score: anomaly.z_score,
            };
            let data = Some(build_anomaly_period_series(
                labels,
                means,
                anomaly.mean,
                anomaly.std_dev,
            ));
            let Some(node_id) = add_scored_root(
                tree,
                &self.scoring_ctx,
                analysis,
                description,
                data,
                RootMeta {
                    detector: "history_anomaly",
                    measure: measure_ref(col),
                    agg: if col == "rows" {
                        Aggregation::Count
                    } else {
                        Aggregation::Mean
                    },
                    dimension: None,
                    granularity: granularity_name(ctx.granularity),
                    why: format!(
                        "covers the whole table; a typical period sits within 2 standard deviations of history — this one is {:.1} away",
                        anomaly.z_score.abs()
                    ),
                },
            ) else {
                continue;
            };
            if col != "rows" {
                for seg_col in &schema.dimension_columns {
                    queue.push_back(AnalysisTask::AttributePeriodSegment {
                        parent_id: node_id,
                        target_col: col.to_string(),
                        segment_col: seg_col.clone(),
                        period: ctx.current_period.to_string(),
                        depth: 1,
                    });
                }
            }
        }
        first_level_count
    }

    /// Attribute one parent finding's movement to the values of a dimension
    /// within the current period.
    fn attribute_period_segment_arm(
        &self,
        ctx: &CadenceCtx<'_>,
        parent_id: NodeId,
        target_col: &str,
        segment_col: &str,
        period: &str,
        tree: &mut AnalysisTree,
    ) {
        let Some(target_values) = ctx.cache.numeric.get(target_col) else {
            return;
        };
        let Some(segment_values) = ctx.cache.dimension.get(segment_col) else {
            return;
        };

        let segments = attribute_period_segments_cached(
            target_col,
            segment_col,
            target_values,
            segment_values,
            period,
            ctx.period_labels,
        );

        for attr in segments {
            if attr.p_value < self.p_threshold || attr.contribution.abs() > 0.0 {
                let description = format!(
                    "In {}: '{}' = '{}' was {:.0}% {}, contributing {:.0}% of total change",
                    period,
                    segment_col,
                    attr.segment_value,
                    attr.change_percent.abs(),
                    if attr.change_percent > 0.0 {
                        "higher"
                    } else {
                        "lower"
                    },
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
                if let Some(child_id) =
                    add_child_scored(tree, parent_id, analysis, description, &self.scoring_ctx)
                {
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

    /// Review report: anomaly detection with attribution (How are we doing? What happened? Why?)
    /// Fallback for when no time column exists
    pub(super) fn run_review_impl(
        &self,
        df: &DataFrame,
        schema: &DataSchema,
    ) -> Result<AnalysisResult> {
        let mut queue: VecDeque<AnalysisTask> = VecDeque::new();
        let mut tree = AnalysisTree::new();
        let mut first_level_count: usize = 0;
        let mut deeper_count: usize = 0;

        let cache = ColumnCache::new(df, schema)?;
        let ctx = ReviewCtx {
            df,
            schema,
            cache: &cache,
        };

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
                    self.detect_anomalies_arm(&ctx, &column, &mut tree, &mut queue)?;
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
                    self.attribute_segment_arm(
                        &ctx,
                        AttributionTask {
                            parent_id,
                            target_col: &target_col,
                            segment_col: &segment_col,
                            depth,
                        },
                        &mut tree,
                        &mut queue,
                    )?;
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
                    self.search_correlations_arm(&ctx, parent_id, &target_col, &mut tree)?;
                },
                _ => {}, // Ignore other task types in review
            }
        }

        // Dedup, then diversity-select the top roots
        dedup::dedup(&mut tree);
        crate::analysis::history::apply_novelty(&mut tree, &self.history, self.now_epoch);
        crate::analysis::select::select_top(&mut tree, self.select_top);

        Ok(AnalysisResult {
            tree,
            first_level_count,
            deeper_count,
        })
    }

    /// Latest-value anomaly on a raw (row-ordered) measure; queues segment
    /// attribution when one is found.
    fn detect_anomalies_arm(
        &self,
        ctx: &ReviewCtx<'_>,
        column: &str,
        tree: &mut AnalysisTree,
        queue: &mut VecDeque<AnalysisTask>,
    ) -> Result<()> {
        let Some(anomaly) = detect_anomaly(ctx.df, column)? else {
            return Ok(());
        };
        if anomaly.z_score.abs() <= self.z_threshold {
            return Ok(());
        }
        let description = format!(
            "Anomaly detected in '{}': latest value {:.2} is {:.1} std devs {} the mean ({:.2})",
            column,
            anomaly.value,
            anomaly.z_score.abs(),
            if anomaly.z_score > 0.0 {
                "above"
            } else {
                "below"
            },
            anomaly.mean
        );

        let analysis = AnalysisType::Anomaly {
            column: anomaly.column.clone(),
            value: anomaly.value,
            mean: anomaly.mean,
            std_dev: anomaly.std_dev,
            z_score: anomaly.z_score,
        };
        let series_data = ctx
            .cache
            .numeric
            .get(column)
            .map(|values| build_anomaly_series(values, anomaly.mean, anomaly.std_dev));
        let Some(node_id) = add_scored_root(
            tree,
            &self.scoring_ctx,
            analysis,
            description,
            series_data,
            RootMeta {
                detector: "raw_anomaly",
                measure: measure_ref(column),
                agg: Aggregation::Mean,
                dimension: None,
                granularity: "row",
                why: format!(
                    "covers the whole table; typical values sit within 2 standard deviations of the mean — the latest is {:.1} away",
                    anomaly.z_score.abs()
                ),
            },
        ) else {
            return Ok(());
        };

        // Spawn attribution tasks
        for cat_col in &ctx.schema.dimension_columns {
            queue.push_back(AnalysisTask::AttributeSegment {
                parent_id: node_id,
                target_col: column.to_string(),
                segment_col: cat_col.clone(),
                depth: 1,
            });
        }
        Ok(())
    }

    /// Attribute a parent anomaly to one segment value; queues a correlation
    /// search under the new child.
    fn attribute_segment_arm(
        &self,
        ctx: &ReviewCtx<'_>,
        task: AttributionTask<'_>,
        tree: &mut AnalysisTree,
        queue: &mut VecDeque<AnalysisTask>,
    ) -> Result<()> {
        let Some(attr) = attribute_segment(ctx.df, task.target_col, task.segment_col)? else {
            return Ok(());
        };
        if attr.p_value >= self.p_threshold {
            return Ok(());
        }
        let description = format!(
            "Segment '{}' = '{}' contributes {:.2} to '{}' (p={:.4})",
            task.segment_col, attr.segment_value, attr.contribution, task.target_col, attr.p_value
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
            tree,
            task.parent_id,
            analysis,
            description,
            &self.scoring_ctx,
        ) {
            let parent_chain = tree.nodes[task.parent_id.0].filter_chain.clone();
            let mut chain = parent_chain;
            chain.push(crate::analysis::tree::FilterStep {
                column: attr.segment_column.clone(),
                op: "eq".to_string(),
                value: attr.segment_value,
            });
            tree.set_filter_chain(node_id, chain);
            queue.push_back(AnalysisTask::SearchCorrelations {
                parent_id: node_id,
                target_col: task.target_col.to_string(),
                depth: task.depth + 1,
            });
        }
        Ok(())
    }

    /// Correlate the target against every other analyzable column, attaching
    /// strong pairs as children.
    fn search_correlations_arm(
        &self,
        ctx: &ReviewCtx<'_>,
        parent_id: NodeId,
        target_col: &str,
        tree: &mut AnalysisTree,
    ) -> Result<()> {
        for other_col in &ctx.schema.analyzable_columns() {
            if other_col.as_str() == target_col {
                continue;
            }
            let Some(corr) = correlate(ctx.df, target_col, other_col)? else {
                continue;
            };
            if !(corr.p_value < self.p_threshold && corr.r_value.abs() > 0.5) {
                continue;
            }
            let description = format!(
                "Correlation between '{}' and '{}': r={:.3} (p={:.4})",
                target_col, other_col, corr.r_value, corr.p_value
            );
            let xs = ctx.cache.numeric.get(target_col).cloned();
            let ys = ctx.cache.numeric.get(other_col).cloned();
            let scatter_data = match (xs, ys) {
                (Some(x), Some(y)) => Some(build_scatter_data(
                    &x,
                    &y,
                    target_col,
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
        Ok(())
    }
}
