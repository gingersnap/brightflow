use std::collections::HashMap;

use crate::analysis::engine::ColumnCache;
use crate::stats::significance::{mean, std_dev, z_score};

#[derive(Debug, Clone)]
pub struct OutlierCluster {
    pub period: String,
    pub columns: Vec<String>,
    pub direction: String,                      // "spike" or "dip"
    pub common_segments: Vec<(String, String)>, // (dimension, value) pairs
}

/// Find outlier clusters across multiple columns.
/// Groups related outliers that occur in the same period.
///
/// # Arguments
/// * `cache` - Pre-extracted column data
/// * `period_labels` - Period label for each row
/// * `z_threshold` - Z-score threshold for outlier detection (default: 2.0)
///
/// # Returns
/// List of outlier clusters where 2+ columns have same-direction outliers in the same period.
pub fn find_outlier_clusters(
    cache: &ColumnCache,
    period_labels: &[Option<String>],
    z_threshold: f64,
) -> Vec<OutlierCluster> {
    if period_labels.is_empty() {
        return Vec::new();
    }

    // Step 1: Aggregate values by period for each numeric column
    let period_stats = aggregate_by_period(cache, period_labels);

    // Step 2: Find outliers in each column
    let column_outliers = find_column_outliers(&period_stats, z_threshold);

    // Step 3: Cluster outliers by period and direction
    let clusters = cluster_outliers(&column_outliers);

    // Step 4: Find common dimension values for each cluster
    enrich_with_common_segments(clusters, cache, period_labels)
}

/// Statistics for a period in a single column
#[derive(Debug)]
struct PeriodStat {
    period: String,
    #[allow(dead_code)]
    value: f64, // Mean value for this period
    z: f64, // Z-score compared to other periods
}

/// Aggregate values by period for each numeric column
fn aggregate_by_period(
    cache: &ColumnCache,
    period_labels: &[Option<String>],
) -> HashMap<String, Vec<PeriodStat>> {
    let mut result: HashMap<String, Vec<PeriodStat>> = HashMap::new();

    for (col_name, values) in &cache.numeric {
        // Group values by period
        let mut period_values: HashMap<String, Vec<f64>> = HashMap::new();

        for (val, period_opt) in values.iter().zip(period_labels.iter()) {
            if let Some(period) = period_opt {
                period_values.entry(period.clone()).or_default().push(*val);
            }
        }

        // Calculate mean for each period
        let period_means: Vec<(String, f64)> = period_values
            .into_iter()
            .map(|(period, vals)| (period, mean(&vals)))
            .collect();

        if period_means.len() < 3 {
            continue; // Need at least 3 periods for meaningful comparison
        }

        // Calculate z-scores for period means
        let all_means: Vec<f64> = period_means.iter().map(|(_, m)| *m).collect();
        let global_mean = mean(&all_means);
        let global_std = std_dev(&all_means);

        let stats: Vec<PeriodStat> = period_means
            .into_iter()
            .map(|(period, period_mean)| PeriodStat {
                period,
                value: period_mean,
                z: z_score(period_mean, global_mean, global_std),
            })
            .collect();

        result.insert(col_name.clone(), stats);
    }

    result
}

/// Outlier information for a single column
#[derive(Debug)]
struct ColumnOutlier {
    column: String,
    period: String,
    direction: String, // "spike" or "dip"
    #[allow(dead_code)]
    z_score: f64,
}

/// Find outliers in each column
fn find_column_outliers(
    period_stats: &HashMap<String, Vec<PeriodStat>>,
    z_threshold: f64,
) -> Vec<ColumnOutlier> {
    let mut outliers = Vec::new();

    for (col_name, stats) in period_stats {
        for stat in stats {
            if stat.z.abs() > z_threshold {
                let direction = if stat.z > 0.0 { "spike" } else { "dip" };
                outliers.push(ColumnOutlier {
                    column: col_name.clone(),
                    period: stat.period.clone(),
                    direction: direction.to_string(),
                    z_score: stat.z,
                });
            }
        }
    }

    outliers
}

/// Cluster outliers by period and direction
fn cluster_outliers(outliers: &[ColumnOutlier]) -> Vec<OutlierCluster> {
    // Group by (period, direction)
    let mut groups: HashMap<(String, String), Vec<String>> = HashMap::new();

    for outlier in outliers {
        let key = (outlier.period.clone(), outlier.direction.clone());
        groups.entry(key).or_default().push(outlier.column.clone());
    }

    // Only keep clusters with 2+ columns
    groups
        .into_iter()
        .filter(|(_, columns)| columns.len() >= 2)
        .map(|((period, direction), mut columns)| {
            columns.sort();
            OutlierCluster {
                period,
                columns,
                direction,
                common_segments: Vec::new(),
            }
        })
        .collect()
}

