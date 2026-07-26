use crate::config::EvaluationConfig;
use crate::models::{EvaluationReport, MetricScore};
use benchmark::result::BenchmarkResult;

pub trait MetricEvaluator {
    fn evaluate(&self, result: &BenchmarkResult, config: &EvaluationConfig) -> Vec<MetricScore>;
}

pub trait ReportBuilder {
    fn build_report(
        &self,
        results: &[BenchmarkResult],
        config: &EvaluationConfig,
    ) -> EvaluationReport;
}
