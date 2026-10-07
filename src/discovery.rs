use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use ignore::{DirEntry, WalkBuilder};
use thiserror::Error;

use crate::config::{PathFilter, absolute_path};

pub const SKIPPED_DIRECTORIES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "vendor",
    "third_party",
    "third-party",
    "thirdparty",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".venv",
    "venv",
    "__pycache__",
    "coverage",
];

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("path does not exist: {0}")]
    MissingPath(PathBuf),
    #[error("could not inspect {path}: {source}")]
    Metadata {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("file discovery failed: {0}")]
    Walk(#[from] ignore::Error),
}

fn should_visit(entry: &DirEntry) -> bool {
    if !entry.file_type().is_some_and(|kind| kind.is_dir()) {
        return true;
    }

    entry
        .file_name()
        .to_str()
        .is_none_or(|name| !SKIPPED_DIRECTORIES.contains(&name))
}

/// Finds unique files beneath the requested paths while applying ignore and project filters.
///
/// # Errors
///
/// Returns an error when an input path is missing or cannot be inspected, or when filesystem
/// traversal fails.
pub fn discover(
    paths: &[PathBuf],
    path_filter: &PathFilter,
) -> Result<Vec<PathBuf>, DiscoveryError> {
    let roots = if paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        paths.to_vec()
    };
    let mut files = BTreeSet::new();

    for root in roots {
        let metadata = fs::metadata(&root).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                DiscoveryError::MissingPath(root.clone())
            } else {
                DiscoveryError::Metadata {
                    path: root.clone(),
                    source,
                }
            }
        })?;
        let root = fs::canonicalize(&root)
            .map_err(|source| DiscoveryError::Metadata { path: root, source })?;
        let root = absolute_path(&root, &root);

        if metadata.is_file() {
            if path_filter.includes_file(&root) {
                files.insert(root);
            }
            continue;
        }

        let directory_filter = path_filter.clone();
        let walker = WalkBuilder::new(&root)
            .standard_filters(true)
            .hidden(false)
            .follow_links(false)
            .filter_entry(move |entry| {
                should_visit(entry)
                    && (!entry.file_type().is_some_and(|kind| kind.is_dir())
                        || directory_filter.allows_directory(entry.path()))
            })
            .build();

        for entry in walker {
            let entry = entry?;
            if entry.file_type().is_some_and(|kind| kind.is_file())
                && path_filter.includes_file(entry.path())
            {
                files.insert(entry.into_path());
            }
        }
    }

    Ok(files.into_iter().collect())
}
