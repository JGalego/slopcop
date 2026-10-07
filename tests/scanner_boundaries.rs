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
fn discovery_skips_third_party_directories_and_linguist_excluded_files() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path();
    fs::create_dir(root.join(".git")).expect("create Git marker");
    fs::write(
        root.join(".gitattributes"),
        "src/deps/** linguist-vendored\nsrc/deps/ours.py -linguist-vendored\n",
    )
    .expect("write attributes");
    for name in ["src/deps", "src/web", "src/third_party/zlib"] {
        fs::create_dir_all(root.join(name)).expect("create directory");
    }
    fs::write(
        root.join("src/web/.gitattributes"),
        "*.gen.py linguist-generated\n",
    )
    .expect("write nested attributes");
    let source = "# TODO: implement retry handling\nrun()\n";
    for name in [
        "src/app.py",
        "src/deps/vendored.py",
        "src/deps/ours.py",
        "src/web/api.gen.py",
        "src/third_party/zlib/inflate.py",
    ] {
        fs::write(root.join(name), source).expect("write source");
    }

    // Scanning a subdirectory still applies the `.gitattributes` at the top of the work tree.
    let result = scan_paths(&[root.join("src")], &ScanOptions::default()).expect("scan succeeds");
    let mut paths = result
        .findings
        .iter()
        .map(|finding| finding.path.clone())
        .collect::<Vec<_>>();
    paths.sort();
    assert_eq!(paths.len(), 2, "{paths:?}");
    assert!(paths[0].ends_with("src/app.py"), "{paths:?}");
    assert!(paths[1].ends_with("src/deps/ours.py"), "{paths:?}");

    // A file named on the command line is scanned even when its attributes exclude it.
    let explicit = scan_paths(
        &[root.join("src/deps/vendored.py")],
        &ScanOptions::default(),
    )
    .expect("scan succeeds");
    assert_eq!(explicit.findings.len(), 1);
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

    assert_eq!(result.findings, [] as [slopcop::Finding; 0]);
}