/// Find common dimension values for rows in each cluster's period
fn enrich_with_common_segments(
    mut clusters: Vec<OutlierCluster>,
    cache: &ColumnCache,
    period_labels: &[Option<String>],
) -> Vec<OutlierCluster> {
    for cluster in &mut clusters {
        let mut common: Vec<(String, String)> = Vec::new();

        for (dim_name, dim_values) in &cache.dimension {
            // Find all dimension values in this period
            let mut values_in_period: HashMap<String, usize> = HashMap::new();
            let mut total_in_period = 0;

            for (dim_val, period_opt) in dim_values.iter().zip(period_labels.iter()) {
                if let Some(period) = period_opt {
                    if period == &cluster.period && !dim_val.is_empty() {
                        *values_in_period.entry(dim_val.clone()).or_default() += 1;
                        total_in_period += 1;
                    }
                }
            }

            // If one value dominates (>50% of rows), consider it "common"
            for (val, count) in values_in_period {
                if count as f64 / total_in_period as f64 > 0.5 {
                    common.push((dim_name.clone(), val));
                }
            }
        }

        cluster.common_segments = common;
    }

    clusters
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_cache() -> ColumnCache {
        let mut numeric = HashMap::new();
        let mut dimension = HashMap::new();

        // Sales and Profit both spike in Mar (period 3) - with 5 normal periods and 1 outlier
        // Normal months: ~100, Outlier month Mar: ~400
        numeric.insert(
            "Sales".to_string(),
            vec![
                100.0, 102.0, // Jan
                105.0, 103.0, // Feb
                400.0, 410.0, // Mar (spike!)
                98.0, 101.0, // Apr
                99.0, 100.0, // May
            ],
        );
        numeric.insert(
            "Profit".to_string(),
            vec![
                10.0, 10.2, // Jan
                10.5, 10.3, // Feb
                50.0, 52.0, // Mar (spike!)
                9.8, 10.1, // Apr
                9.9, 10.0, // May
            ],
        );
        // Quantity stays normal - should NOT be in cluster
        numeric.insert(
            "Quantity".to_string(),
            vec![
                50.0, 51.0, // Jan
                52.0, 51.0, // Feb
                53.0, 52.0, // Mar (normal)
                50.0, 51.0, // Apr
                51.0, 50.0, // May
            ],
        );

        dimension.insert(
            "Region".to_string(),
            vec![
                "East".to_string(),
                "West".to_string(),
                "East".to_string(),
                "West".to_string(),
                "East".to_string(),
                "East".to_string(),
                "West".to_string(),
                "East".to_string(),
                "East".to_string(),
                "West".to_string(),
            ],
        );

        ColumnCache { numeric, dimension }
    }

    #[test]
    fn test_find_outlier_clusters() {
        let cache = make_cache();

        // 5 periods: Jan, Feb, Mar, Apr, May - 2 rows each
        let period_labels: Vec<Option<String>> = vec![
            Some("2023-01".to_string()),
            Some("2023-01".to_string()),
            Some("2023-02".to_string()),
            Some("2023-02".to_string()),
            Some("2023-03".to_string()),
            Some("2023-03".to_string()),
            Some("2023-04".to_string()),
            Some("2023-04".to_string()),
            Some("2023-05".to_string()),
            Some("2023-05".to_string()),
        ];

        let clusters = find_outlier_clusters(&cache, &period_labels, 1.5);

        // Should find a spike cluster in Mar with Sales and Profit
        assert!(!clusters.is_empty(), "Expected at least one cluster");

        let spike_cluster = clusters.iter().find(|c| c.direction == "spike");
        assert!(spike_cluster.is_some(), "Expected a spike cluster");

        let cluster = spike_cluster.unwrap();
        assert!(
            cluster.columns.contains(&"Sales".to_string()),
            "Expected Sales in cluster"
        );
        assert!(
            cluster.columns.contains(&"Profit".to_string()),
            "Expected Profit in cluster"
        );
        assert_eq!(cluster.period, "2023-03", "Expected cluster in March");
    }

    #[test]
    fn test_no_clusters_when_isolated() {
        let mut numeric = HashMap::new();
        // Only Sales spikes, not enough for a cluster
        numeric.insert("Sales".to_string(), vec![100.0, 110.0, 500.0]);
        numeric.insert("Profit".to_string(), vec![10.0, 11.0, 12.0]);

        let cache = ColumnCache {
            numeric,
            dimension: HashMap::new(),
        };

        let period_labels: Vec<Option<String>> = vec![
            Some("2023-01".to_string()),
            Some("2023-02".to_string()),
            Some("2023-03".to_string()),
        ];

        let clusters = find_outlier_clusters(&cache, &period_labels, 2.0);
        assert!(clusters.is_empty());
    }
}
