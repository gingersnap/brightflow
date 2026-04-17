use serde::Serialize;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisTree {
    pub nodes: Vec<AnalysisNode>,
    pub roots: Vec<NodeId>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisNode {
    pub id: NodeId,
    pub parent_id: Option<NodeId>,
    pub analysis: AnalysisType,
    pub significance: f64,
    /// Technical description with statistics (for data scientists)
    pub description: String,
    /// Natural language summary for non-technical readers
    pub summary: String,
    /// Technical summary with statistical notation
    pub tech_summary: String,
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone, Serialize)]
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
        cluster_size: usize,
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
}

#[derive(Debug, Clone, Serialize)]
pub enum TrendDirection {
    Increasing,
    Decreasing,
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

/// Format a period label for natural language
fn humanize_period(period: &str) -> String {
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
            | Self::OutlierCluster { .. } => AnalysisCategory::Trends,

            // Root Cause - "Why did this change happen?"
            Self::Segment { .. } | Self::Correlation { .. } => AnalysisCategory::RootCause,
            // Note: Drivers category analyses (SegmentBreakdown, Pareto, etc.)
            // will be added here in the future
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
                format!("{col} is {direction} at {value:.2} (typically around {mean:.2})")
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
                    "{col} {direction} by {change_abs:.1}% ({previous_value:.1} → {current_value:.1}) in {current} compared to {previous}"
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
                format!("{col} was {change_abs:.1}% {direction} in {p} ({period_value:.1} vs avg {other_periods_mean:.1})")
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
                format!("{col} shows {strength} {period_name} seasonality (r={autocorrelation:.2})")
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
                    "{col} in {p}: Actual {actual:.1} vs Expected {expected:.1} ({direction}{deviation_percent:.0}% deviation)"
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
        let summary = analysis.natural_summary();
        let tech_summary = analysis.tech_summary();
        let id = NodeId(self.nodes.len());
        let node = AnalysisNode {
            id,
            parent_id: None,
            analysis,
            significance,
            description,
            summary,
            tech_summary,
            children: Vec::new(),
        };
        self.nodes.push(node);
        self.roots.push(id);
        id
    }

    pub fn add_child(
        &mut self,
        parent_id: NodeId,
        analysis: AnalysisType,
        significance: f64,
        description: String,
    ) -> NodeId {
        let summary = analysis.natural_summary();
        let tech_summary = analysis.tech_summary();
        let id = NodeId(self.nodes.len());
        let node = AnalysisNode {
            id,
            parent_id: Some(parent_id),
            analysis,
            significance,
            description,
            summary,
            tech_summary,
            children: Vec::new(),
        };
        self.nodes.push(node);
        self.nodes[parent_id.0].children.push(id);
        id
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
