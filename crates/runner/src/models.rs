use benchmark::result::BenchmarkResult;
use dataset::metadata::DatasetMetadata;
use environment::EnvironmentMetadata;
use evaluation::models::{EvaluationReport, ValidationStatus};
use recommendation::models::RecommendationReport;
use serde::Serialize;

/// Comprehensive outcome of an end-to-end benchmark and recommendation pipeline run.
#[derive(Debug, Clone, Serialize)]
pub struct PipelineResult {
    /// Environment metadata recorded at execution start.
    pub environment: EnvironmentMetadata,
    /// Dataset validation and preprocessing metadata.
    pub dataset: DatasetMetadata,
    /// Raw benchmark results per database.
    pub benchmark_results: Vec<BenchmarkResult>,
    /// Calculated evaluation metrics, validation statuses, and aggregated stats.
    pub evaluation_report: EvaluationReport,
    /// Ranked candidate list and final database recommendation.
    pub recommendation_report: RecommendationReport,
}

impl PipelineResult {
    /// Identifier of the recommended database, if any eligible candidate was found.
    pub fn best_database(&self) -> Option<&str> {
        self.recommendation_report.best()
    }

    /// Whether any database was successfully recommended.
    pub fn has_recommendation(&self) -> bool {
        self.recommendation_report.has_eligible_candidates()
    }

    /// Number of databases that ran in the benchmark.
    pub fn database_count(&self) -> usize {
        self.benchmark_results.len()
    }

    /// Formats a clean human-readable summary of the entire pipeline run.
    pub fn format_summary(&self) -> String {
        let mut out = String::new();

        out.push_str(
            "================================================================================\n",
        );
        out.push_str(
            "           DATABASE BENCHMARKING & RECOMMENDATION - PIPELINE REPORT             \n",
        );
        out.push_str(
            "================================================================================\n\n",
        );

        // 1. Environment
        out.push_str("--- ENVIRONMENT ---\n");
        out.push_str(&format!(
            "  Experiment ID : {}\n",
            self.environment.experiment_id
        ));
        out.push_str(&format!(
            "  Timestamp     : {}\n",
            self.environment.timestamp
        ));
        out.push_str(&format!("  OS            : {}\n", self.environment.os));
        out.push_str(&format!(
            "  CPU Cores     : {}\n",
            self.environment.cpu_cores
        ));
        out.push_str(&format!(
            "  Total RAM     : {} MB\n\n",
            self.environment.total_ram_mb
        ));

        // 2. Dataset
        out.push_str("--- DATASET ---\n");
        out.push_str(&format!("  Path          : {}\n", self.dataset.path));
        out.push_str(&format!("  Format        : {}\n", self.dataset.format));
        out.push_str(&format!(
            "  Documents     : {}\n",
            self.dataset.document_count
        ));
        out.push_str(&format!(
            "  Size          : {} bytes\n",
            self.dataset.original_size_bytes
        ));
        out.push_str(&format!(
            "  SHA-256 Hash  : {}\n\n",
            self.dataset.dataset_hash
        ));

        // 3. Execution & Evaluation
        out.push_str("--- DATABASE EXECUTION & EVALUATION ---\n");
        for res in &self.benchmark_results {
            let eval = self.evaluation_report.databases.get(&res.database);
            let status_str = match eval.map(|e| &e.validation_status) {
                Some(ValidationStatus::Valid) => "VALID".to_string(),
                Some(ValidationStatus::Invalid(msg)) => format!("INVALID ({msg})"),
                Some(ValidationStatus::Skipped) => "SKIPPED".to_string(),
                None => "UNKNOWN".to_string(),
            };

            let insert_status = if res.insert.status.is_success() {
                format!("{:.2} ms", res.insert.duration.as_secs_f64() * 1000.0)
            } else {
                "FAILED".to_string()
            };

            let query_status = if res.queries.is_empty() {
                "None executed".to_string()
            } else {
                let success_count = res.queries.iter().filter(|q| q.status.is_success()).count();
                format!("{}/{} succeeded", success_count, res.queries.len())
            };

            let throughput = eval
                .and_then(|e| e.metrics.get("throughput"))
                .map(|m| format!("{:.2} ops/sec", m.value))
                .unwrap_or_else(|| "N/A".to_string());

            let success_rate = eval
                .and_then(|e| e.metrics.get("success_rate"))
                .map(|m| format!("{:.1}%", m.value))
                .unwrap_or_else(|| "N/A".to_string());

            out.push_str(&format!("  Database: {}\n", res.database));
            out.push_str(&format!("    Validation Status : {}\n", status_str));
            out.push_str(&format!("    Insert Duration   : {}\n", insert_status));
            out.push_str(&format!("    Queries           : {}\n", query_status));
            out.push_str(&format!("    Throughput        : {}\n", throughput));
            out.push_str(&format!("    Success Rate      : {}\n\n", success_rate));
        }

        // 4. Recommendation
        out.push_str("--- RECOMMENDATION OUTCOME ---\n");
        if let Some(ref best) = self.recommendation_report.best_candidate {
            let score = self
                .recommendation_report
                .ranked_candidates
                .first()
                .map(|r| r.score)
                .unwrap_or(0.0);
            out.push_str(&format!(
                "  ★ BEST DATABASE RECOMMENDED: '{}' (Score: {:.2})\n\n",
                best, score
            ));
        } else {
            out.push_str("  ⚠ NO ELIGIBLE DATABASE RECOMMENDED\n\n");
        }

        if !self.recommendation_report.ranked_candidates.is_empty() {
            out.push_str("  Ranked Candidates:\n");
            for candidate in &self.recommendation_report.ranked_candidates {
                out.push_str(&format!(
                    "    #{}. {} - Score: {:.2} | {}\n",
                    candidate.rank, candidate.database, candidate.score, candidate.explanation
                ));
            }
            out.push('\n');
        }

        if !self.recommendation_report.excluded_candidates.is_empty() {
            out.push_str("  Excluded Candidates:\n");
            for candidate in &self.recommendation_report.excluded_candidates {
                out.push_str(&format!(
                    "    - {} : {}\n",
                    candidate.database, candidate.explanation
                ));
            }
            out.push('\n');
        }

        out.push_str(&format!(
            "  Summary: {}\n",
            self.recommendation_report.summary
        ));
        out.push_str(
            "================================================================================\n",
        );

        out
    }
}
