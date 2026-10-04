use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn command(directory: &Path, program: &str, args: &[&str]) -> Output {
    let output = Command::new(program)
        .current_dir(directory)
        .args(args)
        .output()
        .expect("run command");
    assert!(
        output.status.success(),
        "{program} {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn init_repository(directory: &Path) {
    command(directory, "git", &["init", "--quiet"]);
    command(
        directory,
        "git",
        &["config", "user.email", "slopcop@example.invalid"],
    );
    command(directory, "git", &["config", "user.name", "slopcop tests"]);
}

fn slopcop(directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_slopcop"))
        .current_dir(directory)
        .args(args)
        .output()
        .expect("run slopcop")
}

#[test]
fn staged_mode_reads_the_index_not_the_worktree() {
    let directory = tempfile::tempdir().expect("temporary directory");
    init_repository(directory.path());
    let path = directory.path().join("app.py");
    fs::write(&path, "run()\n").expect("write initial file");
    command(directory.path(), "git", &["add", "app.py"]);
    command(
        directory.path(),
        "git",
        &["commit", "--quiet", "-m", "initial"],
    );

    fs::write(&path, "# TODO: implement retries\nrun()\n").expect("write staged slop");
    command(directory.path(), "git", &["add", "app.py"]);
    fs::write(&path, "run()\n").expect("restore clean worktree only");

    let output = slopcop(directory.path(), &["--staged", "--format", "json"]);
    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert_eq!(report["findings"][0]["rule_id"], "DEAD002");
}

#[test]
fn nested_git_paths_and_configuration_share_one_base() {
    let directory = tempfile::tempdir().expect("temporary directory");
    init_repository(directory.path());
    fs::create_dir(directory.path().join("src")).expect("source directory");
    fs::write(
        directory.path().join(".slopcop.toml"),
        "[slopcop.files]\ninclude = [\"src/**\"]\n",
    )
    .expect("config");
    fs::write(
        directory.path().join("src/app.py"),
        "# TODO: implement retries\nrun()\n",
    )
    .expect("source");
    fs::write(
        directory.path().join("outside.py"),
        "# TODO: outside selected directory\n",
    )
    .expect("outside source");
    command(directory.path(), "git", &["add", "."]);
    for path in ["app.py", ".", "./app.py"] {
        let output = slopcop(
            &directory.path().join("src"),
            &["--staged", path, "--format", "json"],
        );
        assert_eq!(
            output.status.code(),
            Some(1),
            "{path}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("JSON report");
        assert_eq!(report["summary"]["findings"], 1);
        assert_eq!(report["findings"][0]["path"], "app.py");
    }
}

#[test]
fn diff_reports_new_local_occurrences_after_existing_findings() {
    let directory = tempfile::tempdir().expect("temporary directory");
    init_repository(directory.path());
    let path = directory.path().join("app.rs");
    fs::write(
        &path,
        "fn existing() { todo!(); }\n\nfn added() { execute(); }\n",
    )
    .expect("baseline");
    command(directory.path(), "git", &["add", "."]);
    command(
        directory.path(),
        "git",
        &["commit", "--quiet", "-m", "baseline"],
    );
    fs::write(
        &path,
        "fn existing() { todo!(); }\n\nfn added() { todo!(); }\n",
    )
    .expect("changed source");
    let output = slopcop(directory.path(), &["--diff", "--format", "json"]);
    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON report");
    assert_eq!(report["summary"]["findings"], 1);
    assert_eq!(report["findings"][0]["location"]["line"], 3);
}

#[test]
fn changed_mode_uses_committed_content_even_with_dirty_or_missing_worktree_files() {
    let directory = tempfile::tempdir().expect("temporary directory");
    init_repository(directory.path());
    let path = directory.path().join("app.rs");
    fs::write(&path, "fn run() { execute(); }\n").expect("baseline");
    command(directory.path(), "git", &["add", "."]);
    command(
        directory.path(),
        "git",
        &["commit", "--quiet", "-m", "baseline"],
    );
    let base = command(directory.path(), "git", &["rev-parse", "HEAD"]);
    let base = String::from_utf8(base.stdout).expect("commit ID");
    fs::write(&path, "fn run() { todo!(); }\n").expect("committed change");
    command(directory.path(), "git", &["add", "."]);
    command(
        directory.path(),
        "git",
        &["commit", "--quiet", "-m", "placeholder"],
    );
    let args = ["--changed", "--base", base.trim(), "--format", "json"];
    let clean = slopcop(directory.path(), &args);
    fs::write(
        &path,
        format!("{}fn run() {{ execute(); }}\n", "\n".repeat(20)),
    )
    .expect("dirty source");
    let dirty = slopcop(directory.path(), &args);
    fs::remove_file(&path).expect("remove worktree source");
    let missing = slopcop(directory.path(), &args);
    assert_eq!(clean.status.code(), Some(1));
    assert_eq!(dirty.stdout, clean.stdout);
    assert_eq!(missing.stdout, clean.stdout);
}

#[test]
fn diff_mode_reports_only_changed_hunk_lines() {
    let directory = tempfile::tempdir().expect("temporary directory");
    init_repository(directory.path());
    let path = directory.path().join("app.py");
    fs::write(
        &path,
        "try:\n    run()\nexcept RuntimeError:\n    pass\nvalue = 1\n",
    )
    .expect("write baseline");
    command(directory.path(), "git", &["add", "app.py"]);
    command(
        directory.path(),
        "git",
        &["commit", "--quiet", "-m", "initial"],
    );

    fs::write(
        &path,
        "try:\n    run()\nexcept RuntimeError:\n    pass\nvalue = 2\n",
    )
    .expect("change clean line");
    let clean_diff = slopcop(directory.path(), &["--diff", "--format", "json"]);
    assert_eq!(clean_diff.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&clean_diff.stdout).expect("valid JSON");
    assert_eq!(report["summary"]["findings"], 0);

    fs::write(
        &path,
        "try:\n    run()\nexcept RuntimeError:\n    pass\n# TODO: replace fallback\nvalue = 2\n",
    )
    .expect("add changed slop");
    let bad_diff = slopcop(directory.path(), &["--diff", "--format", "json"]);
    assert_eq!(bad_diff.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&bad_diff.stdout).expect("valid JSON");
    let ids: Vec<_> = report["findings"]
        .as_array()
        .expect("findings array")
        .iter()
        .map(|finding| finding["rule_id"].as_str().expect("rule id"))
        .collect();
    assert_eq!(ids, ["DEAD002"]);
}
