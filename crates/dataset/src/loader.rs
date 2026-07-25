use crate::model::Document;
use common::error::{BenchmarkError, Result};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub struct DatasetLoader;

impl DatasetLoader {
    /// Returns a streaming iterator over `Document`s parsed from a JSONL file.
    ///
    /// Each non-empty line is expected to be a valid JSON object matching `Document`.
    /// Trailing empty lines (e.g. a single trailing newline) are silently skipped.
    /// Genuinely malformed JSON or schema violations produce an error with line context.
    pub fn load_jsonl<P: AsRef<Path>>(path: P) -> Result<impl Iterator<Item = Result<Document>>> {
        let file = File::open(path)
            .map_err(|e| BenchmarkError::Validation(format!("Cannot open file: {}", e)))?;
        let reader = BufReader::new(file);

        let iter = reader.lines().enumerate().filter_map(|(i, line_res)| {
            match line_res {
                Err(e) => Some(Err(BenchmarkError::Validation(format!(
                    "Read error at line {}: {}",
                    i + 1,
                    e
                )))),
                Ok(line) if line.trim().is_empty() => {
                    // skip blank / trailing-newline lines
                    None
                }
                Ok(line) => {
                    let doc_res: std::result::Result<Document, _> = serde_json::from_str(&line);
                    Some(doc_res.map_err(|e| {
                        BenchmarkError::Validation(format!(
                            "JSON parse error at line {}: {}",
                            i + 1,
                            e
                        ))
                    }))
                }
            }
        });

        Ok(iter)
    }

    /// Returns a streaming iterator over `Document`s parsed from a CSV file.
    ///
    /// Required columns: `id`, `text`.
    /// Optional columns: `title`, any other column is stored as a string in `metadata`.
    pub fn load_csv<P: AsRef<Path>>(path: P) -> Result<impl Iterator<Item = Result<Document>>> {
        let file = File::open(path)
            .map_err(|e| BenchmarkError::Validation(format!("Cannot open file: {}", e)))?;
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(file);

        let headers = reader
            .headers()
            .map_err(|e| BenchmarkError::Validation(format!("CSV header error: {}", e)))?
            .clone();

        let iter = reader
            .into_records()
            .enumerate()
            .map(move |(i, record_res)| {
                let record = record_res.map_err(|e| {
                    BenchmarkError::Validation(format!("CSV error at line {}: {}", i + 2, e))
                })?;

                let mut id = String::new();
                let mut title: Option<String> = None;
                let mut text = String::new();
                let mut metadata_map = serde_json::Map::new();

                for (idx, header) in headers.iter().enumerate() {
                    let val = record.get(idx).unwrap_or("");
                    match header {
                        "id" => id = val.to_string(),
                        "title" => {
                            title = if val.is_empty() {
                                None
                            } else {
                                Some(val.to_string())
                            }
                        }
                        "text" => text = val.to_string(),
                        _ => {
                            metadata_map.insert(
                                header.to_string(),
                                serde_json::Value::String(val.to_string()),
                            );
                        }
                    }
                }

                Ok(Document {
                    id,
                    title,
                    text,
                    metadata: serde_json::Value::Object(metadata_map),
                })
            });

        Ok(iter)
    }
}
