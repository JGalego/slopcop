use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use ignore::{DirEntry, WalkBuilder};
use thiserror::Error;

use crate::attributes::{ATTRIBUTES_FILE_NAME, LinguistExclusions};
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
/// Files found by walking a directory are dropped when `.gitattributes` marks them
/// `linguist-vendored` or `linguist-generated`; explicitly named files are kept.
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
    let mut found = Vec::new();
    let mut attributes = LinguistExclusions::default();

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

        for directory in enclosing_repository_directories(&root) {
            read_attributes(&mut attributes, &directory);
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
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            if entry.file_name() == ATTRIBUTES_FILE_NAME {
                if let Some(directory) = entry.path().parent() {
                    read_attributes(&mut attributes, directory);
                }
            }
            if path_filter.includes_file(entry.path()) {
                found.push(entry.into_path());
            }
        }
    }

    files.extend(found.into_iter().filter(|path| !attributes.excludes(path)));
    Ok(files.into_iter().collect())
}

fn read_attributes(attributes: &mut LinguistExclusions, directory: &Path) {
    if let Ok(contents) = fs::read_to_string(directory.join(ATTRIBUTES_FILE_NAME)) {
        attributes.add(directory, &contents);
    }
}

/// The parents of a scan root up to the top of its Git work tree, whose `.gitattributes` files
/// also govern the root. Outside a work tree there are none, so unrelated files are not read.
fn enclosing_repository_directories(root: &Path) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    for directory in root.ancestors() {
        if directory.join(".git").exists() {
            return directories;
        }
        if let Some(parent) = directory.parent() {
            directories.push(parent.to_path_buf());
        }
    }
    Vec::new()
}
