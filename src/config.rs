use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::Deserialize;
use thiserror::Error;

use crate::model::{Module, Severity};
use crate::rules::registry;

pub const CONFIG_FILE_NAME: &str = ".slopcop.toml";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleSetting {
    Disabled,
    Severity(Severity),
}

#[derive(Clone, Debug)]
pub struct Config {
    pub source_path: Option<PathBuf>,
    pub fail_level: Severity,
    pub max_file_size: u64,
    pub deadweight_enabled: bool,
    pub vibecheck_enabled: bool,
    pub rules: BTreeMap<String, RuleSetting>,
    pub path_filter: PathFilter,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            source_path: None,
            fail_level: Severity::Warning,
            max_file_size: 1_000_000,
            deadweight_enabled: true,
            vibecheck_enabled: true,
            rules: BTreeMap::new(),
            path_filter: PathFilter::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PathFilter {
    base: PathBuf,
    cwd: PathBuf,
    includes: Arc<Gitignore>,
    excludes: Arc<Gitignore>,
    has_includes: bool,
}

impl Default for PathFilter {
    fn default() -> Self {
        let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let base = absolute_path(&base, &base);
        let empty = Arc::new(
            GitignoreBuilder::new(&base)
                .build()
                .expect("empty ignore set"),
        );
        Self {
            cwd: base.clone(),
            base,
            includes: Arc::clone(&empty),
            excludes: empty,
            has_includes: false,
        }
    }
}

impl PathFilter {
    fn new(base: PathBuf, includes: &[String], excludes: &[String]) -> Result<Self, ConfigError> {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let cwd = absolute_path(&cwd, &cwd);
        let base = absolute_path(&base, &cwd);
        Ok(Self {
            cwd,
            includes: Arc::new(build_globs(&base, includes)?),
            excludes: Arc::new(build_globs(&base, excludes)?),
            base,
            has_includes: !includes.is_empty(),
        })
    }

    #[must_use]
    pub fn includes_file(&self, path: &Path) -> bool {
        let absolute = self.absolute(path);
        if !absolute.starts_with(&self.base) {
            return !self.has_includes;
        }
        !self
            .excludes
            .matched_path_or_any_parents(&absolute, false)
            .is_ignore()
            && (!self.has_includes
                || self
                    .includes
                    .matched_path_or_any_parents(&absolute, false)
                    .is_ignore())
    }

    #[must_use]
    pub fn allows_directory(&self, path: &Path) -> bool {
        let absolute = self.absolute(path);
        !absolute.starts_with(&self.base)
            || !self
                .excludes
                .matched_path_or_any_parents(&absolute, true)
                .is_ignore()
    }

    #[must_use]
    pub fn absolute(&self, path: &Path) -> PathBuf {
        absolute_path(path, &self.cwd)
    }

