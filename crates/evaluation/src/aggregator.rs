use std::collections::HashMap;

/// Aggregated statistics across multiple numeric values.
#[derive(Debug, Clone, PartialEq)]
pub struct AggregatedStat {
    pub count: usize,
    pub mean: f64,
    pub median: f64,
    pub min: f64,
    pub max: f64,
    pub variance: f64,
}

/// Compute aggregate statistics over a slice of f64 values.
///
/// Returns `None` if the slice is empty.
pub fn aggregate(values: &[f64]) -> Option<AggregatedStat> {
    if values.is_empty() {
        return None;
    }

    let count = values.len();
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let mean = values.iter().sum::<f64>() / count as f64;

    let variance = if count == 1 {
        0.0
    } else {
        values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count as f64
    };

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = if count % 2 == 0 {
        (sorted[count / 2 - 1] + sorted[count / 2]) / 2.0
    } else {
        sorted[count / 2]
    };

    Some(AggregatedStat {
        count,
        mean,
        median,
        min,
        max,
        variance,
    })
}

/// Collects numeric metric values across multiple benchmark runs and computes
/// aggregate statistics (average, median, min, max, variance) per metric name.
///
/// Use [`push`](Self::push) to record individual observations, then call
/// [`compute`](Self::compute) to obtain the statistics.
///
/// All operations handle empty collections safely — [`compute`](Self::compute)
/// returns an empty map and [`compute_for`](Self::compute_for) returns `None`
/// when no observations exist.
///
/// # Example
/// ```rust
/// use evaluation::aggregator::MetricsAggregator;
///
/// let mut agg = MetricsAggregator::new();
/// agg.push("latency_ms", 10.0);
/// agg.push("latency_ms", 20.0);
/// agg.push("latency_ms", 30.0);
///
/// let stats = agg.compute();
/// let lat = stats.get("latency_ms").unwrap();
/// assert_eq!(lat.mean, 20.0);
/// assert_eq!(lat.min, 10.0);
/// assert_eq!(lat.max, 30.0);
/// ```
#[derive(Debug, Clone, Default)]
pub struct MetricsAggregator {
    values: HashMap<String, Vec<f64>>,
}

impl MetricsAggregator {
    /// Create an empty aggregator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one observation of `metric_name` with the given `value`.
    pub fn push(&mut self, metric_name: impl Into<String>, value: f64) {
        self.values
            .entry(metric_name.into())
            .or_default()
            .push(value);
    }

    /// Merge all observations from `other` into this aggregator.
    pub fn merge(&mut self, other: &MetricsAggregator) {
        for (name, vals) in &other.values {
            self.values
                .entry(name.clone())
                .or_default()
                .extend_from_slice(vals);
        }
    }

    /// Returns the number of distinct metric names recorded.
    pub fn metric_count(&self) -> usize {
        self.values.len()
    }

    /// Returns `true` if no observations have been recorded yet.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Compute [`AggregatedStat`] for every recorded metric name.
    ///
    /// Metrics whose value slice is empty are excluded from the output (a
    /// situation that cannot arise through normal [`push`](Self::push) usage,
    /// but is handled defensively).
    pub fn compute(&self) -> HashMap<String, AggregatedStat> {
        self.values
            .iter()
            .filter_map(|(name, vals)| aggregate(vals).map(|stat| (name.clone(), stat)))
            .collect()
    }

    /// Compute the [`AggregatedStat`] for a single named metric.
    ///
    /// Returns `None` when `metric_name` has not been recorded or its slice is
    /// empty.
    pub fn compute_for(&self, metric_name: &str) -> Option<AggregatedStat> {
        self.values
            .get(metric_name)
            .and_then(|vals| aggregate(vals))
    }

