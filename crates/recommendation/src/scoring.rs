use crate::config::RecommendationConfig;
use crate::models::{CandidateStatus, RecommendationReport, RecommendationResult};
use evaluation::models::{DatabaseEvaluation, EvaluationReport, ValidationStatus};
use std::collections::HashMap;

/// Helper structure holding extracted raw metrics for an eligible candidate.
#[derive(Debug, Clone, Default)]
struct CandidateRawMetrics {
    latency: Option<f64>,
    throughput: Option<f64>,
    accuracy: Option<f64>,
    resource_usage: Option<f64>,
}

impl CandidateRawMetrics {
    fn is_empty(&self) -> bool {
        self.latency.is_none()
            && self.throughput.is_none()
            && self.accuracy.is_none()
            && self.resource_usage.is_none()
    }
}

/// Core scoring engine that performs metric extraction, normalization,
/// multi-criteria weighting, and ranking.
pub struct ScoreCalculator;

impl ScoreCalculator {
    /// Computes the complete recommendation report from an evaluation report.
    pub fn compute_recommendations(
        report: &EvaluationReport,
        config: &RecommendationConfig,
    ) -> RecommendationReport {
        let mut eligible_entries: Vec<(&String, &DatabaseEvaluation)> = Vec::new();
        let mut excluded_results: Vec<RecommendationResult> = Vec::new();

        // 1. Separate eligible candidates from excluded ones.
        // We sort keys first to guarantee deterministic evaluation order.
        let mut sorted_db_keys: Vec<&String> = report.databases.keys().collect();
        sorted_db_keys.sort();

        for db_name in sorted_db_keys {
            // Ignore internal evaluation artifacts (__aggregate__ and __runs__).
            if db_name.starts_with("__") {
                continue;
            }

            let eval = &report.databases[db_name];

            // Check validation status.
            if let ValidationStatus::Invalid(ref reason) = eval.validation_status {
                excluded_results.push(RecommendationResult {
                    database: db_name.clone(),
                    score: 0.0,
                    rank: 0,
                    contributing_metrics: HashMap::new(),
                    explanation: format!(
                        "Excluded from recommendations: validation failed ({reason})."
                    ),
                    status: CandidateStatus::Excluded(format!("Validation failed: {reason}")),
                });
                continue;
            }

            if config.require_valid_status
                && eval.validation_status != ValidationStatus::Valid
                && eval.validation_status != ValidationStatus::Skipped
            {
                excluded_results.push(RecommendationResult {
                    database: db_name.clone(),
                    score: 0.0,
                    rank: 0,
                    contributing_metrics: HashMap::new(),
                    explanation: "Excluded from recommendations: invalid validation status."
                        .to_string(),
                    status: CandidateStatus::Excluded("Invalid validation status".to_string()),
                });
                continue;
            }

            // Check if any metrics exist.
            if eval.metrics.is_empty() {
                excluded_results.push(RecommendationResult {
                    database: db_name.clone(),
                    score: 0.0,
                    rank: 0,
                    contributing_metrics: HashMap::new(),
                    explanation:
                        "Excluded from recommendations: no metrics recorded in evaluation."
                            .to_string(),
                    status: CandidateStatus::Excluded(
                        "No metrics recorded in evaluation".to_string(),
                    ),
                });
                continue;
            }

            // Check minimum success rate threshold.
            if config.min_success_rate > 0.0 {
                if let Some(sr) = eval.metrics.get("success_rate") {
                    if sr.value < config.min_success_rate {
                        excluded_results.push(RecommendationResult {
                            database: db_name.clone(),
                            score: 0.0,
                            rank: 0,
                            contributing_metrics: HashMap::new(),
                            explanation: format!(
                                "Excluded from recommendations: success rate {:.1}% is below minimum required {:.1}%.",
                                sr.value, config.min_success_rate
                            ),
                            status: CandidateStatus::Excluded(format!(
                                "Success rate {:.1}% below minimum required {:.1}%",
                                sr.value, config.min_success_rate
                            )),
                        });
                        continue;
                    }
                }
            }

            eligible_entries.push((db_name, eval));
        }

        // 2. Extract raw comparable metrics for eligible candidates.
        let mut candidate_metrics: Vec<(&String, CandidateRawMetrics)> = Vec::new();

        for (db_name, eval) in eligible_entries {
            let raw = Self::extract_raw_metrics(eval);
            if raw.is_empty() {
                excluded_results.push(RecommendationResult {
                    database: (*db_name).clone(),
                    score: 0.0,
                    rank: 0,
                    contributing_metrics: HashMap::new(),
                    explanation:
                        "Excluded from recommendations: no recognized scoring metrics found."
                            .to_string(),
                    status: CandidateStatus::Excluded(
                        "No recognized scoring metrics found".to_string(),
                    ),
                });
            } else {
                candidate_metrics.push((db_name, raw));
            }
        }

        // 3. Compute min and max across all eligible candidates for each metric dimension.
        let min_max = Self::compute_min_max(&candidate_metrics);

        // 4. Normalize metrics and calculate composite score for each candidate.
        let mut scored_candidates: Vec<RecommendationResult> = Vec::new();

        for (db_name, raw) in &candidate_metrics {
            let (score, contributing) = Self::score_candidate(raw, &min_max, config);

            scored_candidates.push(RecommendationResult {
                database: (*db_name).clone(),
                score,
                rank: 0, // rank will be assigned after deterministic sorting
                contributing_metrics: contributing,
                explanation: String::new(),
                status: CandidateStatus::Eligible,
            });
        }

        // 5. Deterministic sorting and tie-breaking:
        // Primary: score descending.
        // Secondary: database name ascending (alphabetical).
        scored_candidates.sort_by(|a, b| match b.score.partial_cmp(&a.score) {
            Some(std::cmp::Ordering::Equal) | None => a.database.cmp(&b.database),
            Some(ord) => ord,
        });

        // 6. Assign 1-based ranks and generate evidence-based explanations.
        for (i, candidate) in scored_candidates.iter_mut().enumerate() {
            let rank = i + 1;
            candidate.rank = rank;
            candidate.explanation =
                Self::generate_explanation(rank, candidate.score, &candidate.contributing_metrics);
        }

        // Deterministically sort excluded results by database name as well.
        excluded_results.sort_by(|a, b| a.database.cmp(&b.database));

        // 7. Determine best candidate and build report summary.
        let best_candidate = scored_candidates.first().map(|c| c.database.clone());

        let summary = if let Some(ref best) = best_candidate {
            let best_score = scored_candidates[0].score;
            format!(
                "Recommended database '{}' with composite score {:.2} for workload '{}'. {} eligible candidates ranked, {} excluded.",
                best, best_score, report.workload_name, scored_candidates.len(), excluded_results.len()
            )
        } else if !excluded_results.is_empty() {
            format!(
                "No eligible database candidates found for workload '{}'. All {} evaluated candidate(s) were excluded.",
                report.workload_name, excluded_results.len()
            )
        } else {
            format!(
                "No database candidates evaluated for workload '{}'.",
                report.workload_name
            )
        };

        RecommendationReport {
            workload_name: report.workload_name.clone(),
            ranked_candidates: scored_candidates,
            best_candidate,
            excluded_candidates: excluded_results,
            summary,
        }
    }