    pub(crate) fn display_path(&self, path: &Path) -> PathBuf {
        let absolute = self.absolute(path);
        absolute
            .strip_prefix(&self.cwd)
            .unwrap_or(&absolute)
            .to_path_buf()
    }
}

pub(crate) fn absolute_path(path: &Path, cwd: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            #[cfg(windows)]
            Component::Prefix(prefix) => match prefix.kind() {
                std::path::Prefix::VerbatimDisk(drive) => normalized.push(format!("{}:", char::from(drive))),
                std::path::Prefix::VerbatimUNC(server, share) => {
                    let mut unc = std::ffi::OsString::from("\\\\");
                    unc.push(server);
                    unc.push("\\");
                    unc.push(share);
                    normalized.push(unc);
                }
                _ => normalized.push(prefix.as_os_str()),
            },
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not read configuration {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid configuration {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("invalid severity for {key}: {message}")]
    Severity { key: String, message: String },
    #[error("unknown rule in configuration: {0}")]
    UnknownRule(String),
    #[error("invalid path pattern {pattern:?}: {message}")]
    Glob { pattern: String, message: String },
    #[error("configuration disables every rule")]
    NoRulesEnabled,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    #[serde(default)]
    slopcop: Section,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Section {
    fail_level: Option<String>,
    max_file_size: Option<u64>,
    #[serde(default)]
    deadweight: ModuleSection,
    #[serde(default)]
    vibecheck: ModuleSection,
    #[serde(default)]
    rules: BTreeMap<String, String>,
    #[serde(default)]
    ignore: IgnoreSection,
    #[serde(default)]
    files: FilesSection,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModuleSection {
    #[serde(default = "enabled_by_default")]
    enabled: bool,
}

impl Default for ModuleSection {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct IgnoreSection {
    #[serde(default)]
    paths: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilesSection {
    #[serde(default)]
    include: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
}

const fn enabled_by_default() -> bool {
    true
}

impl Config {
    /// Loads an explicit configuration or discovers the nearest project configuration.
    ///
    /// # Errors
    ///
    /// Returns an error for unreadable or invalid TOML, unknown rules, invalid globs, invalid
    /// severities, or a configuration that disables every rule.
    pub fn load(explicit: Option<&Path>) -> Result<Self, ConfigError> {
        let Some(path) = explicit.map(Path::to_path_buf).or_else(discover_config) else {
            return Ok(Self::default());
        };
        let path = fs::canonicalize(&path).map_err(|source| ConfigError::Read { path, source })?;
        let source = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
            path: path.clone(),
            source,
        })?;
        let document: Document = toml::from_str(&source).map_err(|source| ConfigError::Parse {
            path: path.clone(),
            source,
        })?;
        Self::from_document(path, document)
    }

    fn from_document(path: PathBuf, document: Document) -> Result<Self, ConfigError> {
        let section = document.slopcop;
        let fail_level = match section.fail_level {
            Some(value) => Severity::from_str(&value).map_err(|message| ConfigError::Severity {
                key: "slopcop.fail-level".to_owned(),
                message,
            })?,
            None => Severity::Warning,
        };
        let known_rules: HashSet<_> = registry()
            .into_iter()
            .map(|rule| rule.metadata().id.to_owned())
            .collect();
        let mut rules = BTreeMap::new();
        for (rule_id, value) in section.rules {
            let normalized_id = rule_id.to_ascii_uppercase();
            if !known_rules.contains(&normalized_id) {
                return Err(ConfigError::UnknownRule(rule_id));
            }
            let setting =
                if value.eq_ignore_ascii_case("off") || value.eq_ignore_ascii_case("disabled") {
                    RuleSetting::Disabled
                } else {
                    RuleSetting::Severity(Severity::from_str(&value).map_err(|message| {
                        ConfigError::Severity {
                            key: format!("slopcop.rules.{normalized_id}"),
                            message,
                        }
                    })?)
                };
            rules.insert(normalized_id, setting);
        }

        let deadweight_enabled = section.deadweight.enabled;
        let vibecheck_enabled = section.vibecheck.enabled;
        let enabled_count = registry()
            .into_iter()
            .filter(|rule| match rule.metadata().module {
                Module::Deadweight => deadweight_enabled,
                Module::Vibecheck => vibecheck_enabled,
            })
            .filter(|rule| rules.get(rule.metadata().id) != Some(&RuleSetting::Disabled))
            .count();
        if enabled_count == 0 {
            return Err(ConfigError::NoRulesEnabled);
        }

        let mut excludes = section.ignore.paths;
        excludes.extend(section.files.exclude);
        let base = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let path_filter = PathFilter::new(base, &section.files.include, &excludes)?;

        Ok(Self {
            source_path: Some(path),
            fail_level,
            max_file_size: section.max_file_size.unwrap_or(1_000_000),
            deadweight_enabled,
            vibecheck_enabled,
            rules,
            path_filter,
        })
    }

    #[must_use]
    pub fn rule_enabled(&self, rule_id: &str, module: Module) -> bool {
        let module_enabled = match module {
            Module::Deadweight => self.deadweight_enabled,
            Module::Vibecheck => self.vibecheck_enabled,
        };
        module_enabled && self.rules.get(rule_id) != Some(&RuleSetting::Disabled)
    }

    #[must_use]
    pub fn severity_for(&self, rule_id: &str, default: Severity) -> Severity {
        match self.rules.get(rule_id) {
            Some(RuleSetting::Severity(severity)) => *severity,
            Some(RuleSetting::Disabled) | None => default,
        }
    }
}

fn discover_config() -> Option<PathBuf> {
    let mut directory = std::env::current_dir().ok()?;
    loop {
        let candidate = directory.join(CONFIG_FILE_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !directory.pop() {
            return None;
        }
    }
}

fn build_globs(base: &Path, patterns: &[String]) -> Result<Gitignore, ConfigError> {
    let mut builder = GitignoreBuilder::new(base);
    for pattern in patterns {
        let normalized = pattern.strip_prefix("./").unwrap_or(pattern);
        builder
            .add_line(None, normalized)
            .map_err(|error| ConfigError::Glob {
                pattern: pattern.clone(),
                message: error.to_string(),
            })?;
    }
    builder.build().map_err(|error| ConfigError::Glob {
        pattern: patterns.join(", "),
        message: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_overrides_and_path_filters() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join(CONFIG_FILE_NAME);
        fs::write(
            &path,
            "[slopcop]\nfail-level = \"error\"\nmax-file-size = 42\n\n[slopcop.vibecheck]\nenabled = false\n\n[slopcop.rules]\nDEAD002 = \"info\"\n\n[slopcop.ignore]\npaths = [\"generated/**\"]\n",
        )
        .expect("write config");

        let config = Config::load(Some(&path)).expect("valid config");

        assert_eq!(config.fail_level, Severity::Error);
        assert_eq!(config.max_file_size, 42);
        assert!(!config.vibecheck_enabled);
        assert_eq!(
            config.severity_for("DEAD002", Severity::Warning),
            Severity::Info
        );
        assert!(
            !config
                .path_filter
                .includes_file(&directory.path().join("generated/api.rs"))
        );
        assert!(
            config
                .path_filter
                .includes_file(&directory.path().join("src/api.rs"))
        );
    }

    #[test]
    #[cfg(windows)]
    fn windows_canonical_paths_match_regular_paths() {
        let cwd = Path::new(r"C:\repo");
        assert_eq!(absolute_path(Path::new(r"\\?\C:\repo\src\app.rs"), cwd), PathBuf::from(r"C:\repo\src\app.rs"));
        assert_eq!(absolute_path(Path::new(r"\\?\UNC\server\repo\app.rs"), cwd), PathBuf::from(r"\\server\repo\app.rs"));
    }

    #[test]
    fn filters_use_git_semantics_and_config_directory() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join(CONFIG_FILE_NAME);
        fs::write(&path, "[slopcop.files]\ninclude = [\"src/**\"]\n[slopcop.ignore]\npaths = [\"ignored.py\", \"archive/\"]\n").expect("write config");
        let config = Config::load(Some(&path)).expect("load config");
        assert!(
            config
                .path_filter
                .includes_file(&directory.path().join("src/app.py"))
        );
        assert!(
            !config
                .path_filter
                .includes_file(&directory.path().join("src/nested/ignored.py"))
        );
        assert!(
            !config
                .path_filter
                .includes_file(&directory.path().join("src/archive/app.py"))
        );
        assert!(
            !config
                .path_filter
                .includes_file(&directory.path().join("docs/app.py"))
        );
    }

    #[test]
    fn rejects_unknown_rules_and_all_disabled_modules() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join(CONFIG_FILE_NAME);
        fs::write(&path, "[slopcop.rules]\nNOPE999 = \"off\"\n").expect("write config");
        assert!(matches!(
            Config::load(Some(&path)),
            Err(ConfigError::UnknownRule(_))
        ));

        fs::write(
            &path,
            "[slopcop.deadweight]\nenabled = false\n[slopcop.vibecheck]\nenabled = false\n",
        )
        .expect("write config");
        assert!(matches!(
            Config::load(Some(&path)),
            Err(ConfigError::NoRulesEnabled)
        ));

        fs::write(&path, "[slopcop]\nfail-leevl = \"warning\"\n").expect("write misspelled config");
        assert!(matches!(
            Config::load(Some(&path)),
            Err(ConfigError::Parse { .. })
        ));
    }
}
