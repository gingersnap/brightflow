//! Analysis engine: report orchestration.
//!
//! Split by concern: `cache` (column extraction), `review` (what changed
//! lately), `trends` (what changes over time), `payloads` (chart data).

mod cache;
mod drivers;
mod meta;
mod payloads;
mod pipeline;
mod review;
mod trends;

pub use cache::ColumnCache;

use anyhow::Result;
use polars::prelude::*;

use std::collections::HashMap;

use crate::analysis::history::HistoryEntry;
use crate::analysis::scoring::{self, ScoringContext};
use crate::analysis::tree::{
    AnalysisResult, AnalysisTree, AnalysisType, NodeId, ReportType, ReviewCadence,
};
use crate::data::config::TimeGranularity;
use crate::data::schema::DataSchema;
use crate::debug::DebugLog;

pub struct AnalysisEngine {
    z_threshold: f64,
    p_threshold: f64,
    max_depth: usize,
    scoring_ctx: ScoringContext,
    /// Diversity-selected top-N returned as roots (see `analysis::select`).
    select_top: usize,
    /// Prior exposure per insight fingerprint (see `analysis::history`).
    history: HashMap<String, HistoryEntry>,
    /// "Now" for novelty decay, unix epoch seconds.
    now_epoch: i64,
}

/// Return the latest two distinct period labels (previous, current) sorted by
/// string order. Shared by the trends and drivers passes.
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
            select_top: 50,
            history: HashMap::new(),
            now_epoch: 0,
        }
    }

    pub fn with_select_top(mut self, n: usize) -> Self {
        self.select_top = n.max(1);
        self
    }

    /// Provide prior insight exposure for novelty decay. The API layer loads
    /// this from `insight_history`; the engine itself never touches the DB.
    pub fn with_history(mut self, history: HashMap<String, HistoryEntry>, now_epoch: i64) -> Self {
        self.history = history;
        self.now_epoch = now_epoch;
        self
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
}
