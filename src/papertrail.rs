use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::config::Config;
use crate::git;
use crate::rules::papertrail::{CommitContext, registry};
use crate::scanner::ScanResult;

#[derive(Debug, Error)]
pub enum PapertrailError {
    #[error("could not read commit message {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error(transparent)]
    Git(#[from] git::GitError),
    #[error("git returned malformed commit history")]
    MalformedHistory,
}

/// Scans a commit message file supplied by a `commit-msg` hook.
///
/// # Errors
///
/// Returns an error when the message file cannot be read.
pub fn scan_message_file(path: &Path, config: &Config) -> Result<ScanResult, PapertrailError> {
    let source = fs::read_to_string(path).map_err(|source| PapertrailError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(scan_messages([(path.to_path_buf(), source)], false, config))
}

/// Scans commits reachable from `HEAD`, optionally restricted to commits after a base revision.
///
/// # Errors
///
/// Returns an error outside a Git repository or when Git cannot resolve or read the requested
/// history.
pub fn scan_history(
    base: Option<&str>,
    max_count: usize,
    config: &Config,
) -> Result<ScanResult, PapertrailError> {
    let root = git::repository_root()?;
    let max_count = max_count.to_string();
    let range = if let Some(base) = base {
        let merge_base = git::git_output(&root, &["merge-base", base, "HEAD"])?;
        let merge_base = String::from_utf8_lossy(&merge_base.stdout)
            .trim()
            .to_owned();
        Some(format!("{merge_base}..HEAD"))
    } else {
        None
    };
    let mut args = vec!["log", "-z", "--format=%H%x00%B", "--max-count", &max_count];
    args.push(range.as_deref().unwrap_or("HEAD"));
    let output = git::git_output(&root, &args)?;
    let commits = parse_history(&output.stdout)?;
    let messages = commits.into_iter().map(|(commit, message)| {
        (
            PathBuf::from("commit").join(commit),
            String::from_utf8_lossy(&message).into_owned(),
        )
    });
    Ok(scan_messages(messages, true, config))
}

fn scan_messages(
    messages: impl IntoIterator<Item = (PathBuf, String)>,
    from_history: bool,
    config: &Config,
) -> ScanResult {
    let rules: Vec<_> = registry()
        .into_iter()
        .filter(|rule| config.rule_enabled(rule.metadata().id, rule.metadata().module))
        .collect();
    let mut scanned_commits = 0;
    let mut findings = Vec::new();
    for (path, source) in messages {
        scanned_commits += 1;
        let context = CommitContext::new(&path, &source, from_history);
        for rule in &rules {
            rule.check(&context, &mut findings);
        }
    }
    for finding in &mut findings {
        finding.severity = config.severity_for(finding.rule_id, finding.severity);
    }
    findings.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.location.line.cmp(&right.location.line))
            .then(left.location.column.cmp(&right.location.column))
            .then(left.rule_id.cmp(right.rule_id))
    });
    ScanResult {
        scanned_files: 0,
        scanned_commits,
        skipped_files: 0,
        findings,
    }
}

fn parse_history(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, PapertrailError> {
    let chunks: Vec<_> = bytes.split(|byte| *byte == 0).collect();
    if chunks.last() != Some(&&[][..]) || chunks.len() % 2 == 0 {
        return Err(PapertrailError::MalformedHistory);
    }
    let mut commits = Vec::new();
    let mut index = 0;
    while index + 1 < chunks.len() {
        let message = chunks[index + 1];
        let commit = std::str::from_utf8(chunks[index])
            .ok()
            .filter(|value| value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or(PapertrailError::MalformedHistory)?;
        commits.push((commit.to_owned(), message.to_vec()));
        index += 2;
    }
    Ok(commits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nul_delimited_git_history() {
        let first = "a".repeat(40);
        let second = "b".repeat(40);
        let bytes = format!("{first}\0WIP\n\0{second}\0Describe behavior\n\0");
        let commits = parse_history(bytes.as_bytes()).expect("parse history");
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].0, first);
        assert_eq!(commits[0].1, b"WIP\n");
    }

    #[test]
    fn rejects_truncated_git_history() {
        let bytes = format!("{}\0", "a".repeat(40));
        assert!(matches!(
            parse_history(bytes.as_bytes()),
            Err(PapertrailError::MalformedHistory)
        ));
    }
}