    /// Return an iterator over all recorded metric names.
    pub fn metric_names(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── aggregate() free function ────────────────────────────────────────────

    #[test]
    fn test_aggregate_empty() {
        assert_eq!(aggregate(&[]), None);
    }

    #[test]
    fn test_aggregate_single_value() {
        let stat = aggregate(&[42.0]).unwrap();
        assert_eq!(stat.count, 1);
        assert_eq!(stat.mean, 42.0);
        assert_eq!(stat.median, 42.0);
        assert_eq!(stat.min, 42.0);
        assert_eq!(stat.max, 42.0);
        assert_eq!(stat.variance, 0.0);
    }

    #[test]
    fn test_aggregate_multiple_values() {
        let stat = aggregate(&[10.0, 20.0, 30.0, 40.0, 50.0]).unwrap();
        assert_eq!(stat.count, 5);
        assert_eq!(stat.mean, 30.0);
        assert_eq!(stat.median, 30.0);
        assert_eq!(stat.min, 10.0);
        assert_eq!(stat.max, 50.0);
        // variance = ((10-30)^2 + (20-30)^2 + (30-30)^2 + (40-30)^2 + (50-30)^2) / 5
        //          = (400 + 100 + 0 + 100 + 400) / 5 = 200
        assert!((stat.variance - 200.0).abs() < 1e-9);
    }

    #[test]
    fn test_aggregate_even_count_median() {
        let stat = aggregate(&[10.0, 20.0, 30.0, 40.0]).unwrap();
        // median = (20 + 30) / 2 = 25
        assert_eq!(stat.median, 25.0);
    }

    // ── MetricsAggregator ────────────────────────────────────────────────────

    #[test]
    fn test_metrics_aggregator_new_is_empty() {
        let agg = MetricsAggregator::new();
        assert!(agg.is_empty());
        assert_eq!(agg.metric_count(), 0);
        assert!(agg.compute().is_empty());
    }

    #[test]
    fn test_metrics_aggregator_single_metric() {
        let mut agg = MetricsAggregator::new();
        agg.push("latency_ms", 10.0);
        agg.push("latency_ms", 20.0);
        agg.push("latency_ms", 30.0);

        let stats = agg.compute();
        let lat = stats.get("latency_ms").unwrap();
        assert_eq!(lat.count, 3);
        assert_eq!(lat.mean, 20.0);
        assert_eq!(lat.median, 20.0);
        assert_eq!(lat.min, 10.0);
        assert_eq!(lat.max, 30.0);
    }

    #[test]
    fn test_metrics_aggregator_multiple_metrics() {
        let mut agg = MetricsAggregator::new();
        agg.push("insert_ms", 100.0);
        agg.push("insert_ms", 200.0);
        agg.push("query_ms", 10.0);
        agg.push("query_ms", 20.0);
        agg.push("query_ms", 30.0);

        assert_eq!(agg.metric_count(), 2);
        let stats = agg.compute();
        assert_eq!(stats.len(), 2);

        let ins = stats.get("insert_ms").unwrap();
        assert_eq!(ins.mean, 150.0);

        let qry = stats.get("query_ms").unwrap();
        assert_eq!(qry.mean, 20.0);
    }

    #[test]
    fn test_metrics_aggregator_compute_for() {
        let mut agg = MetricsAggregator::new();
        agg.push("tp", 1.0);
        agg.push("tp", 3.0);

        let stat = agg.compute_for("tp").unwrap();
        assert_eq!(stat.mean, 2.0);
        assert_eq!(stat.min, 1.0);
        assert_eq!(stat.max, 3.0);

        assert!(agg.compute_for("nonexistent").is_none());
    }

    #[test]
    fn test_metrics_aggregator_merge() {
        let mut a = MetricsAggregator::new();
        a.push("metric", 10.0);
        a.push("metric", 20.0);

        let mut b = MetricsAggregator::new();
        b.push("metric", 30.0);
        b.push("other", 5.0);

        a.merge(&b);

        assert_eq!(a.metric_count(), 2);
        let stat = a.compute_for("metric").unwrap();
        assert_eq!(stat.count, 3);
        assert_eq!(stat.mean, 20.0);
    }

    #[test]
    fn test_metrics_aggregator_variance() {
        let mut agg = MetricsAggregator::new();
        // values: [10, 20, 30]  mean=20  variance = ((10-20)²+(20-20)²+(30-20)²)/3 = 200/3
        agg.push("v", 10.0);
        agg.push("v", 20.0);
        agg.push("v", 30.0);
        let stat = agg.compute_for("v").unwrap();
        let expected_variance = 200.0 / 3.0;
        assert!((stat.variance - expected_variance).abs() < 1e-9);
    }

    #[test]
    fn test_metrics_aggregator_metric_names_iterator() {
        let mut agg = MetricsAggregator::new();
        agg.push("a", 1.0);
        agg.push("b", 2.0);
        agg.push("c", 3.0);

        let mut names: Vec<&str> = agg.metric_names().collect();
        names.sort_unstable();
        assert_eq!(names, vec!["a", "b", "c"]);
    }
}
