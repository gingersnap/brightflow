//! The analysis tree: the structure a report is built and rendered from.
//!
//! Findings are nodes with parents rather than a flat list, because the useful
//! output is "revenue fell, and here is the segment responsible" — a hierarchy of
//! explanation. The three `ReportType`s (Review, Drivers, Trends) are three
//! traversals of the same generators, not three separate engines.

use serde::Serialize;
use ts_rs::TS;

/// Report types that can be generated
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportType {
    /// How are we doing? What happened? Why?
    Review,
    /// What is driving our performance?
    Drivers,
    /// What's changing over time? Where is it going?
    Trends,
}

impl ReportType {
    pub fn suffix(&self) -> &'static str {
        match self {
            Self::Review => "review",
            Self::Drivers => "drivers",
            Self::Trends => "trends",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::Review => "Review",
            Self::Drivers => "Drivers",
            Self::Trends => "Trends & Forecast",
        }
    }
}

/// Review cadence - determines what period to analyze and compare against
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewCadence {
    /// Daily checkpoint: today vs yesterday
    Daily,
    /// Weekly business review: this week vs last week
    Weekly,
    /// Monthly business review: this month vs last month
    Monthly,
}

impl ReviewCadence {
    pub fn suffix(&self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::Daily => "Daily Review",
            Self::Weekly => "Weekly Review",
            Self::Monthly => "Monthly Review",
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::Daily, Self::Weekly, Self::Monthly]
    }
}

/// High-level categories for organizing analysis findings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, TS)]
#[ts(export)]
pub enum AnalysisCategory {
    /// "How are we doing vs expectations?"
    Performance,
    /// "What's changing over time?"
    Trends,
    /// "What makes up this KPI?" (structural composition)
    Drivers,
    /// "Why did this change happen?" (diagnostic explanation)
    RootCause,
}

impl AnalysisCategory {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Performance => "Performance",
            Self::Trends => "Trends",
            Self::Drivers => "Drivers",
            Self::RootCause => "Root Cause",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Performance => "How are we doing vs expectations?",
            Self::Trends => "What's changing over time?",
            Self::Drivers => "What makes up this KPI?",
            Self::RootCause => "Why did this change happen?",
        }
    }

    /// Returns all categories in display order
    pub fn all() -> &'static [Self] {
        &[
            Self::Performance,
            Self::Trends,
            Self::Drivers,
            Self::RootCause,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, TS)]
#[ts(export, type = "number")]
pub struct NodeId(pub usize);

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisTree {
    pub nodes: Vec<AnalysisNode>,
    pub roots: Vec<NodeId>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisNode {
    pub id: NodeId,
    pub parent_id: Option<NodeId>,
    pub analysis: AnalysisType,
    /// Final composite score — kept under the legacy name because the
    /// frontend sorts on it. Equal to `final_score(score_breakdown)`.
    pub significance: f64,
    /// Calibrated component scores — exposed for the UI explainer
    pub score_breakdown: ScoreBreakdown,
    /// Technical description with statistics (for data scientists)
    pub description: String,
    /// Natural language summary for non-technical readers
    pub summary: String,
    /// Technical summary with statistical notation
    pub tech_summary: String,
    /// Why this finding is interesting, in plain language
    /// ("affects 34% of rows; a stable series would show this <1% of the time")
    #[serde(default)]
    pub why: String,
    /// How the underlying series was derived (measure → filters → derivations)
    #[serde(default)]
    pub provenance: Vec<ProvenanceStep>,
    /// Composition depth: 1 = bare aggregate, +1 per filter/derivation
    pub depth: u8,
    /// Stable story fingerprint (see `analysis::fingerprint`) — keys history,
    /// dismissals, and suppressions
    #[serde(default)]
    pub fingerprint: String,
    /// 1-based position after diversity selection; None for non-root nodes
    #[ts(optional)]
    pub rank: Option<u32>,
    /// Whether this movement is good or bad news, from measure polarity.
    /// Absent = neutral / unknown (see `analysis::polarity`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sentiment: Option<Sentiment>,
    pub children: Vec<NodeId>,
    /// Optional payload of underlying data needed by per-type renderers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<NodeData>,
    /// Filter chain implied by this finding's drill path — used to open the finding in Explore
    #[serde(default)]
    pub filter_chain: Vec<FilterStep>,
}

/// One stage of a derived series' recipe, rendered as a chip in the UI.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceStep {
    /// "measure" | "filter" | "derive"
    pub kind: String,
    pub label: String,
}