    /// Extracts raw metrics across the 4 dimensions from a database evaluation.
    fn extract_raw_metrics(eval: &DatabaseEvaluation) -> CandidateRawMetrics {
        CandidateRawMetrics {
            latency: Self::extract_latency(eval),
            throughput: Self::extract_throughput(eval),
            accuracy: Self::extract_accuracy(eval),
            resource_usage: Self::extract_resource_usage(eval),
        }
    }

    fn extract_latency(eval: &DatabaseEvaluation) -> Option<f64> {
        let keys = [
            "average_query_latency",
            "average_latency",
            "latency",
            "insert_latency",
        ];
        for k in &keys {
            if let Some(m) = eval.metrics.get(*k) {
                if m.value >= 0.0 {
                    return Some(m.value);
                }
            }
        }

        // Fallback: search any key ending with _latency or containing latency
        let mut sorted_keys: Vec<_> = eval.metrics.keys().collect();
        sorted_keys.sort();
        for k in sorted_keys {
            if k.contains("latency") {
                if let Some(m) = eval.metrics.get(k) {
                    if m.value >= 0.0 {
                        return Some(m.value);
                    }
                }
            }
        }

        None
    }

    fn extract_throughput(eval: &DatabaseEvaluation) -> Option<f64> {
        if let Some(m) = eval.metrics.get("throughput") {
            if m.value >= 0.0 {
                return Some(m.value);
            }
        }

        let mut sorted_keys: Vec<_> = eval.metrics.keys().collect();
        sorted_keys.sort();
        for k in sorted_keys {
            if k.contains("throughput") {
                if let Some(m) = eval.metrics.get(k) {
                    if m.value >= 0.0 {
                        return Some(m.value);
                    }
                }
            }
        }

        None
    }

