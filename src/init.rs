use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::config::CONFIG_FILE_NAME;

const CONFIG: &str = r#"[slopcop]
fail-level = "warning"
max-file-size = 1000000

[slopcop.deadweight]
enabled = true

[slopcop.papertrail]
enabled = true

[slopcop.vibecheck]
enabled = true

[slopcop.rules]

[slopcop.ignore]
paths = [
  "vendor/**",
  "generated/**",
]

[slopcop.files]
include = []
exclude = []
"#;

const PRE_COMMIT: &str = r"repos:
  - repo: https://github.com/JGalego/slopcop
    rev: v0.2.0
    hooks:
      - id: slopcop
            - id: slopcop-commit-msg
";

#[derive(Clone, Debug)]
pub struct InitResult {
    pub created: Vec<PathBuf>,
    pub existing: Vec<PathBuf>,
}

#[derive(Debug, Error)]
pub enum InitError {
    #[error("could not inspect {path}: {source}")]
    Inspect {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not create {path}: {source}")]
    Create {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Creates the default configuration and pre-commit files without overwriting either one.
///
/// # Errors
///
/// Returns an error when an existing path cannot be inspected or a missing file cannot be
/// created and written.
pub fn initialize(directory: &Path) -> Result<InitResult, InitError> {
    let mut result = InitResult {
        created: Vec::new(),
        existing: Vec::new(),
    };
    create_if_missing(directory.join(CONFIG_FILE_NAME), CONFIG, &mut result)?;
    create_if_missing(
        directory.join(".pre-commit-config.yaml"),
        PRE_COMMIT,
        &mut result,
    )?;
    Ok(result)
}

fn create_if_missing(
    path: PathBuf,
    contents: &str,
    result: &mut InitResult,
) -> Result<(), InitError> {
    match fs::metadata(&path) {
        Ok(_) => {
            result.existing.push(path);
            return Ok(());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => return Err(InitError::Inspect { path, source }),
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|source| InitError::Create {
            path: path.clone(),
            source,
        })?;
    file.write_all(contents.as_bytes())
        .map_err(|source| InitError::Create {
            path: path.clone(),
            source,
        })?;
    result.created.push(path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialization_never_overwrites_existing_files() {
        let directory = tempfile::tempdir().expect("temporary directory");
        fs::write(directory.path().join(CONFIG_FILE_NAME), "custom = true\n")
            .expect("write custom config");

        let result = initialize(directory.path()).expect("initialize project");

        assert_eq!(
            result.created,
            [directory.path().join(".pre-commit-config.yaml")]
        );
        assert_eq!(result.existing, [directory.path().join(CONFIG_FILE_NAME)]);
        assert_eq!(
            fs::read_to_string(directory.path().join(CONFIG_FILE_NAME)).expect("read config"),
            "custom = true\n"
        );
        let hooks = fs::read_to_string(directory.path().join(".pre-commit-config.yaml"))
            .expect("read hook config");
        assert!(hooks.contains("id: slopcop-commit-msg"));
    }
}
