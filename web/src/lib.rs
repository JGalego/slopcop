#![forbid(unsafe_code)]

//! Browser bindings for the slopcop demo. The page lists a repository tree, asks [`Scanner::wants`]
//! which blobs are worth downloading, feeds them to [`Scanner::add`], and renders the JSON report
//! returned by [`Scanner::finish`].

use std::path::{Component, Path, PathBuf};

use slopcop::config::{CONFIG_FILE_NAME, Config};
use slopcop::discovery::SKIPPED_DIRECTORIES;
use slopcop::language::{SourceType, classify};
use slopcop::reporting::write_json;
use slopcop::rules::metadata_registry;
use slopcop::{ScanOptions, SourceFile, scan_sources};
use wasm_bindgen::prelude::*;

/// Returns metadata for every registered rule as a JSON array.
///
/// # Panics
///
/// Panics if rule metadata cannot be serialized, which would be a programming error.
#[wasm_bindgen]
#[must_use]
pub fn rules() -> String {
    serde_json::to_string(&metadata_registry()).expect("rule metadata serializes")
}

#[wasm_bindgen]
pub struct Scanner {
    options: ScanOptions,
    sources: Vec<SourceFile>,
}

#[wasm_bindgen]
impl Scanner {
    /// Builds a scanner from the repository's `.slopcop.toml` text, or the defaults when absent.
    ///
    /// # Errors
    ///
    /// Returns the configuration error message when the TOML is invalid.
    #[wasm_bindgen(constructor)]
    pub fn new(config: Option<String>) -> Result<Self, JsError> {
        let config = match config {
            Some(source) => Config::from_toml(PathBuf::from(CONFIG_FILE_NAME), &source)
                .map_err(|error| JsError::new(&error.to_string()))?,
            None => Config::default(),
        };
        Ok(Self {
            options: ScanOptions {
                max_file_size: config.max_file_size,
                config,
            },
            sources: Vec::new(),
        })
    }

    /// Reports whether a repository blob would be scanned, so the page can skip downloading it.
    #[must_use]
    pub fn wants(&self, path: &str, size: f64) -> bool {
        wanted(Path::new(path), size, &self.options)
    }

    pub fn add(&mut self, path: String, bytes: Vec<u8>) {
        self.sources.push(SourceFile {
            path: PathBuf::from(path),
            bytes,
        });
    }

    /// Scans every added file and returns the same JSON document as `slopcop --format json`.
    ///
    /// # Panics
    ///
    /// Panics if the report cannot be serialized, which would be a programming error.
    pub fn finish(&mut self) -> String {
        let result = scan_sources(std::mem::take(&mut self.sources), &self.options);
        let mut output = Vec::new();
        write_json(&mut output, &result).expect("report serializes");
        String::from_utf8(output).expect("report is UTF-8")
    }
}

fn wanted(path: &Path, size: f64, options: &ScanOptions) -> bool {
    #[allow(clippy::cast_precision_loss)]
    let within_limit = size <= options.max_file_size as f64;
    within_limit
        && classify(path) != SourceType::Unknown
        && !path.components().any(|component| match component {
            Component::Normal(name) => name
                .to_str()
                .is_some_and(|name| SKIPPED_DIRECTORIES.contains(&name)),
            _ => false,
        })
        && options.config.path_filter.includes_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scanner(config: Option<&str>) -> Scanner {
        Scanner::new(config.map(str::to_owned)).expect("valid config")
    }

    #[test]
    fn selects_known_source_types_outside_dependency_directories() {
        let scanner = scanner(None);
        assert!(scanner.wants("README.md", 10.0));
        assert!(scanner.wants("src/app.py", 10.0));
        assert!(!scanner.wants("assets/logo.png", 10.0));
        assert!(!scanner.wants("node_modules/pkg/index.js", 10.0));
        assert!(!scanner.wants("src/huge.rs", 2_000_000.0));
    }

    #[test]
    fn honors_repository_ignore_paths() {
        let scanner = scanner(Some("[slopcop.ignore]\npaths = [\"fixtures/**\"]\n"));
        assert!(!scanner.wants("fixtures/slop.py", 10.0));
        assert!(scanner.wants("src/app.py", 10.0));
    }

    #[test]
    fn reports_findings_with_repository_relative_paths() {
        let mut scanner = scanner(None);
        scanner.add(
            "src/app.py".to_owned(),
            b"def load():\n    try:\n        run()\n    except Exception:\n        pass\n".to_vec(),
        );
        let report: serde_json::Value =
            serde_json::from_str(&scanner.finish()).expect("valid JSON");
        assert_eq!(report["summary"]["scanned_files"], 1);
        assert_eq!(report["findings"][0]["path"], "src/app.py");
    }
}