/// Interestingness = Impact × Significance (× Novelty × KPI boost).
///
/// `final_score = significance · √impact · (0.5 + 0.5·novelty) · kpi_boost`
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ScoreBreakdown {
    /// 1 − p under the detector's type-specific null, in [0, 1]
    pub significance: f64,
    /// Share of total data volume the finding affects, in [0, 1]
    pub impact: f64,
    /// 1.0 = never shown before; decays with history (wired in 1C)
    pub novelty: f64,
    pub kpi_boost: f64,
}

impl ScoreBreakdown {
    pub fn empty() -> Self {
        Self {
            significance: 0.0,
            impact: 0.0,
            novelty: 1.0,
            kpi_boost: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FilterStep {
    pub column: String,
    pub op: String,
    pub value: String,
}

/// Renderer payload — small data slices attached so frontend can draw a chart
/// without a second round-trip. Series are downsampled to ≤200 points.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(tag = "type")]
pub enum NodeData {
    /// Time-series for the column with optional comparison band
    Series {
        labels: Vec<String>,
        values: Vec<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        band_low: Option<Vec<f64>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        band_high: Option<Vec<f64>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        marker_index: Option<usize>,
        /// Human y-axis label (measure name), when known
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        y_label: Option<String>,
    },
    /// Time-series with a fitted regression line overlay
    SeriesWithFit {
        labels: Vec<String>,
        values: Vec<f64>,
        fit: Vec<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        y_label: Option<String>,
    },
    /// Two periods compared, paired bars per category
    PairedBars {
        labels: Vec<String>,
        previous: Vec<f64>,
        current: Vec<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        y_label: Option<String>,
    },
    /// Ranked horizontal bars (one per segment value)
    SegmentBars {
        labels: Vec<String>,
        values: Vec<f64>,
        contributions_pct: Vec<f64>,
        /// Human label of the plotted value (measure or "share %")
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        value_label: Option<String>,
    },
    /// Scatter plot points with optional fit line
    Scatter {
        x: Vec<f64>,
        y: Vec<f64>,
        x_label: String,
        y_label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        fit_slope: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fit_intercept: Option<f64>,
    },
    /// Forecast cone: history + extrapolated point with prediction interval
    Forecast {
        labels: Vec<String>,
        history: Vec<f64>,
        expected: f64,
        actual: f64,
        pi_low: f64,
        pi_high: f64,
    },
    /// Multiple sparklines (one per affected column) for cluster findings
    Multi {
        labels: Vec<String>,
        series: Vec<NamedSeries>,
        marker_index: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        y_label: Option<String>,
    },
    /// Histogram pair for distribution shift
    HistogramPair {
        bin_edges: Vec<f64>,
        previous: Vec<f64>,
        current: Vec<f64>,
    },
    /// Lorenz / concentration curve
    Lorenz {
        cumulative_share: Vec<f64>,
        cumulative_population: Vec<f64>,
        gini: f64,
    },
    /// Before/after diff of dimension membership
    MembershipDiff {
        added: Vec<String>,
        removed: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct NamedSeries {
    pub name: String,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(tag = "type")]
pub enum AnalysisType {
    Anomaly {
        column: String,
        value: f64,
        mean: f64,
        std_dev: f64,
        z_score: f64,
    },
    Segment {
        target_column: String,
        segment_column: String,
        segment_value: String,
        contribution: f64,
        change_percent: f64,
        /// What percentage of the total change this segment explains
        contribution_pct: f64,
        p_value: f64,
    },
    Correlation {
        column_a: String,
        column_b: String,
        r_value: f64,
        p_value: f64,
    },
    Trend {
        column: String,
        direction: TrendDirection,
        slope: f64,
        r_squared: f64,
        p_value: f64,
    },
    /// Period-over-period comparison (e.g., this week vs last week)
    PeriodComparison {
        column: String,
        current_period: String,
        previous_period: String,
        current_value: f64,
        previous_value: f64,
        change_percent: f64,
        p_value: f64,
    },
    /// A specific period that deviates significantly from others
    PeriodAnomaly {
        column: String,
        period: String,
        period_value: f64,
        other_periods_mean: f64,
        change_percent: f64,
        p_value: f64,
    },
    /// Recurring pattern detected in time-series data
    Seasonality {
        column: String,
        period_name: String, // "weekly", "monthly", "yearly"
        autocorrelation: f64,
        p_value: f64,
    },
    /// Multiple columns with outliers in the same period
    OutlierCluster {
        period: String,
        columns: Vec<String>,
        direction: String, // "spike" or "dip"
        #[ts(type = "number")]
        cluster_size: usize,
        /// How many numeric columns were scanned (binomial-null denominator)
        #[ts(type = "number")]
        columns_tested: usize,
        /// How many periods were scanned (multiple-comparison correction)
        #[ts(type = "number")]
        n_periods: usize,
    },
    /// Actual value deviates from historical trend forecast
    ForecastDeviation {
        column: String,
        period: String,
        actual: f64,
        expected: f64,
        deviation_percent: f64,
        p_value: f64,
    },
    /// Concentration alert: a small number of segment values dominate the metric
    Concentration {
        column: String,
        segment_column: String,
        /// Herfindahl-Hirschman index ([0, 1]) — higher = more concentrated
        hhi: f64,
        /// Top-N share (e.g. top 3 contribute X% of total)
        top_n: usize,
        top_share: f64,
        /// Optional change vs prior baseline
        #[ts(optional)]
        hhi_delta: Option<f64>,
        /// Number of distinct segment values (uniform-null denominator)
        #[ts(type = "number")]
        n_segments: usize,
        /// Number of contributing rows (pseudo-count mass for the null)
        #[ts(type = "number")]
        n_rows: usize,
    },
    /// Distribution of a metric shifted between two windows (KS-test)
    DistributionShift {
        column: String,
        previous_period: String,
        current_period: String,
        /// KS statistic ([0, 1])
        ks_statistic: f64,
        p_value: f64,
    },
    /// New or disappeared dimension values between two periods
    MembershipChange {
        segment_column: String,
        previous_period: String,
        current_period: String,
        added_count: usize,
        removed_count: usize,
        /// Membership size in the previous period (churn-null exposure)
        #[ts(type = "number")]
        prev_size: usize,
        /// Membership size in the current period
        #[ts(type = "number")]
        curr_size: usize,
    },
    /// Change-point in a time series (level shift)
    ChangePoint {
        column: String,
        /// Period label where the level shift starts
        period: String,
        before_mean: f64,
        after_mean: f64,
        /// Cumulative sum statistic at the change point
        cusum: f64,
        /// Synthetic p-value derived from |cusum| / σ
        p_value: f64,
    },
    /// A segment's volume rank among its dimension's siblings changed
    RankChange {
        /// Measure being ranked (usually row volume)
        measure: String,
        dimension: String,
        value: String,
        #[ts(type = "number")]
        previous_rank: usize,
        #[ts(type = "number")]
        new_rank: usize,
        /// Number of sibling values in the ranking
        #[ts(type = "number")]
        n_siblings: usize,
        p_value: f64,
    },
    /// The top value of a dimension holds more share than its rank
    /// distribution predicts (power-law null)
    TopDominance {
        measure: String,
        dimension: String,
        value: String,
        /// Observed share of the leader, in [0, 1]
        share: f64,
        /// Share the power-law fit over ranks 2..k predicts for rank 1
        expected_share: f64,
        #[ts(type = "number")]
        n_values: usize,
        p_value: f64,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub enum TrendDirection {
    Increasing,
    Decreasing,
}

/// Good/bad framing of a finding, derived from measure polarity.
#[derive(Debug, Clone, Copy, Serialize, TS, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum Sentiment {
    Good,
    Bad,
}

/// Convert column name to human-readable label
fn humanize_column(name: &str) -> String {
    name.replace('_', " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Format a measure value for prose: thousands separators and
/// magnitude-aware precision (1,234,567 / 12.34 / 0.0042). Statistical
/// quantities (p, r, z, R²) keep their fixed-precision formatting — this is
/// for values users compare against their own mental numbers.
pub(crate) fn format_value(value: f64) -> String {
    if !value.is_finite() {
        return format!("{value}");
    }
    let abs = value.abs();
    if abs >= 1000.0 {
        let rounded = value.round();
        let negative = rounded < 0.0;
        let digits = format!("{}", rounded.abs() as u64);
        let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                grouped.push(',');
            }
            grouped.push(c);
        }
        if negative {
            format!("-{grouped}")
        } else {
            grouped
        }
    } else if abs > 0.0 && abs < 0.01 {
        format!("{value:.4}")
    } else if (value - value.round()).abs() < 1e-9 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.2}")
    }
}

/// "value" / "values" — tiny, but "1 values" reads broken.
pub(crate) fn pluralize(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("{count} {singular}")
    } else {
        format!("{count} {plural}")
    }
}

/// Format a period label for natural language
pub(crate) fn humanize_period(period: &str) -> String {
    // Handle formats like "2023-03", "2023-W12", "2023-Q1", "2023"
    if period.contains("-W") {
        let parts: Vec<&str> = period.split("-W").collect();
        if let (Some(year), Some(week)) = (parts.first(), parts.get(1)) {
            return format!("week {week} of {year}");
        }
    } else if period.contains("-Q") {
        let parts: Vec<&str> = period.split("-Q").collect();
        if let (Some(year), Some(quarter)) = (parts.first(), parts.get(1)) {
            return format!("Q{quarter} {year}");
        }
    } else if period.len() == 7 && period.contains('-') {
        // "2023-03" format
        let parts: Vec<&str> = period.split('-').collect();
        if let (Some(year), Some(month_num)) = (parts.first(), parts.get(1)) {
            let month_name = match *month_num {
                "01" => "January",
                "02" => "February",
                "03" => "March",
                "04" => "April",
                "05" => "May",
                "06" => "June",
                "07" => "July",
                "08" => "August",
                "09" => "September",
                "10" => "October",
                "11" => "November",
                "12" => "December",
                _ => month_num,
            };
            return format!("{month_name} {year}");
        }
    }
    period.to_string()
}

impl AnalysisType {
    /// Stable machine name of the variant (persisted in insight history).
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Anomaly { .. } => "anomaly",
            Self::Segment { .. } => "segment",
            Self::Correlation { .. } => "correlation",
            Self::Trend { .. } => "trend",
            Self::PeriodComparison { .. } => "period_comparison",
            Self::PeriodAnomaly { .. } => "period_anomaly",
            Self::Seasonality { .. } => "seasonality",
            Self::OutlierCluster { .. } => "outlier_cluster",
            Self::ForecastDeviation { .. } => "forecast_deviation",
            Self::Concentration { .. } => "concentration",
            Self::DistributionShift { .. } => "distribution_shift",
            Self::MembershipChange { .. } => "membership_change",
            Self::ChangePoint { .. } => "change_point",
            Self::RankChange { .. } => "rank_change",
            Self::TopDominance { .. } => "top_dominance",
        }
    }

    /// Returns the category this analysis type belongs to
    pub fn category(&self) -> AnalysisCategory {
        match self {
            // Performance - "How are we doing vs expectations?"
            Self::Anomaly { .. } | Self::ForecastDeviation { .. } => AnalysisCategory::Performance,

            // Trends - "What's changing over time?"
            Self::Trend { .. }
            | Self::Seasonality { .. }
            | Self::PeriodComparison { .. }
            | Self::PeriodAnomaly { .. }
            | Self::OutlierCluster { .. }
            | Self::DistributionShift { .. }
            | Self::ChangePoint { .. }
            | Self::RankChange { .. }
            | Self::MembershipChange { .. } => AnalysisCategory::Trends,

            // Root Cause - "Why did this change happen?"
            Self::Segment { .. } | Self::Correlation { .. } => AnalysisCategory::RootCause,
            // Drivers - "What makes up this KPI?"
            Self::Concentration { .. } | Self::TopDominance { .. } => AnalysisCategory::Drivers,
        }
    }

    /// Generate a technical summary with statistical notation
    pub fn tech_summary(&self) -> String {
        match self {
            Self::Anomaly {
                column,
                value,
                z_score,
                ..
            } => {
                format!("{column}: value={value:.2}, z={z_score:.2}")
            },
            Self::Segment {
                segment_column,
                segment_value,
                change_percent,
                contribution_pct,
                p_value,
                ..
            } => {
                format!(
                    "{segment_column}=\"{segment_value}\": Δ={change_percent:.1}%, contribution={contribution_pct:.1}%, p={p_value:.4}"
                )
            },
            Self::Correlation {
                column_a,
                column_b,
                r_value,
                p_value,
            } => {
                format!("corr({column_a}, {column_b}): r={r_value:.3}, p={p_value:.4}")
            },
            Self::Trend {
                column,
                direction,
                slope,
                r_squared,
                p_value,
            } => {
                let dir = match direction {
                    TrendDirection::Increasing => "↑",
                    TrendDirection::Decreasing => "↓",
                };
                format!("{column} {dir}: slope={slope:.4}, R²={r_squared:.3}, p={p_value:.4}")
            },
            Self::PeriodComparison {
                column,
                current_period,
                previous_period,
                change_percent,
                p_value,
                ..
            } => {
                format!(
                    "{column}: {current_period} vs {previous_period} Δ={change_percent:.1}%, p={p_value:.4}"
                )
            },
            Self::PeriodAnomaly {
                column,
                period,
                change_percent,
                p_value,
                ..
            } => {
                format!("{column} [{period}]: Δ={change_percent:.1}% vs mean, p={p_value:.4}")
            },
            Self::Seasonality {
                column,
                period_name,
                autocorrelation,
                p_value,
            } => {
                format!(
                    "{column}: {period_name} seasonality, r={autocorrelation:.3}, p={p_value:.4}"
                )
            },
            Self::OutlierCluster {
                period,
                columns,
                direction,
                ..
            } => {
                let cols = columns.join(", ");
                format!("[{period}] {direction} cluster: {cols}")
            },
            Self::ForecastDeviation {
                column,
                period,
                actual,
                expected,
                deviation_percent,
                p_value,
            } => {
                format!(
                    "{column} [{period}]: actual={actual:.2}, expected={expected:.2}, Δ={deviation_percent:.1}%, p={p_value:.4}"
                )
            },
            Self::Concentration {
                column,
                segment_column,
                hhi,
                top_n,
                top_share,
                ..
            } => {
                format!("{column} by {segment_column}: HHI={hhi:.2}, top {top_n} = {top_share:.1}%")
            },
            Self::DistributionShift {
                column,
                previous_period,
                current_period,
                ks_statistic,
                p_value,
            } => {
                format!(
                    "{column} [{previous_period} → {current_period}]: KS={ks_statistic:.3}, p={p_value:.4}"
                )
            },
            Self::MembershipChange {
                segment_column,
                previous_period,
                current_period,
                added_count,
                removed_count,
                ..
            } => {
                format!(
                    "{segment_column} [{previous_period} → {current_period}]: +{added_count}, -{removed_count}"
                )
            },
            Self::ChangePoint {
                column,
                period,
                before_mean,
                after_mean,
                p_value,
                ..
            } => {
                format!(
                    "{column} change-point @ {period}: {before_mean:.1} → {after_mean:.1}, p={p_value:.4}"
                )
            },
            Self::RankChange {
                measure,
                dimension,
                value,
                previous_rank,
                new_rank,
                n_siblings,
                p_value,
            } => {
                format!(
                    "{dimension}=\"{value}\" [{measure}]: rank {previous_rank} → {new_rank} of {n_siblings}, p={p_value:.4}"
                )
            },
            Self::TopDominance {
                measure,
                dimension,
                value,
                share,
                expected_share,
                p_value,
                ..
            } => {
                format!(
                    "{dimension}=\"{value}\" [{measure}]: share {:.1}% vs {:.1}% expected, p={p_value:.4}",
                    share * 100.0,
                    expected_share * 100.0
                )
            },
        }
    }

    /// Generate a natural language summary
    pub fn natural_summary(&self) -> String {
        match self {
            Self::Anomaly {
                column,
                value,
                mean,
                z_score,
                ..
            } => {
                let col = humanize_column(column);
                let direction = if *z_score > 0.0 {
                    "unusually high"
                } else {
                    "unusually low"
                };
                format!(
                    "{col} is {direction} at {} (typically around {})",
                    format_value(*value),
                    format_value(*mean)
                )
            },
            Self::Segment {
                target_column,
                segment_column,
                segment_value,
                change_percent,
                contribution_pct,
                ..
            } => {
                let target = humanize_column(target_column);
                let segment = humanize_column(segment_column);
                let direction = if *change_percent > 0.0 { "up" } else { "down" };
                let change_abs = change_percent.abs();
                let contrib_abs = contribution_pct.abs();
                // Format: "Ship Mode = Second Class was up 142%, contributing 35% of total Sales change"
                if contribution_pct.abs() > 0.1 {
                    format!(
                        "{segment} = \"{segment_value}\" was {direction} {change_abs:.0}%, contributing {contrib_abs:.0}% of total {target} change"
                    )
                } else {
                    // Fallback for non-period attributions where contribution_pct isn't meaningful
                    let dir_verb = if *change_percent > 0.0 {
                        "driving up"
                    } else {
                        "pulling down"
                    };
                    format!("{segment} = \"{segment_value}\" is {dir_verb} {target} by {change_abs:.1}%")
                }
            },
            Self::Correlation {
                column_a,
                column_b,
                r_value,
                ..
            } => {
                let a = humanize_column(column_a);
                let b = humanize_column(column_b);
                let strength = if r_value.abs() > 0.8 {
                    "strongly"
                } else if r_value.abs() > 0.6 {
                    "moderately"
                } else {
                    "somewhat"
                };
                let direction = if *r_value > 0.0 {
                    "positively"
                } else {
                    "negatively"
                };
                format!("{a} and {b} are {strength} {direction} correlated")
            },
            Self::Trend {
                column, direction, ..
            } => {
                let col = humanize_column(column);
                let dir = match direction {
                    TrendDirection::Increasing => "increasing",
                    TrendDirection::Decreasing => "decreasing",
                };
                format!("{col} is consistently {dir}")
            },
            Self::PeriodComparison {
                column,
                current_period,
                previous_period,
                current_value,
                previous_value,
                change_percent,
                ..
            } => {
                let col = humanize_column(column);
                let current = humanize_period(current_period);
                let previous = humanize_period(previous_period);
                let direction = if *change_percent > 0.0 {
                    "increased"
                } else {
                    "decreased"
                };
                let change_abs = change_percent.abs();
                format!(
                    "{col} {direction} by {change_abs:.1}% ({} → {}) in {current} compared to {previous}",
                    format_value(*previous_value),
                    format_value(*current_value)
                )
            },
            Self::PeriodAnomaly {
                column,
                period,
                period_value,
                other_periods_mean,
                change_percent,
                ..
            } => {
                let col = humanize_column(column);
                let p = humanize_period(period);
                let direction = if *change_percent > 0.0 {
                    "higher"
                } else {
                    "lower"
                };
                let change_abs = change_percent.abs();
                format!(
                    "{col} was {change_abs:.1}% {direction} in {p} ({} vs avg {})",
                    format_value(*period_value),
                    format_value(*other_periods_mean)
                )
            },
            Self::Seasonality {
                column,
                period_name,
                autocorrelation,
                ..
            } => {
                let col = humanize_column(column);
                let strength = if autocorrelation.abs() > 0.8 {
                    "strong"
                } else if autocorrelation.abs() > 0.6 {
                    "moderate"
                } else {
                    "weak"
                };
                format!("{col} repeats on a {period_name} cycle ({strength} pattern)")
            },
            Self::OutlierCluster {
                period,
                columns,
                direction,
                ..
            } => {
                let p = humanize_period(period);
                let cols = columns
                    .iter()
                    .map(|c| humanize_column(c))
                    .collect::<Vec<_>>()
                    .join(", ");
                let dir_word = if direction == "spike" { "Spike" } else { "Dip" };
                format!("{p}: {dir_word} cluster in {cols}")
            },
            Self::ForecastDeviation {
                column,
                period,
                actual,
                expected,
                deviation_percent,
                ..
            } => {
                let col = humanize_column(column);
                let p = humanize_period(period);
                let direction = if *deviation_percent > 0.0 { "+" } else { "" };
                format!(
                    "{col} in {p}: Actual {} vs Expected {} ({direction}{deviation_percent:.0}% deviation)",
                    format_value(*actual),
                    format_value(*expected)
                )
            },
            Self::Concentration {
                column,
                segment_column,
                top_n,
                top_share,
                ..
            } => {
                let c = humanize_column(column);
                let s = humanize_column(segment_column);
                let (values_word, verb) = if *top_n == 1 {
                    ("value", "accounts")
                } else {
                    ("values", "account")
                };
                format!(
                    "{c} is concentrated — the top {top_n} {s} {values_word} {verb} for {top_share:.0}% of the total"
                )
            },
            Self::DistributionShift {
                column,
                previous_period,
                current_period,
                ..
            } => {
                let c = humanize_column(column);
                let prev = humanize_period(previous_period);
                let curr = humanize_period(current_period);
                format!("{c} distribution shifted from {prev} to {curr}")
            },
            Self::MembershipChange {
                segment_column,
                added_count,
                removed_count,
                current_period,
                ..
            } => {
                let s = humanize_column(segment_column);
                let p = humanize_period(current_period);
                format!(
                    "{s} membership changed in {p}: {} new, {} disappeared",
                    pluralize(*added_count, "value", "values"),
                    pluralize(*removed_count, "value", "values")
                )
            },
            Self::ChangePoint {
                column,
                period,
                before_mean,
                after_mean,
                ..
            } => {
                let c = humanize_column(column);
                let p = humanize_period(period);
                let direction = if after_mean > before_mean {
                    "stepped up"
                } else {
                    "stepped down"
                };
                format!(
                    "{c} {direction} at {p} ({} → {})",
                    format_value(*before_mean),
                    format_value(*after_mean)
                )
            },
            Self::RankChange {
                dimension,
                value,
                previous_rank,
                new_rank,
                n_siblings,
                ..
            } => {
                let d = humanize_column(dimension);
                let direction = if new_rank < previous_rank {
                    "climbed"
                } else {
                    "dropped"
                };
                format!(
                    "{d} \"{value}\" {direction} from #{previous_rank} to #{new_rank} of {n_siblings} by volume"
                )
            },
            Self::TopDominance {
                dimension,
                value,
                share,
                expected_share,
                ..
            } => {
                let d = humanize_column(dimension);
                format!(
                    "{d} \"{value}\" holds {:.0}% of the volume — far above the {:.0}% its peers' drop-off suggests",
                    share * 100.0,
                    expected_share * 100.0
                )
            },
        }
    }
}

impl AnalysisTree {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            roots: Vec::new(),
        }
    }

    pub fn add_root(
        &mut self,
        analysis: AnalysisType,
        significance: f64,
        description: String,
    ) -> NodeId {
        self.add_root_full(
            analysis,
            significance,
            ScoreBreakdown::empty(),
            description,
            None,
        )
    }

    pub fn add_root_full(
        &mut self,
        analysis: AnalysisType,
        significance: f64,
        score_breakdown: ScoreBreakdown,
        description: String,
        data: Option<NodeData>,
    ) -> NodeId {
        let summary = analysis.natural_summary();
        let tech_summary = analysis.tech_summary();
        let id = NodeId(self.nodes.len());
        let node = AnalysisNode {
            id,
            parent_id: None,
            analysis,
            significance,
            score_breakdown,
            description,
            summary,
            tech_summary,
            why: String::new(),
            provenance: Vec::new(),
            depth: 1,
            fingerprint: String::new(),
            rank: None,
            sentiment: None,
            children: Vec::new(),
            data,
            filter_chain: Vec::new(),
        };
        self.nodes.push(node);
        self.roots.push(id);
        id
    }

    /// Attach insight metadata (why-interesting, provenance recipe, depth,
    /// stable fingerprint) to a node after creation.
    pub fn set_insight_meta(
        &mut self,
        id: NodeId,
        why: String,
        provenance: Vec<ProvenanceStep>,
        depth: u8,
        fingerprint: String,
    ) {
        if let Some(n) = self.nodes.get_mut(id.0) {
            n.why = why;
            n.provenance = provenance;
            n.depth = depth;
            n.fingerprint = fingerprint;
        }
    }

    pub fn add_child(
        &mut self,
        parent_id: NodeId,
        analysis: AnalysisType,
        significance: f64,
        description: String,
    ) -> NodeId {
        self.add_child_full(
            parent_id,
            analysis,
            significance,
            ScoreBreakdown::empty(),
            description,
            None,
        )
    }

    pub fn add_child_full(
        &mut self,
        parent_id: NodeId,
        analysis: AnalysisType,
        significance: f64,
        score_breakdown: ScoreBreakdown,
        description: String,
        data: Option<NodeData>,
    ) -> NodeId {
        let summary = analysis.natural_summary();
        let tech_summary = analysis.tech_summary();
        let id = NodeId(self.nodes.len());
        let parent_chain = self.nodes[parent_id.0].filter_chain.clone();
        let parent_depth = self.nodes[parent_id.0].depth;
        let node = AnalysisNode {
            id,
            parent_id: Some(parent_id),
            analysis,
            significance,
            score_breakdown,
            description,
            summary,
            tech_summary,
            why: String::new(),
            provenance: Vec::new(),
            depth: parent_depth.saturating_add(1),
            fingerprint: String::new(),
            rank: None,
            sentiment: None,
            children: Vec::new(),
            data,
            filter_chain: parent_chain,
        };
        self.nodes.push(node);
        self.nodes[parent_id.0].children.push(id);
        id
    }

    /// Replace the filter_chain of a node (used to extend drill paths)
    pub fn set_filter_chain(&mut self, id: NodeId, chain: Vec<FilterStep>) {
        if let Some(n) = self.nodes.get_mut(id.0) {
            n.filter_chain = chain;
        }
    }
}

impl Default for AnalysisTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Wraps an analysis tree with execution statistics
pub struct AnalysisResult {
    pub tree: AnalysisTree,
    pub first_level_count: usize,
    pub deeper_count: usize,
}
