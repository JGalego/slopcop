use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use thiserror::Error;

use crate::config::Config;
use crate::discovery::{DiscoveryError, discover};
use crate::language::classify;
use crate::model::Finding;
use crate::rules::{ScanContext, registry};
use crate::suppression;

#[derive(Clone, Debug)]
pub struct ScanOptions {
    pub max_file_size: u64,
    pub config: Config,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_file_size: 1_000_000,
            config: Config::default(),
        }
    }
}

#[derive(Debug)]
pub struct ScanResult {
    pub scanned_files: usize,
    pub scanned_commits: usize,
    pub skipped_files: usize,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Error)]
pub enum ScanError {
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl ScanError {
    #[must_use]
    pub const fn is_usage_error(&self) -> bool {
        matches!(self, Self::Discovery(DiscoveryError::MissingPath(_)))
    }
}

enum FileOutcome {
    Scanned(Vec<Finding>),
    Skipped,
}

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

/// Discovers and scans filesystem paths with the supplied project options.
///
/// # Errors
///
/// Returns an error when discovery or a selected file read fails.
pub fn scan_paths(paths: &[PathBuf], options: &ScanOptions) -> Result<ScanResult, ScanError> {
    let paths = discover(paths, &options.config.path_filter)?;
    let rules = enabled_rules(options);
    let outcomes: Result<Vec<_>, ScanError> = paths
        .par_iter()
        .map(|path| scan_file(path, options, &rules))
        .collect();

    Ok(finish_scan(outcomes?))
}

/// Scans in-memory source files and returns findings in stable source order.
#[must_use]
pub fn scan_sources(sources: Vec<SourceFile>, options: &ScanOptions) -> ScanResult {
    let rules = enabled_rules(options);
    let outcomes = sources
        .into_par_iter()
        .filter(|source| options.config.path_filter.includes_file(&source.path))
        .map(|source| scan_bytes(&source.path, &source.bytes, options, &rules))
        .collect();
    finish_scan(outcomes)
}

fn enabled_rules(options: &ScanOptions) -> Vec<Box<dyn crate::rules::Rule>> {
    registry()
        .into_iter()
        .filter(|rule| {
            options
                .config
                .rule_enabled(rule.metadata().id, rule.metadata().module)
        })
        .collect()
}

fn finish_scan(outcomes: Vec<FileOutcome>) -> ScanResult {
    let mut scanned_files = 0;
    let mut skipped_files = 0;
    let mut findings = Vec::new();
    for outcome in outcomes {
        match outcome {
            FileOutcome::Scanned(mut file_findings) => {
                scanned_files += 1;
                findings.append(&mut file_findings);
            }
            FileOutcome::Skipped => skipped_files += 1,
        }
    }

    findings.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.location.line.cmp(&right.location.line))
            .then(left.location.column.cmp(&right.location.column))
            .then(left.rule_id.cmp(right.rule_id))
    });

    ScanResult {
        scanned_files,
        scanned_commits: 0,
        skipped_files,
        findings,
    }
}

fn scan_file(
    path: &Path,
    options: &ScanOptions,
    rules: &[Box<dyn crate::rules::Rule>],
) -> Result<FileOutcome, ScanError> {
    let metadata = fs::metadata(path).map_err(|source| ScanError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.len() > options.max_file_size {
        return Ok(FileOutcome::Skipped);
    }

    let bytes = fs::read(path).map_err(|source| ScanError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(scan_bytes(path, &bytes, options, rules))
}

fn scan_bytes(
    path: &Path,
    bytes: &[u8],
    options: &ScanOptions,
    rules: &[Box<dyn crate::rules::Rule>],
) -> FileOutcome {
    if bytes.len() as u64 > options.max_file_size {
        return FileOutcome::Skipped;
    }
    if bytes.contains(&0) {
        return FileOutcome::Skipped;
    }
    let Ok(source) = std::str::from_utf8(bytes) else {
        return FileOutcome::Skipped;
    };
    let source_type = classify(path);
    if looks_generated(source) || looks_minified(source, source_type) {
        return FileOutcome::Skipped;
    }

    let display_path = options.config.path_filter.display_path(path);
    let context = ScanContext::new(&display_path, source, source_type);
    let mut findings = Vec::new();
    for rule in rules {
        rule.check(&context, &mut findings);
    }
    suppression::apply(source, &mut findings);
    for finding in &mut findings {
        finding.severity = options
            .config
            .severity_for(finding.rule_id, finding.severity);
    }

    FileOutcome::Scanned(findings)
}

fn looks_generated(source: &str) -> bool {
    let mut end = source.len().min(2_048);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    let header = source[..end]
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .skip_while(|line| line.is_empty())
        .take_while(|line| {
            line.is_empty()
                || ["#", "//", "/*", "*", "<!--"]
                    .iter()
                    .any(|marker| line.starts_with(marker))
        })
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();
    // "Code generated", "automatically generated", "auto-generated", and "This file is
    // generated" all contain the word; the editing warning separates them from prose about
    // generators.
    header.contains("@generated")
        || header.contains("generated")
            && ["do not edit", "do not modify", "don't edit", "don't modify"]
                .iter()
                .any(|warning| header.contains(warning))
}

fn looks_minified(source: &str, source_type: crate::language::SourceType) -> bool {
    if !matches!(
        source_type,
        crate::language::SourceType::Code(
            crate::language::Language::JavaScript | crate::language::Language::TypeScript
        )
    ) {
        return false;
    }
    let mut lines = 0;
    let mut total_length = 0;
    let mut has_extreme_line = false;
    for line in source.lines() {
        lines += 1;
        total_length += line.len();
        has_extreme_line |= line.len() >= 2_000;
    }
    has_extreme_line && lines > 0 && total_length / lines >= 200
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_markers_must_be_in_leading_comment_headers() {
        assert!(looks_generated("# @generated: do not edit\nrun()\n"));
        assert!(looks_generated(
            "/* Code generated by a tool. DO NOT EDIT. */\nrun();\n"
        ));
        assert!(looks_generated(
            "// This file is generated. Do not edit!\n// see the wrapper generator\nint x;\n"
        ));
        assert!(looks_generated(
            "# AUTO-GENERATED FILE. DO NOT MODIFY.\nvalue = 1\n"
        ));
        assert!(!looks_generated(
            "// Generated values are cached by the loader.\nint x;\n"
        ));
        assert!(!looks_generated(
            "fn run() { let marker = \"@generated\"; todo!(); }\n"
        ));
        assert!(!looks_generated("run()\n# @generated\n"));
        let source = format!(
            "# {}é\n{}\n# @generated\n",
            "a".repeat(2045),
            "x".repeat(100)
        );
        assert!(!looks_generated(&source));
    }

    #[test]
    fn minified_javascript_is_skipped_without_hiding_formatted_source() {
        let minified = "const value=compute(input);".repeat(100);
        assert!(looks_minified(
            &minified,
            crate::language::SourceType::Code(crate::language::Language::JavaScript)
        ));
        assert!(!looks_minified(
            "const value = compute(input);\nreturn value;\n",
            crate::language::SourceType::Code(crate::language::Language::JavaScript)
        ));
        let embedded_asset = format!(
            "const data = \"{}\";\n{}",
            "x".repeat(2_100),
            "const value = compute(input);\n".repeat(100)
        );
        assert!(!looks_minified(
            &embedded_asset,
            crate::language::SourceType::Code(crate::language::Language::JavaScript)
        ));
        assert!(!looks_minified(
            &minified,
            crate::language::SourceType::Code(crate::language::Language::Python)
        ));
    }
}
