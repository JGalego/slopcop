#![forbid(unsafe_code)]

//! Browser bindings for the slopcop demo. The page lists a repository tree, passes its
//! `.gitattributes` files to [`Scanner::attributes`], asks [`Scanner::wants`] which blobs are worth
//! downloading, feeds them to [`Scanner::add`], and renders the JSON report
//! returned by [`Scanner::finish`]. [`Scanner::html`] renders the same scan as a downloadable page.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use slopcop::attributes::LinguistExclusions;
use slopcop::config::{CONFIG_FILE_NAME, Config, RuleSetting};
use slopcop::discovery::SKIPPED_DIRECTORIES;
use slopcop::language::{SourceType, classify};
use slopcop::polygraph::{
    LM_REVISION, LM_TOKENIZER_FILE, LM_TOKENIZER_SHA256, LM_WEIGHTS_FILE, LM_WEIGHTS_SHA256,
    MODEL_SHA256, LanguageModel, Model, install_language_model, install_model,
    installed_language_model, installed_model, model_file_name,
};
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

/// Returns the slopcop version these bindings were built from, as reported by `slopcop --version`.
#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Describes the Polygraph model this build accepts as JSON: its `file` name on the site and the
/// `sha256` of its contents.
///
/// # Panics
///
/// Panics if the description cannot be serialized, which would be a programming error.
#[wasm_bindgen]
#[must_use]
pub fn polygraph_model() -> String {
    serde_json::to_string(&serde_json::json!({
        "file": model_file_name(),
        "sha256": MODEL_SHA256,
    }))
    .expect("model description serializes")
}

/// Describes the pinned language-model files accepted by this build.
///
/// # Panics
///
/// Panics if the description cannot be serialized, which would be a programming error.
#[wasm_bindgen]
#[must_use]
pub fn polygraph_language_model() -> String {
    let base =
        format!("https://huggingface.co/HuggingFaceTB/SmolLM2-135M/resolve/{LM_REVISION}");
    serde_json::to_string(&serde_json::json!({
        "weights": {
            "file": LM_WEIGHTS_FILE,
            "url": format!("{base}/{LM_WEIGHTS_FILE}?download=true"),
            "sha256": LM_WEIGHTS_SHA256,
        },
        "tokenizer": {
            "file": LM_TOKENIZER_FILE,
            "url": format!("{base}/{LM_TOKENIZER_FILE}?download=true"),
            "sha256": LM_TOKENIZER_SHA256,
        },
    }))
    .expect("language-model description serializes")
}

#[wasm_bindgen]
pub struct Scanner {
    options: ScanOptions,
    attributes: LinguistExclusions,
    sources: Vec<SourceFile>,
    result: Option<ScanResult>,
    embedding_loaded: bool,
    language_model_loaded: bool,
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
            embedding_loaded: false,
            language_model_loaded: false,
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

    /// Turns the Polygraph rules on with the model file's bytes, as `--polygraph` does.
    ///
    /// # Errors
    ///
    /// Returns an error when the bytes are not the model this build accepts.
    // wasm-bindgen passes JavaScript arrays in as owned values.
    #[allow(clippy::needless_pass_by_value)]
    pub fn load_polygraph(&mut self, bytes: Vec<u8>) -> Result<(), JsError> {
        if installed_model().is_none() {
            let model =
                Model::from_bytes(&bytes).map_err(|error| JsError::new(&error.to_string()))?;
            install_model(Arc::new(model));
        }
        self.options.config.polygraph_enabled = true;
        self.embedding_loaded = true;
        Ok(())
    }

