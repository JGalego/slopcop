use std::fs;

use slopcop::{ScanOptions, scan_paths};

#[test]
fn discovery_deduplicates_and_skips_ignored_binary_large_and_generated_files() {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::create_dir(directory.path().join(".git")).expect("create Git marker");
    fs::write(directory.path().join(".gitignore"), "ignored.py\n").expect("write ignore file");
    let visible = directory.path().join("visible.py");
    fs::write(&visible, "# TODO: implement retry handling\nrun()\n").expect("write visible file");
    fs::write(
        directory.path().join("ignored.py"),
        "# TODO: ignored by Git rules\n",
    )
    .expect("write ignored file");
    fs::write(directory.path().join("binary.py"), [0, 1, 2, 3]).expect("write binary file");
    fs::write(
        directory.path().join("large.py"),
        format!("# {}\n", "padding ".repeat(20)),
    )
    .expect("write large file");
    fs::write(
        directory.path().join("generated.py"),
        "# @generated: do not edit\n# TODO: generated placeholder\n",
    )
    .expect("write generated file");
    let options = ScanOptions {
        max_file_size: 64,
        ..ScanOptions::default()
    };

    let result = scan_paths(
        &[
            directory.path().to_path_buf(),
            visible.clone(),
            directory.path().join(".").join("visible.py"),
        ],
        &options,
    )
    .expect("scan succeeds");

    assert_eq!(result.findings.len(), 1);
    assert_eq!(result.findings[0].rule_id, "DEAD002");
    assert_eq!(result.skipped_files, 3);
}

#[test]
fn configuration_values_do_not_receive_prose_rules() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("settings.json");
    fs::write(
        &path,
        r#"{"copy":"Ultimately. That said. At its core. With that in mind."}"#,
    )
    .expect("write configuration");

    let result = scan_paths(&[path], &ScanOptions::default()).expect("scan succeeds");

    assert!(result.findings.is_empty());
}
