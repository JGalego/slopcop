use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use slopcop::config::Config;
use slopcop::discovery::discover;
use slopcop::rules::metadata_registry;
use slopcop::{ScanOptions, scan_paths};

fn validate_coverage(root: &Path, config: &Config) -> Result<(), String> {
    let mut required: BTreeSet<PathBuf> = BTreeSet::from([root.join("README.md")]);
    for directory in ["src", "docs"] {
        for entry in ignore::WalkBuilder::new(root.join(directory))
            .standard_filters(false)
            .build()
        {
            let entry = entry.map_err(|error| error.to_string())?;
            if entry.file_type().is_some_and(|kind| kind.is_file()) {
                required.insert(entry.into_path());
            }
        }
    }
    let discovered: BTreeSet<_> = discover(&[root.to_path_buf()], &config.path_filter)
        .map_err(|error| error.to_string())?
        .into_iter()
        .collect();
    if let Some(path) = required.difference(&discovered).next() {
        return Err(format!(
            "self-lint excludes required artifact {}",
            path.display()
        ));
    }
    let options = ScanOptions {
        max_file_size: config.max_file_size,
        config: config.clone(),
    };
    let result = scan_paths(&required.iter().cloned().collect::<Vec<_>>(), &options)
        .map_err(|error| error.to_string())?;
    if result.scanned_files != required.len() || result.skipped_files != 0 {
        return Err(
            "self-lint skips required artifacts through size or generated-file detection".into(),
        );
    }
    Ok(())
}

#[test]
fn self_lint_keeps_every_module_enabled_and_source_in_scope() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = root.join(".slopcop.toml");
    let config = Config::load(Some(&config_path)).expect("valid self-lint config");

    assert!(config.deadweight_enabled, "deadweight must remain enabled");
    assert!(config.papertrail_enabled, "papertrail must remain enabled");
    assert!(config.vibecheck_enabled, "vibecheck must remain enabled");
    validate_coverage(root, &config)
        .expect("self-lint must scan all source and documentation artifacts");
    assert!(
        !config
            .path_filter
            .includes_file(&root.join("tests/fixtures/slop/dead001.py")),
        "intentional positive fixtures should be excluded from self-lint"
    );
    for metadata in metadata_registry() {
        assert!(
            config.rule_enabled(metadata.id, metadata.module),
            "self-lint disabled {}",
            metadata.id
        );
    }
}

#[test]
fn coverage_guard_rejects_selective_exclusions_and_size_bypasses() {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::create_dir(directory.path().join("src")).expect("source directory");
    fs::create_dir(directory.path().join("docs")).expect("documentation directory");
    fs::write(directory.path().join("README.md"), "# Test project\n").expect("readme");
    fs::write(
        directory.path().join("src/lib.rs"),
        "pub fn run() { execute(); }\n",
    )
    .expect("library");
    fs::write(
        directory.path().join("src/main.rs"),
        "fn main() { run(); }\n",
    )
    .expect("binary");
    let path = directory.path().join(".slopcop.toml");
    for config in [
        "[slopcop.ignore]\npaths = [\"src/main.rs\"]\n",
        "[slopcop]\nmax-file-size = 1\n",
    ] {
        fs::write(&path, config).expect("config");
        let config = Config::load(Some(&path)).expect("load config");
        assert!(validate_coverage(directory.path(), &config).is_err());
    }
}

#[test]
fn repository_passes_its_own_rules() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config = Config::load(Some(&root.join(".slopcop.toml"))).expect("valid self-lint config");
    let options = ScanOptions {
        max_file_size: config.max_file_size,
        config,
    };

    let result = scan_paths(&[root.to_path_buf()], &options).expect("self scan succeeds");

    assert!(result.scanned_files > 0);
    assert!(
        result.findings.is_empty(),
        "repository findings: {:?}",
        result.findings
    );
}
