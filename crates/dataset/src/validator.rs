use benchmark_config::DatasetConfig;
use common::error::{BenchmarkError, Result};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::hashing::DatasetHasher;
use crate::loader::DatasetLoader;
use crate::metadata::DatasetMetadata;
use crate::preprocess::Preprocessor;

pub struct DatasetValidator;

impl DatasetValidator {
    /// Process, validate, preprocess, and hash a dataset described by `config`.
    ///
    /// # Errors
    /// Returns a structured `BenchmarkError::Validation` for any schema, format,
    /// size-limit, duplicate-ID, or empty-text violation.
    pub fn process_dataset(config: &DatasetConfig) -> Result<DatasetMetadata> {
        let path = Path::new(&config.path);

        // --- existence check ---
        if !path.exists() {
            return Err(BenchmarkError::Validation(format!(
                "Dataset file not found: {}",
                config.path
            )));
        }

        // --- file size check (fast, before full parse) ---
        let fs_meta = fs::metadata(path).map_err(|e| BenchmarkError::Validation(e.to_string()))?;
        let size_bytes = fs_meta.len();
        let max_size_bytes = (config.max_size_gb * 1024.0 * 1024.0 * 1024.0) as u64;

        if size_bytes > max_size_bytes {
            return Err(BenchmarkError::Validation(format!(
                "Dataset size {} bytes exceeds configured maximum of {} bytes ({} GB)",
                size_bytes, max_size_bytes, config.max_size_gb
            )));
        }

        // --- format detection ---
        let is_csv = config.path.ends_with(".csv");
        let is_jsonl = config.path.ends_with(".jsonl");

        if !is_csv && !is_jsonl {
            return Err(BenchmarkError::Validation(
                "Unsupported dataset format. Must be .jsonl or .csv".to_string(),
            ));
        }

        // --- streaming validation and hashing ---
        let mut seen_ids: HashSet<String> = HashSet::new();
        let mut hasher = DatasetHasher::new();
        hasher.update_with_config(&config.empty_text_policy, config.trim_whitespace);

        let mut doc_count: usize = 0;

        if is_jsonl {
            let iter = DatasetLoader::load_jsonl(path)?;
            for (record_num, doc_res) in iter.enumerate() {
                let mut doc = doc_res?;

                // validate and mutate in place
                validate_id(&doc.id, record_num + 1)?;

                if !seen_ids.insert(doc.id.clone()) {
                    return Err(BenchmarkError::Validation(format!(
                        "Duplicate ID detected: {}",
                        doc.id
                    )));
                }

                doc.text = Preprocessor::preprocess(&doc.text, config).into_owned();

                if doc.text.is_empty() && config.empty_text_policy == "reject" {
                    return Err(BenchmarkError::Validation(format!(
                        "Empty text after preprocessing at record {} (ID: {})",
                        record_num + 1,
                        doc.id
                    )));
                }

                hasher.update_document(&doc);
                doc_count += 1;
            }
        } else {
            let iter = DatasetLoader::load_csv(path)?;
            for (record_num, doc_res) in iter.enumerate() {
                let mut doc = doc_res?;

                validate_id(&doc.id, record_num + 1)?;

                if !seen_ids.insert(doc.id.clone()) {
                    return Err(BenchmarkError::Validation(format!(
                        "Duplicate ID detected: {}",
                        doc.id
                    )));
                }

                doc.text = Preprocessor::preprocess(&doc.text, config).into_owned();

                if doc.text.is_empty() && config.empty_text_policy == "reject" {
                    return Err(BenchmarkError::Validation(format!(
                        "Empty text after preprocessing at record {} (ID: {})",
                        record_num + 1,
                        doc.id
                    )));
                }

                hasher.update_document(&doc);
                doc_count += 1;
            }
        }

        Ok(DatasetMetadata {
            path: config.path.clone(),
            format: if is_csv {
                "csv".to_string()
            } else {
                "jsonl".to_string()
            },
            document_count: doc_count,
            original_size_bytes: size_bytes,
            dataset_hash: hasher.finalize(),
            empty_text_policy: config.empty_text_policy.clone(),
            trim_whitespace: config.trim_whitespace,
        })
    }
}

/// Validate that a document ID is non-empty after trimming.
fn validate_id(id: &str, record_num: usize) -> Result<()> {
    if id.trim().is_empty() {
        return Err(BenchmarkError::Validation(format!(
            "Empty or missing ID at record {}",
            record_num
        )));
    }
    Ok(())
}
