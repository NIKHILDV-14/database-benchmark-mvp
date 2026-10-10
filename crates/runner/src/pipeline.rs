use crate::error::PipelineError;
use crate::models::PipelineResult;
use benchmark::engine::BenchmarkEngine;
use benchmark::workload::{QuerySpec, Workload};
use benchmark_config::validator::ConfigValidator;
use benchmark_config::BenchmarkConfig;
use dataset::loader::DatasetLoader;
use dataset::model::Document;
use dataset::preprocess::Preprocessor;
use dataset::validator::DatasetValidator;
use environment::recorder::EnvironmentRecorder;
use evaluation::config::EvaluationConfig;
use evaluation::report_builder::EvaluationReportBuilder;
use evaluation::ReportBuilder;
use recommendation::config::RecommendationConfig;
use recommendation::engine::RecommendationEngine;
use std::path::Path;
use tracing::info;

/// Options for configuring the execution of the end-to-end pipeline.
#[derive(Debug, Clone)]
pub struct PipelineOptions {
    /// Configuration for the Phase 6 evaluation engine.
    pub evaluation_config: EvaluationConfig,
    /// Configuration for the Phase 7 recommendation engine.
    pub recommendation_config: RecommendationConfig,
    /// Workload queries to run against each database.
    pub queries: Vec<QuerySpec>,
}

impl Default for PipelineOptions {
    fn default() -> Self {
        Self {
            evaluation_config: EvaluationConfig::default(),
            recommendation_config: RecommendationConfig::default(),
            queries: vec![QuerySpec::new(
                "count_query",
                "SELECT COUNT(*) FROM documents",
            )],
        }
    }
}

/// The main orchestrator connecting all phases into an end-to-end workflow:
///
/// Dataset loading → preprocessing/embedding where required → database adapters
/// → benchmark execution → evaluation/report generation → database recommendation.
pub struct BenchmarkPipeline;

impl BenchmarkPipeline {
    /// Loads a configuration file from disk and executes the full pipeline with default options.
    pub fn run_from_file(config_path: impl AsRef<Path>) -> Result<PipelineResult, PipelineError> {
        Self::run_from_file_with_options(config_path, &PipelineOptions::default())
    }

    /// Loads a configuration file from disk and executes the full pipeline with custom options.
    pub fn run_from_file_with_options(
        config_path: impl AsRef<Path>,
        options: &PipelineOptions,
    ) -> Result<PipelineResult, PipelineError> {
        let path = config_path.as_ref();
        if !path.exists() {
            return Err(PipelineError::ConfigNotFound(path.display().to_string()));
        }

        let config_str = path
            .to_str()
            .ok_or_else(|| PipelineError::ConfigNotFound(path.display().to_string()))?;

        let config = BenchmarkConfig::load(config_str)
            .map_err(|e| PipelineError::ConfigParse(e.to_string()))?;

        Self::run_with_options(&config, options)
    }

    /// Executes the full pipeline using an in-memory `BenchmarkConfig` with default options.
    pub fn run(config: &BenchmarkConfig) -> Result<PipelineResult, PipelineError> {
        Self::run_with_options(config, &PipelineOptions::default())
    }

    /// Executes the full pipeline using an in-memory `BenchmarkConfig` and custom `PipelineOptions`.
    pub fn run_with_options(
        config: &BenchmarkConfig,
        options: &PipelineOptions,
    ) -> Result<PipelineResult, PipelineError> {
        info!("Starting end-to-end database benchmarking pipeline");

        // ── 1. Validate Configuration ─────────────────────────────────────────
        ConfigValidator::validate(config)
            .map_err(|e| PipelineError::InvalidConfig(e.to_string()))?;

        // ── 2. Record Environment ─────────────────────────────────────────────
        let environment =
            EnvironmentRecorder::record().map_err(|e| PipelineError::Environment(e.to_string()))?;

        info!(
            experiment_id = %environment.experiment_id,
            os = %environment.os,
            cores = environment.cpu_cores,
            ram_mb = environment.total_ram_mb,
            "Environment metadata recorded"
        );

        // ── 3. Validate and Hash Dataset ──────────────────────────────────────
        let dataset_meta = DatasetValidator::process_dataset(&config.dataset)
            .map_err(|e| PipelineError::Dataset(e.to_string()))?;

        info!(
            dataset_path = %dataset_meta.path,
            doc_count = dataset_meta.document_count,
            hash = %dataset_meta.dataset_hash,
            "Dataset validated successfully"
        );

        // ── 4. Load & Preprocess Documents ────────────────────────────────────
        let documents = Self::load_documents(config)?;

        info!(
            loaded_docs = documents.len(),
            "Preprocessed documents loaded into memory"
        );

        // ── 5. Build Benchmark Workload ───────────────────────────────────────
        let workload = Workload::new(&config.experiment.name)
            .with_documents(documents)
            .with_queries(options.queries.clone());

        // ── 6. Execute Benchmark Engine ───────────────────────────────────────
        info!(
            workload = %workload.name,
            queries = workload.queries.len(),
            "Executing benchmark engine against enabled databases"
        );

        let engine = BenchmarkEngine::new(config.clone());
        let benchmark_results = engine.run(&workload);

        if benchmark_results.is_empty() {
            return Err(PipelineError::Benchmark(
                "No databases were executed (verify databases are enabled in configuration)"
                    .to_string(),
            ));
        }

        info!(
            db_count = benchmark_results.len(),
            "Benchmark runs completed"
        );

        // ── 7. Generate Evaluation Report (Phase 6) ───────────────────────────
        let report_builder = EvaluationReportBuilder::new(&workload.name);
        let evaluation_report =
            report_builder.build_report(&benchmark_results, &options.evaluation_config);

        info!(
            evaluated_databases = evaluation_report.databases.len(),
            "Evaluation report generated"
        );

        // ── 8. Generate Recommendation Report (Phase 7) ───────────────────────
        let rec_engine = RecommendationEngine::new(options.recommendation_config.clone());
        let recommendation_report = rec_engine.recommend(&evaluation_report);

        if let Some(ref best) = recommendation_report.best_candidate {
            info!(best_database = %best, "Recommendation completed with winner");
        } else {
            info!("Recommendation completed (no eligible candidates found)");
        }

        // ── 9. Assemble Pipeline Result ───────────────────────────────────────
        Ok(PipelineResult {
            environment,
            dataset: dataset_meta,
            benchmark_results,
            evaluation_report,
            recommendation_report,
        })
    }

    /// Loads and preprocesses documents from the configured dataset path.
    fn load_documents(config: &BenchmarkConfig) -> Result<Vec<Document>, PipelineError> {
        let path = Path::new(&config.dataset.path);
        let is_csv = config.dataset.path.ends_with(".csv");

        let mut docs = Vec::new();

        if is_csv {
            let iter =
                DatasetLoader::load_csv(path).map_err(|e| PipelineError::Dataset(e.to_string()))?;
            for doc_res in iter {
                let mut doc = doc_res.map_err(|e| PipelineError::Dataset(e.to_string()))?;
                doc.text = Preprocessor::preprocess(&doc.text, &config.dataset).into_owned();
                docs.push(doc);
            }
        } else {
            let iter = DatasetLoader::load_jsonl(path)
                .map_err(|e| PipelineError::Dataset(e.to_string()))?;
            for doc_res in iter {
                let mut doc = doc_res.map_err(|e| PipelineError::Dataset(e.to_string()))?;
                doc.text = Preprocessor::preprocess(&doc.text, &config.dataset).into_owned();
                docs.push(doc);
            }
        }

        Ok(docs)
    }
}
