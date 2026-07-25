use benchmark_config::DatasetConfig;
use std::borrow::Cow;

pub struct Preprocessor;

impl Preprocessor {
    pub fn preprocess<'a>(text: &'a str, config: &DatasetConfig) -> Cow<'a, str> {
        let mut processed = Cow::Borrowed(text);

        // Normalize line endings
        if processed.contains("\r\n") {
            processed = Cow::Owned(processed.replace("\r\n", "\n"));
        }

        // Trim whitespace
        if config.trim_whitespace {
            if processed.starts_with(char::is_whitespace)
                || processed.ends_with(char::is_whitespace)
            {
                processed = Cow::Owned(processed.trim().to_string());
            }
        }

        processed
    }
}
