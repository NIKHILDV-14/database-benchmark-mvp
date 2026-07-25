pub mod hashing;
pub mod loader;
pub mod metadata;
pub mod model;
pub mod preprocess;
pub mod validator;

#[cfg(test)]
mod tests {
    use super::*;
    use benchmark_config::DatasetConfig;
    use common::error::BenchmarkError;
    use std::fs;
    use std::path::PathBuf;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn tmp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dataset_test_{}", name))
    }

    fn test_config(path: &str, empty_policy: &str, trim: bool) -> DatasetConfig {
        DatasetConfig {
            path: path.to_string(),
            max_size_gb: 0.01, // 10 MB cap – generous for unit test files
            empty_text_policy: empty_policy.to_string(),
            trim_whitespace: trim,
        }
    }

    // ── existence & format ────────────────────────────────────────────────────

    #[test]
    fn test_missing_file_rejection() {
        let path = tmp_path("missing_file_123456.jsonl");
        let _ = fs::remove_file(&path); // ensure it really does not exist
        let config = test_config(path.to_str().unwrap(), "reject", true);
        let res = validator::DatasetValidator::process_dataset(&config);
        assert!(res.is_err());
        match res.unwrap_err() {
            BenchmarkError::Validation(msg) => {
                assert!(msg.contains("not found"), "Expected 'not found' in: {msg}")
            }
            e => panic!("Expected Validation error, got: {e:?}"),
        }
    }

    #[test]
    fn test_unsupported_format_rejection() {
        let path = tmp_path("unsupported.txt");
        fs::write(&path, "some text").unwrap();
        let config = test_config(path.to_str().unwrap(), "reject", true);
        let res = validator::DatasetValidator::process_dataset(&config);
        fs::remove_file(&path).unwrap();
        assert!(res.is_err());
        match res.unwrap_err() {
            BenchmarkError::Validation(msg) => assert!(
                msg.contains("Unsupported dataset format"),
                "Expected 'Unsupported dataset format' in: {msg}"
            ),
            e => panic!("Expected Validation error, got: {e:?}"),
        }
    }

    // ── size limit ────────────────────────────────────────────────────────────

    #[test]
    fn test_file_size_limit_enforcement() {
        let path = tmp_path("big.jsonl");
        // write a valid file but set a very tight limit
        let content = r#"{"id": "1", "title": "T", "text": "hello world", "metadata": {}}"#;
        fs::write(&path, content).unwrap();
        let mut config = test_config(path.to_str().unwrap(), "reject", true);
        // set limit to 1 byte — any real file will exceed it
        config.max_size_gb = 1e-12;
        let res = validator::DatasetValidator::process_dataset(&config);
        fs::remove_file(&path).unwrap();
        assert!(res.is_err());
        match res.unwrap_err() {
            BenchmarkError::Validation(msg) => {
                assert!(msg.contains("exceeds"), "Expected 'exceeds' in: {msg}")
            }
            e => panic!("Expected Validation error, got: {e:?}"),
        }
    }

    // ── valid JSONL loading ───────────────────────────────────────────────────

    #[test]
    fn test_valid_jsonl_loading() {
        let path = tmp_path("valid.jsonl");
        let content = concat!(
            r#"{"id": "1", "title": "A", "text": "foo", "metadata": {}}"#,
            "\n",
            r#"{"id": "2", "title": "B", "text": "bar", "metadata": {}}"#,
            "\n" // trailing newline — should not cause an error
        );
        fs::write(&path, content).unwrap();
        let config = test_config(path.to_str().unwrap(), "reject", true);
        let meta = validator::DatasetValidator::process_dataset(&config).unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(meta.document_count, 2, "Should count 2 documents");
        assert_eq!(meta.format, "jsonl");
    }

    #[test]
    fn test_malformed_jsonl_rejection() {
        let path = tmp_path("malformed.jsonl");
        let content = concat!(
            r#"{"id": "1", "title": "A", "text": "foo", "metadata": {}}"#,
            "\n",
            r#"{ NOT VALID JSON "#,
            "\n"
        );
        fs::write(&path, content).unwrap();
        let config = test_config(path.to_str().unwrap(), "reject", true);
        let res = validator::DatasetValidator::process_dataset(&config);
        fs::remove_file(&path).unwrap();
        assert!(res.is_err());
        match res.unwrap_err() {
            BenchmarkError::Validation(msg) => assert!(
                msg.contains("JSON parse error"),
                "Expected 'JSON parse error' in: {msg}"
            ),
            e => panic!("Expected Validation error, got: {e:?}"),
        }
    }

    // ── valid CSV loading ─────────────────────────────────────────────────────

    #[test]
    fn test_valid_csv_loading() {
        let path = tmp_path("valid.csv");
        let content = "id,title,text,category\n\
                       csv_1,Alpha,Hello world,databases\n\
                       csv_2,Beta,Goodbye world,ml\n";
        fs::write(&path, content).unwrap();
        let config = test_config(path.to_str().unwrap(), "reject", true);
        let meta = validator::DatasetValidator::process_dataset(&config).unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(meta.document_count, 2);
        assert_eq!(meta.format, "csv");
    }

    // ── ID validation ─────────────────────────────────────────────────────────

    #[test]
    fn test_duplicate_id_detection() {
        let path = tmp_path("dup_ids.jsonl");
        let content = concat!(
            r#"{"id": "dup", "title": "A", "text": "foo", "metadata": {}}"#,
            "\n",
            r#"{"id": "dup", "title": "B", "text": "bar", "metadata": {}}"#,
            "\n"
        );
        fs::write(&path, content).unwrap();
        let config = test_config(path.to_str().unwrap(), "reject", true);
        let res = validator::DatasetValidator::process_dataset(&config);
        fs::remove_file(&path).unwrap();
        assert!(res.is_err());
        match res.unwrap_err() {
            BenchmarkError::Validation(msg) => assert!(
                msg.contains("Duplicate ID detected: dup"),
                "Expected duplicate ID message, got: {msg}"
            ),
            e => panic!("Expected Validation error, got: {e:?}"),
        }
    }

    #[test]
    fn test_missing_id_field_rejection() {
        // No "id" key at all — serde must fail
        let path = tmp_path("no_id.jsonl");
        let content = r#"{"title": "A", "text": "foo", "metadata": {}}"#;
        fs::write(&path, content).unwrap();
        let config = test_config(path.to_str().unwrap(), "reject", true);
        let res = validator::DatasetValidator::process_dataset(&config);
        fs::remove_file(&path).unwrap();
        assert!(res.is_err(), "Should fail when 'id' field is missing");
    }

    #[test]
    fn test_empty_id_rejection() {
        let path = tmp_path("empty_id.jsonl");
        let content = r#"{"id": "   ", "title": "A", "text": "foo", "metadata": {}}"#;
        fs::write(&path, content).unwrap();
        let config = test_config(path.to_str().unwrap(), "reject", true);
        let res = validator::DatasetValidator::process_dataset(&config);
        fs::remove_file(&path).unwrap();
        assert!(res.is_err());
        match res.unwrap_err() {
            BenchmarkError::Validation(msg) => assert!(
                msg.contains("Empty or missing ID"),
                "Expected 'Empty or missing ID' in: {msg}"
            ),
            e => panic!("Expected Validation error, got: {e:?}"),
        }
    }

    // ── empty-text policy ─────────────────────────────────────────────────────

    #[test]
    fn test_empty_text_reject_policy() {
        let path = tmp_path("empty_text_reject.jsonl");
        // text is only whitespace — after trim it is empty
        let content = r#"{"id": "1", "title": "A", "text": "   ", "metadata": {}}"#;
        fs::write(&path, content).unwrap();
        let config = test_config(path.to_str().unwrap(), "reject", true);
        let res = validator::DatasetValidator::process_dataset(&config);
        fs::remove_file(&path).unwrap();
        assert!(
            res.is_err(),
            "Should reject empty text under 'reject' policy"
        );
    }

    #[test]
    fn test_empty_text_allow_policy() {
        let path = tmp_path("empty_text_allow.jsonl");
        let content = r#"{"id": "1", "title": "A", "text": "   ", "metadata": {}}"#;
        fs::write(&path, content).unwrap();
        let config = test_config(path.to_str().unwrap(), "allow", true);
        let res = validator::DatasetValidator::process_dataset(&config);
        fs::remove_file(&path).unwrap();
        assert!(res.is_ok(), "Should allow empty text under 'allow' policy");
    }

    // ── preprocessing ─────────────────────────────────────────────────────────

    #[test]
    fn test_preprocessing_line_ending_normalisation() {
        use benchmark_config::DatasetConfig;
        use preprocess::Preprocessor;

        let cfg = DatasetConfig {
            path: String::new(),
            max_size_gb: 2.0,
            empty_text_policy: "reject".to_string(),
            trim_whitespace: false,
        };
        let input = "line one\r\nline two\r\nline three";
        let result = Preprocessor::preprocess(input, &cfg);
        assert_eq!(result.as_ref(), "line one\nline two\nline three");
    }

    #[test]
    fn test_preprocessing_trim_whitespace() {
        use benchmark_config::DatasetConfig;
        use preprocess::Preprocessor;

        let cfg = DatasetConfig {
            path: String::new(),
            max_size_gb: 2.0,
            empty_text_policy: "reject".to_string(),
            trim_whitespace: true,
        };
        let input = "  hello world  ";
        let result = Preprocessor::preprocess(input, &cfg);
        assert_eq!(result.as_ref(), "hello world");
    }

    #[test]
    fn test_preprocessing_no_trim_when_disabled() {
        use benchmark_config::DatasetConfig;
        use preprocess::Preprocessor;

        let cfg = DatasetConfig {
            path: String::new(),
            max_size_gb: 2.0,
            empty_text_policy: "reject".to_string(),
            trim_whitespace: false,
        };
        let input = "  hello world  ";
        let result = Preprocessor::preprocess(input, &cfg);
        assert_eq!(result.as_ref(), "  hello world  ");
    }

    // ── deterministic hashing ─────────────────────────────────────────────────

    #[test]
    fn test_hash_is_deterministic_across_runs() {
        let path = tmp_path("hash_det.jsonl");
        let content = r#"{"id": "1", "title": "A", "text": "foo", "metadata": {"z": 9}}"#;
        fs::write(&path, content).unwrap();
        let config = test_config(path.to_str().unwrap(), "reject", true);

        let m1 = validator::DatasetValidator::process_dataset(&config).unwrap();
        let m2 = validator::DatasetValidator::process_dataset(&config).unwrap();
        fs::remove_file(&path).unwrap();

        assert_eq!(
            m1.dataset_hash, m2.dataset_hash,
            "Hash must be identical across runs"
        );
    }

    #[test]
    fn test_hash_stable_under_metadata_key_reorder() {
        // same logical document — only metadata key order differs
        let path1 = tmp_path("hash_order1.jsonl");
        let path2 = tmp_path("hash_order2.jsonl");
        fs::write(
            &path1,
            r#"{"id":"1","title":"A","text":"foo","metadata":{"a":1,"b":2}}"#,
        )
        .unwrap();
        fs::write(
            &path2,
            r#"{"id":"1","title":"A","text":"foo","metadata":{"b":2,"a":1}}"#,
        )
        .unwrap();

        let cfg1 = test_config(path1.to_str().unwrap(), "reject", true);
        let cfg2 = test_config(path2.to_str().unwrap(), "reject", true);
        let m1 = validator::DatasetValidator::process_dataset(&cfg1).unwrap();
        let m2 = validator::DatasetValidator::process_dataset(&cfg2).unwrap();
        fs::remove_file(&path1).unwrap();
        fs::remove_file(&path2).unwrap();

        assert_eq!(
            m1.dataset_hash, m2.dataset_hash,
            "Hash should be identical when metadata key order differs"
        );
    }

    #[test]
    fn test_hash_changes_when_text_changes() {
        let path1 = tmp_path("hash_chg1.jsonl");
        let path2 = tmp_path("hash_chg2.jsonl");
        fs::write(
            &path1,
            r#"{"id":"1","title":"A","text":"foo","metadata":{}}"#,
        )
        .unwrap();
        fs::write(
            &path2,
            r#"{"id":"1","title":"A","text":"fooo","metadata":{}}"#,
        )
        .unwrap();

        let cfg1 = test_config(path1.to_str().unwrap(), "reject", true);
        let cfg2 = test_config(path2.to_str().unwrap(), "reject", true);
        let m1 = validator::DatasetValidator::process_dataset(&cfg1).unwrap();
        let m2 = validator::DatasetValidator::process_dataset(&cfg2).unwrap();
        fs::remove_file(&path1).unwrap();
        fs::remove_file(&path2).unwrap();

        assert_ne!(
            m1.dataset_hash, m2.dataset_hash,
            "Hash must change when text changes"
        );
    }

    #[test]
    fn test_hash_changes_when_id_changes() {
        let path1 = tmp_path("hash_id1.jsonl");
        let path2 = tmp_path("hash_id2.jsonl");
        fs::write(
            &path1,
            r#"{"id":"aaa","title":"A","text":"foo","metadata":{}}"#,
        )
        .unwrap();
        fs::write(
            &path2,
            r#"{"id":"bbb","title":"A","text":"foo","metadata":{}}"#,
        )
        .unwrap();

        let cfg1 = test_config(path1.to_str().unwrap(), "reject", true);
        let cfg2 = test_config(path2.to_str().unwrap(), "reject", true);
        let m1 = validator::DatasetValidator::process_dataset(&cfg1).unwrap();
        let m2 = validator::DatasetValidator::process_dataset(&cfg2).unwrap();
        fs::remove_file(&path1).unwrap();
        fs::remove_file(&path2).unwrap();

        assert_ne!(
            m1.dataset_hash, m2.dataset_hash,
            "Hash must change when ID changes"
        );
    }

    #[test]
    fn test_hash_changes_when_preprocessing_config_changes() {
        let path = tmp_path("hash_cfg.jsonl");
        fs::write(
            &path,
            r#"{"id":"1","title":"A","text":"  hello  ","metadata":{}}"#,
        )
        .unwrap();

        let cfg_trim = test_config(path.to_str().unwrap(), "reject", true);
        let cfg_notrim = test_config(path.to_str().unwrap(), "reject", false);

        let m_trim = validator::DatasetValidator::process_dataset(&cfg_trim).unwrap();
        let m_notrim = validator::DatasetValidator::process_dataset(&cfg_notrim).unwrap();
        fs::remove_file(&path).unwrap();

        assert_ne!(
            m_trim.dataset_hash, m_notrim.dataset_hash,
            "Hash should differ when trim_whitespace changes"
        );
    }

    // ── metadata determinism ──────────────────────────────────────────────────

    #[test]
    fn test_metadata_document_count_is_correct() {
        let path = tmp_path("meta_count.jsonl");
        let content = concat!(
            r#"{"id":"a","title":"X","text":"one","metadata":{}}"#,
            "\n",
            r#"{"id":"b","title":"Y","text":"two","metadata":{}}"#,
            "\n",
            r#"{"id":"c","title":"Z","text":"three","metadata":{}}"#,
            "\n"
        );
        fs::write(&path, content).unwrap();
        let config = test_config(path.to_str().unwrap(), "reject", true);
        let meta = validator::DatasetValidator::process_dataset(&config).unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(meta.document_count, 3);
    }
}
