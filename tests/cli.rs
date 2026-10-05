use std::fs;
use std::process::{Command, Output};

fn slopcop(directory: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_slopcop"))
        .current_dir(directory)
        .args(args)
        .output()
        .expect("run slopcop")
}

#[test]
fn exit_codes_formats_configuration_and_suppressions_are_stable() {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("bad.py"),
        "# TODO: implement retries\nrun()\n",
    )
    .expect("write bad fixture");
    fs::write(
        directory.path().join("clean.py"),
        "# Retry uses the server delay.\nrun()\n",
    )
    .expect("write clean fixture");

    let clean = slopcop(directory.path(), &["clean.py", "--quiet"]);
    assert_eq!(clean.status.code(), Some(0));

    let json_output = slopcop(directory.path(), &["bad.py", "--format", "json"]);
    assert_eq!(json_output.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&json_output.stdout).expect("valid JSON");
    assert_eq!(json["findings"][0]["rule_id"], "DEAD002");

    let sarif_output = slopcop(directory.path(), &["bad.py", "--format", "sarif"]);
    assert_eq!(sarif_output.status.code(), Some(1));
    let sarif: serde_json::Value =
        serde_json::from_slice(&sarif_output.stdout).expect("valid SARIF");
    assert_eq!(sarif["version"], "2.1.0");

    fs::write(
        directory.path().join("suppressed.py"),
        "# slopcop: ignore DEAD002 -- tracked as issue 42\n# TODO: remove compatibility path\n",
    )
    .expect("write suppressed fixture");
    let suppressed = slopcop(directory.path(), &["suppressed.py", "--quiet"]);
    assert_eq!(suppressed.status.code(), Some(0));

    fs::write(
        directory.path().join(".slopcop.toml"),
        "[slopcop.rules]\nDEAD002 = \"info\"\n",
    )
    .expect("write config");
    let demoted = slopcop(directory.path(), &["bad.py", "--format", "json"]);
    assert_eq!(demoted.status.code(), Some(0));
    let json: serde_json::Value = serde_json::from_slice(&demoted.stdout).expect("valid JSON");
    assert_eq!(json["findings"][0]["severity"], "info");

    let missing = slopcop(directory.path(), &["missing.py", "--quiet"]);
    assert_eq!(missing.status.code(), Some(2));
}

#[test]
fn path_filters_work_from_nested_directories_and_relative_configs() {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::create_dir_all(directory.path().join("cfg/src")).expect("create source tree");
    fs::write(
        directory.path().join("cfg/.slopcop.toml"),
        "[slopcop.files]\ninclude = [\"src/**\"]\n",
    )
    .expect("write config");
    let path = directory.path().join("cfg/src/app.py");
    fs::write(&path, "# TODO: implement retries\nrun()\n").expect("write source");
    let nested = slopcop(
        &directory.path().join("cfg/src"),
        &[".", "--format", "json"],
    );
    assert_eq!(nested.status.code(), Some(1));
    let explicit = slopcop(
        directory.path(),
        &[
            path.to_str().expect("UTF-8 path"),
            "--config",
            "cfg/.slopcop.toml",
            "--format",
            "json",
        ],
    );
    assert_eq!(explicit.status.code(), Some(1));
    let duplicate = slopcop(
        &directory.path().join("cfg/src"),
        &[".", path.to_str().expect("UTF-8 path"), "--format", "json"],
    );
    let report: serde_json::Value = serde_json::from_slice(&duplicate.stdout).expect("JSON report");
    assert_eq!(report["summary"]["scanned_files"], 1);
    assert_eq!(report["summary"]["findings"], 1);
}

#[test]
fn explain_includes_complete_rule_metadata() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let output = slopcop(directory.path(), &["explain", "VIBE001"]);

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 output");
    assert!(stdout.contains("module: vibecheck"));
    assert!(stdout.contains("Examples:"));
    assert!(stdout.contains("False positives:"));
    assert!(stdout.contains("VIBE001 = \"info|warning|error|off\""));
}

#[test]
fn commit_message_checks_placeholder_subjects_but_allows_autosquash_authoring() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let message = directory.path().join("COMMIT_EDITMSG");

    fs::write(&message, "# Commit subject\n\nWIP\n").expect("write placeholder message");
    let output = slopcop(
        directory.path(),
        &["commit-message", "COMMIT_EDITMSG", "--format", "json"],
    );
    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON report");
    assert_eq!(report["findings"][0]["rule_id"], "TRAIL001");
    assert_eq!(report["findings"][0]["module"], "papertrail");

    fs::write(&message, "fixup! Handle empty input\n").expect("write fixup message");
    let output = slopcop(
        directory.path(),
        &["commit-message", "COMMIT_EDITMSG", "--quiet"],
    );
    assert_eq!(output.status.code(), Some(0));

    fs::write(
        directory.path().join(".slopcop.toml"),
        "[slopcop.papertrail]\nenabled = false\n",
    )
    .expect("write config");
    fs::write(&message, "WIP\n").expect("restore placeholder message");
    let output = slopcop(
        directory.path(),
        &["commit-message", "COMMIT_EDITMSG", "--quiet"],
    );
    assert_eq!(output.status.code(), Some(0));
}