    fn extract_accuracy(eval: &DatabaseEvaluation) -> Option<f64> {
        // Direct exact matches
        if let Some(m) = eval.metrics.get("accuracy") {
            return Some(m.value);
        }
        if let Some(m) = eval.metrics.get("f1") {
            return Some(m.value);
        }
        if let Some(m) = eval.metrics.get("f1_score") {
            return Some(m.value);
        }

        // Check for injected query accuracy metrics: "accuracy/<query>/f1"
        let mut f1_values = Vec::new();
        let mut prec_rec_values = Vec::new();

        let mut sorted_keys: Vec<_> = eval.metrics.keys().collect();
        sorted_keys.sort();

        for k in sorted_keys {
            if k.starts_with("accuracy/") && k.ends_with("/f1") {
                if let Some(m) = eval.metrics.get(k) {
                    f1_values.push(m.value);
                }
            } else if k.starts_with("accuracy/")
                && (k.ends_with("/precision") || k.ends_with("/recall"))
            {
                if let Some(m) = eval.metrics.get(k) {
                    prec_rec_values.push(m.value);
                }
            }
        }

        if !f1_values.is_empty() {
            let avg: f64 = f1_values.iter().sum::<f64>() / f1_values.len() as f64;
            return Some(avg);
        }

        if !prec_rec_values.is_empty() {
            let avg: f64 = prec_rec_values.iter().sum::<f64>() / prec_rec_values.len() as f64;
            return Some(avg);
        }

        if let Some(m) = eval.metrics.get("precision") {
            return Some(m.value);
        }
        if let Some(m) = eval.metrics.get("recall") {
            return Some(m.value);
        }

        None
    }

    fn extract_resource_usage(eval: &DatabaseEvaluation) -> Option<f64> {
        let keys = [
            "resource_usage",
            "resource_consumption",
            "resource",
            "memory_mb",
            "memory_usage",
            "memory",
            "cpu_percent",
            "cpu_usage",
            "cpu",
        ];
        for k in &keys {
            if let Some(m) = eval.metrics.get(*k) {
                if m.value >= 0.0 {
                    return Some(m.value);
                }
            }
        }

        let mut sorted_keys: Vec<_> = eval.metrics.keys().collect();
        sorted_keys.sort();
        for k in sorted_keys {
            if k.contains("memory") || k.contains("cpu") || k.contains("resource") {
                if let Some(m) = eval.metrics.get(k) {
                    if m.value >= 0.0 {
                        return Some(m.value);
                    }
                }
            }
        }

        None
    }

    /// Computes (min, max) for each metric across candidate measurements.
    fn compute_min_max(
        candidates: &[(&String, CandidateRawMetrics)],
    ) -> HashMap<String, (f64, f64)> {
        let mut min_max = HashMap::new();

        let mut latencies = Vec::new();
        let mut throughputs = Vec::new();
        let mut accuracies = Vec::new();
        let mut resources = Vec::new();

        for (_, raw) in candidates {
            if let Some(v) = raw.latency {
                latencies.push(v);
            }
            if let Some(v) = raw.throughput {
                throughputs.push(v);
            }
            if let Some(v) = raw.accuracy {
                accuracies.push(v);
            }
            if let Some(v) = raw.resource_usage {
                resources.push(v);
            }
        }

        let record_range = |map: &mut HashMap<String, (f64, f64)>, name: &str, vals: &[f64]| {
            if !vals.is_empty() {
                let min = vals.iter().copied().fold(f64::INFINITY, f64::min);
                let max = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                map.insert(name.to_string(), (min, max));
            }
        };

        record_range(&mut min_max, "latency", &latencies);
        record_range(&mut min_max, "throughput", &throughputs);
        record_range(&mut min_max, "accuracy", &accuracies);
        record_range(&mut min_max, "resource_usage", &resources);

        min_max
    }