    /// Turns `POLY001` and `POLY002` on with the pinned weights and tokenizer.
    ///
    /// # Errors
    ///
    /// Returns an error when either file does not match the model this build accepts.
    #[allow(clippy::needless_pass_by_value)]
    pub fn load_polygraph_lm(
        &mut self,
        weights: Vec<u8>,
        tokenizer: Vec<u8>,
    ) -> Result<(), JsError> {
        if installed_language_model().is_none() {
            let model = LanguageModel::from_verified_bytes(&weights, &tokenizer)
                .map_err(|error| JsError::new(&error))?;
            install_language_model(Arc::new(model));
        }
        self.options.config.polygraph_enabled = true;
        self.language_model_loaded = true;
        Ok(())
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
        for rule in if self.embedding_loaded {
            &[][..]
        } else {
            &["POLY003", "POLY004"]
        } {
            self.options
                .config
                .rules
                .insert((*rule).to_owned(), RuleSetting::Disabled);
        }
        for rule in if self.language_model_loaded {
            &[][..]
        } else {
            &["POLY001", "POLY002"]
        } {
            self.options
                .config
                .rules
                .insert((*rule).to_owned(), RuleSetting::Disabled);
        }
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
    fn version_matches_the_linter() {
        let report: serde_json::Value =
            serde_json::from_str(&scanner(None).finish()).expect("valid JSON");
        assert_eq!(report["version"], version());
    }

    #[test]
    fn model_descriptions_pin_every_browser_download() {
        let embedding: serde_json::Value =
            serde_json::from_str(&polygraph_model()).expect("valid model description");
        assert_eq!(embedding["sha256"], MODEL_SHA256);
        let language: serde_json::Value =
            serde_json::from_str(&polygraph_language_model()).expect("valid LM description");
        assert_eq!(language["weights"]["sha256"], LM_WEIGHTS_SHA256);
        assert_eq!(language["tokenizer"]["sha256"], LM_TOKENIZER_SHA256);
        assert!(language["weights"]["url"].as_str().unwrap().contains(LM_REVISION));
    }

    #[test]
    fn browser_model_tiers_are_independent() {
        let mut embedding = scanner(Some("[slopcop.polygraph]\nenabled = true\n"));
        embedding.embedding_loaded = true;
        embedding.finish();
        assert!(!embedding.options.config.rules.contains_key("POLY003"));
        assert_eq!(
            embedding.options.config.rules.get("POLY001"),
            Some(&RuleSetting::Disabled)
        );

        let mut language = scanner(Some("[slopcop.polygraph]\nenabled = true\n"));
        language.language_model_loaded = true;
        language.finish();
        assert!(!language.options.config.rules.contains_key("POLY001"));
        assert_eq!(
            language.options.config.rules.get("POLY003"),
            Some(&RuleSetting::Disabled)
        );
    }

    #[test]
    fn language_model_rejects_unpinned_bytes_before_loading() {
        let error = LanguageModel::from_verified_bytes(b"not weights", b"not a tokenizer")
            .err()
            .expect("bad weights are rejected");
        assert!(error.contains(LM_WEIGHTS_SHA256));
    }

    #[test]
    fn rules_list_includes_every_polygraph_id() {
        let listed: Vec<serde_json::Value> =
            serde_json::from_str(&super::rules()).expect("valid JSON");
        let ids: Vec<&str> = listed
            .iter()
            .filter_map(|rule| rule["id"].as_str())
            .collect();
        for id in ["POLY001", "POLY002", "POLY003", "POLY004"] {
            assert!(ids.contains(&id), "{id}");
        }
    }

    #[test]
    fn embedding_model_rejects_unpinned_bytes() {
        assert!(Model::from_bytes(b"not a model").is_err());
    }

    #[test]
    fn embedding_rules_report_poly003_from_the_slop_fixture() {
        let bytes = embedding_model_bytes();
        let mut scanner = scanner(None);
        scanner.load_polygraph(bytes).expect("pinned embedding model");
        scanner.add(
            "poly003.py".to_owned(),
            include_bytes!("../../tests/fixtures/slop/poly003.py").to_vec(),
        );
        let report: serde_json::Value =
            serde_json::from_str(&scanner.finish()).expect("valid JSON");
        assert!(
            report["findings"]
                .as_array()
                .expect("findings")
                .iter()
                .any(|finding| finding["rule_id"] == "POLY003"),
            "{report}"
        );
    }

    #[test]
    fn embedding_rules_stay_quiet_on_the_clean_poly003_fixture() {
        let bytes = embedding_model_bytes();
        let mut scanner = scanner(None);
        scanner.load_polygraph(bytes).expect("pinned embedding model");
        scanner.add(
            "poly003.py".to_owned(),
            include_bytes!("../../tests/fixtures/clean/poly003.py").to_vec(),
        );
        let report: serde_json::Value =
            serde_json::from_str(&scanner.finish()).expect("valid JSON");
        assert_eq!(report["findings"].as_array().expect("findings").len(), 0);
    }

    #[test]
    fn embedding_rules_report_poly004_across_two_files() {
        let bytes = embedding_model_bytes();
        let mut scanner = scanner(None);
        scanner.load_polygraph(bytes).expect("pinned embedding model");
        scanner.add(
            "docs/a.md".to_owned(),
            include_bytes!("../../tests/fixtures/slop/poly004-a.md").to_vec(),
        );
        scanner.add(
            "docs/b.md".to_owned(),
            include_bytes!("../../tests/fixtures/slop/poly004-b.md").to_vec(),
        );
        let report: serde_json::Value =
            serde_json::from_str(&scanner.finish()).expect("valid JSON");
        assert!(
            report["findings"]
                .as_array()
                .expect("findings")
                .iter()
                .any(|finding| finding["rule_id"] == "POLY004"),
            "{report}"
        );
    }

    fn embedding_model_bytes() -> Vec<u8> {
        let named = std::env::var_os("SLOPCOP_POLYGRAPH_MODEL").map(PathBuf::from);
        let cached = slopcop::polygraph::cached_model_path();
        let bundled = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("site/models")
            .join(model_file_name());
        let path = [named, cached, Some(bundled)]
            .into_iter()
            .flatten()
            .find(|path| path.is_file())
            .unwrap_or_else(|| {
                panic!("run make web-model or make polygraph-model before testing the web crate")
            });
        std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
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
