use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use thiserror::Error;

use crate::config::absolute_path;
use crate::scanner::SourceFile;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug)]
pub struct Selection {
    pub sources: Vec<SourceFile>,
    pub changed_lines: Option<BTreeMap<PathBuf, Vec<LineRange>>>,
}

impl Selection {
    #[must_use]
    pub fn includes_line(&self, path: &Path, line: usize) -> bool {
        self.changed_lines.as_ref().is_none_or(|files| {
            files.get(path).is_some_and(|ranges| {
                ranges
                    .iter()
                    .any(|range| line >= range.start && line <= range.end)
            })
        })
    }
}

#[derive(Debug, Error)]
pub enum GitError {
    #[error("could not run git: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("not inside a Git repository")]
    NotRepository,
    #[error("git command failed: {0}")]
    Command(String),
    #[error("could not read changed file {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Loads selected staged blobs directly from the Git index.
///
/// # Errors
///
/// Returns an error outside a Git repository, when Git cannot run, or when Git cannot read an
/// index blob.
pub fn staged(path_filters: &[PathBuf]) -> Result<Selection, GitError> {
    let root = repository_root()?;
    let cwd = std::env::current_dir()?;
    let output = git_output(
        &root,
        &[
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=ACMR",
            "-z",
            "--",
        ],
    )?;
    let paths = parse_nul_paths(&output.stdout)
        .into_iter()
        .filter(|path| selected(path, path_filters, &root, &cwd))
        .collect::<Vec<_>>();
    let mut sources = Vec::with_capacity(paths.len());
    for path in paths {
        let spec = format!(":{}", path.to_string_lossy());
        let output = git_output(&root, &["show", "--no-textconv", &spec])?;
        sources.push(SourceFile {
            path: root.join(path),
            bytes: output.stdout,
        });
    }
    Ok(Selection {
        sources,
        changed_lines: None,
    })
}

/// Loads working-tree changes, or committed HEAD blobs when comparing against a base revision.
///
/// # Errors
///
/// Returns an error outside a Git repository, when Git cannot produce the comparison, or when a
/// selected working-tree file cannot be read.
pub fn diff(path_filters: &[PathBuf], base: Option<&str>) -> Result<Selection, GitError> {
    let root = repository_root()?;
    let cwd = std::env::current_dir()?;
    let revision = base.map_or_else(|| "HEAD".to_owned(), |base| format!("{base}...HEAD"));
    let output = git_output(
        &root,
        &[
            "diff",
            "--name-only",
            "--diff-filter=ACMR",
            "-z",
            &revision,
            "--",
        ],
    )?;
    let paths = parse_nul_paths(&output.stdout)
        .into_iter()
        .filter(|path| selected(path, path_filters, &root, &cwd))
        .collect::<Vec<_>>();
    let mut sources = Vec::with_capacity(paths.len());
    let mut changed_lines = BTreeMap::new();
    for path in paths {
        let absolute = root.join(&path);
        let bytes = if base.is_some() {
            let spec = format!("HEAD:{}", path.to_string_lossy());
            git_output(&root, &["show", "--no-textconv", &spec])?.stdout
        } else {
            fs::read(&absolute).map_err(|source| GitError::Read {
                path: absolute.clone(),
                source,
            })?
        };
        let patch = git_output(
            &root,
            &[
                "diff",
                "--unified=0",
                "--no-ext-diff",
                "--no-color",
                &revision,
                "--",
                &path.to_string_lossy(),
            ],
        )?;
        changed_lines.insert(absolute.clone(), parse_added_ranges(&patch.stdout));
        sources.push(SourceFile {
            path: absolute,
            bytes,
        });
    }
    Ok(Selection {
        sources,
        changed_lines: Some(changed_lines),
    })
}

fn repository_root() -> Result<PathBuf, GitError> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()?;
    if !output.status.success() {
        return Err(GitError::NotRepository);
    }
    let root = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok(PathBuf::from(root))
}

fn git_output(root: &Path, args: &[&str]) -> Result<Output, GitError> {
    let output = Command::new("git").current_dir(root).args(args).output()?;
    if output.status.success() {
        Ok(output)
    } else {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(GitError::Command(message))
    }
}

fn parse_nul_paths(bytes: &[u8]) -> Vec<PathBuf> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| PathBuf::from(String::from_utf8_lossy(path).into_owned()))
        .collect()
}

fn selected(path: &Path, filters: &[PathBuf], root: &Path, cwd: &Path) -> bool {
    if filters.is_empty() {
        return true;
    }
    filters.iter().any(|filter| {
        let absolute = absolute_path(filter, cwd);
        let selected = root.join(path);
        selected == absolute || selected.starts_with(absolute)
    })
}

fn parse_added_ranges(patch: &[u8]) -> Vec<LineRange> {
    let patch = String::from_utf8_lossy(patch);
    patch
        .lines()
        .filter_map(|line| line.strip_prefix("@@ -"))
        .filter_map(|header| {
            let (_, added) = header.split_once(" +")?;
            let range = added.split_whitespace().next()?;
            let (start, count) = range
                .split_once(',')
                .map_or((range, "1"), |(start, count)| (start, count));
            let start = start.parse::<usize>().ok()?;
            let count = count.parse::<usize>().ok()?;
            (count > 0).then_some(LineRange {
                start,
                end: start + count - 1,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_zero_context_hunks() {
        let patch = b"@@ -2,0 +3,2 @@\n+one\n+two\n@@ -9 +11 @@\n+three\n@@ -20 +22,0 @@\n-old\n";
        assert_eq!(
            parse_added_ranges(patch),
            [
                LineRange { start: 3, end: 4 },
                LineRange { start: 11, end: 11 }
            ]
        );
    }
}