    /// Scores a candidate using normalized values and active metric weights.
    /// Handles missing metrics dynamically without inventing values.
    fn score_candidate(
        raw: &CandidateRawMetrics,
        min_max: &HashMap<String, (f64, f64)>,
        config: &RecommendationConfig,
    ) -> (f64, HashMap<String, f64>) {
        let mut contributing = HashMap::new();
        let mut weighted_sum = 0.0;
        let mut active_weight_sum = 0.0;

        // Normalization helper:
        // lower_is_better: (max - val) / (max - min)
        // higher_is_better: (val - min) / (max - min)
        let normalize = |val: f64, min: f64, max: f64, lower_is_better: bool| -> f64 {
            if (max - min).abs() < 1e-12 {
                1.0 // Equal observed values get full normalized score
            } else if lower_is_better {
                ((max - val) / (max - min)).clamp(0.0, 1.0)
            } else {
                ((val - min) / (max - min)).clamp(0.0, 1.0)
            }
        };

        // 1. Latency (lower is better)
        if let Some(val) = raw.latency {
            if let Some(&(min, max)) = min_max.get("latency") {
                let norm = normalize(val, min, max, true);
                if config.weights.latency > 0.0 {
                    weighted_sum += config.weights.latency * norm;
                    active_weight_sum += config.weights.latency;
                    contributing.insert("latency".to_string(), (norm * 1000.0).round() / 1000.0);
                }
            }
        }

        // 2. Throughput (higher is better)
        if let Some(val) = raw.throughput {
            if let Some(&(min, max)) = min_max.get("throughput") {
                let norm = normalize(val, min, max, false);
                if config.weights.throughput > 0.0 {
                    weighted_sum += config.weights.throughput * norm;
                    active_weight_sum += config.weights.throughput;
                    contributing.insert("throughput".to_string(), (norm * 1000.0).round() / 1000.0);
                }
            }
        }

        // 3. Accuracy (higher is better)
        if let Some(val) = raw.accuracy {
            if let Some(&(min, max)) = min_max.get("accuracy") {
                let norm = normalize(val, min, max, false);
                if config.weights.accuracy > 0.0 {
                    weighted_sum += config.weights.accuracy * norm;
                    active_weight_sum += config.weights.accuracy;
                    contributing.insert("accuracy".to_string(), (norm * 1000.0).round() / 1000.0);
                }
            }
        }

        // 4. Resource usage (lower is better)
        if let Some(val) = raw.resource_usage {
            if let Some(&(min, max)) = min_max.get("resource_usage") {
                let norm = normalize(val, min, max, true);
                if config.weights.resource_usage > 0.0 {
                    weighted_sum += config.weights.resource_usage * norm;
                    active_weight_sum += config.weights.resource_usage;
                    contributing.insert(
                        "resource_usage".to_string(),
                        (norm * 1000.0).round() / 1000.0,
                    );
                }
            }
        }

        let final_score = if active_weight_sum > 0.0 {
            let score = weighted_sum / active_weight_sum;
            (score.clamp(0.0, 1.0) * 10000.0).round() / 10000.0
        } else {
            0.0
        };

        (final_score, contributing)
    }

    /// Generates a concise, evidence-based explanation for an eligible candidate.
    fn generate_explanation(
        rank: usize,
        score: f64,
        contributing: &HashMap<String, f64>,
    ) -> String {
        let mut keys: Vec<&String> = contributing.keys().collect();
        keys.sort();

        let mut parts = Vec::new();
        for k in keys {
            if let Some(val) = contributing.get(k) {
                parts.push(format!("{}: {:.2}", k, val));
            }
        }

        if parts.is_empty() {
            format!("Ranked #{} with composite score {:.2}.", rank, score)
        } else {
            format!(
                "Ranked #{} with composite score {:.2} (contributing metrics: {}).",
                rank,
                score,
                parts.join(", ")
            )
        }
    }
}
