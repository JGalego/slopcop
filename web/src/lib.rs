#![forbid(unsafe_code)]

//! Browser bindings for the slopcop demo. The page lists a repository tree, passes its
//! `.gitattributes` files to [`Scanner::attributes`], asks [`Scanner::wants`] which blobs are worth
//! downloading, feeds them to [`Scanner::add`], and renders the JSON report
//! returned by [`Scanner::finish`]. [`Scanner::html`] renders the same scan as a downloadable page.

use std::path::{Component, Path, PathBuf};

use slopcop::attributes::LinguistExclusions;
use slopcop::config::{CONFIG_FILE_NAME, Config};
use slopcop::discovery::SKIPPED_DIRECTORIES;
use slopcop::language::{SourceType, classify};
use slopcop::reporting::{HtmlContext, write_html, write_json};
use slopcop::rules::metadata_registry;
use slopcop::{ScanOptions, ScanResult, SourceFile, scan_sources};
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
    attributes: LinguistExclusions,
    sources: Vec<SourceFile>,
    result: Option<ScanResult>,
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
            attributes: LinguistExclusions::default(),
            sources: Vec::new(),
            result: None,
        })
    }

    /// Reads a repository `.gitattributes` file, given its repository-relative path, so
    /// [`Scanner::wants`] skips the files it marks `linguist-vendored` or `linguist-generated`.
    pub fn attributes(&mut self, path: &str, contents: &str) {
        let directory = Path::new(path).parent().unwrap_or(Path::new(""));
        self.attributes.add(directory, contents);
    }

    /// Reports whether a repository blob would be scanned, so the page can skip downloading it.
    #[must_use]
    pub fn wants(&self, path: &str, size: f64) -> bool {
        let path = Path::new(path);
        wanted(path, size, &self.options) && !self.attributes.excludes(path)
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
        self.result = Some(result);
        String::from_utf8(output).expect("report is UTF-8")
    }

    /// Renders the last finished scan as the same page as `slopcop --format html`. Findings link to
    /// `{source_url}{path}#L{line}`, and each note is listed under the summary.
    ///
    /// # Errors
    ///
    /// Returns an error when called before [`Scanner::finish`].
    ///
    /// # Panics
    ///
    /// Panics if the report is not UTF-8, which would be a programming error.
    // wasm-bindgen passes JavaScript strings and arrays in as owned values.
    #[allow(clippy::needless_pass_by_value)]
    pub fn html(
        &self,
        title: Option<String>,
        source_url: Option<String>,
        notes: Vec<String>,
    ) -> Result<String, JsError> {
        let result = self
            .result
            .as_ref()
            .ok_or_else(|| JsError::new("finish the scan before rendering a report"))?;
        let mut output = Vec::new();
        write_html(
            &mut output,
            result,
            &HtmlContext {
                title: title.as_deref(),
                source_url: source_url.as_deref(),
                notes: &notes,
            },
        )
        .expect("writing to memory succeeds");
        Ok(String::from_utf8(output).expect("report is UTF-8"))
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
        assert!(!scanner.wants("third_party/zlib/inflate.c", 10.0));
        assert!(!scanner.wants("src/huge.rs", 2_000_000.0));
    }

    #[test]
    fn honors_repository_ignore_paths() {
        let scanner = scanner(Some("[slopcop.ignore]\npaths = [\"fixtures/**\"]\n"));
        assert!(!scanner.wants("fixtures/slop.py", 10.0));
        assert!(scanner.wants("src/app.py", 10.0));
    }

    #[test]
    fn honors_linguist_attributes() {
        let mut scanner = scanner(None);
        scanner.attributes(".gitattributes", "deps/** linguist-vendored\n");
        scanner.attributes("web/.gitattributes", "*.gen.js linguist-generated\n");
        assert!(!scanner.wants("deps/zlib/inflate.c", 10.0));
        assert!(!scanner.wants("web/api.gen.js", 10.0));
        assert!(scanner.wants("api.gen.js", 10.0));
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

        let html = scanner
            .html(
                Some("owner/repo".to_owned()),
                Some("https://github.com/owner/repo/blob/abc/".to_owned()),
                vec!["Scanned in a test.".to_owned()],
            )
            .expect("finished scan renders");
        assert!(html.contains("href=\"https://github.com/owner/repo/blob/abc/src/app.py#L4\""));
        assert!(html.contains("<li>Scanned in a test.</li>"));
    }
}
