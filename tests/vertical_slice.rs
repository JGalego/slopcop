use std::fs;

use slopcop::{ScanOptions, scan_paths};

#[test]
fn scanner_finds_deadweight_and_vibecheck_without_flagging_clean_neighbors() {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("bad.py"),
        "try:\n    connect()\nexcept ConnectionError:\n    pass\n",
    )
    .expect("write Python fixture");
    fs::write(
        directory.path().join("bad.md"),
        "Ultimately, use evidence. That said, be concise. At its core, this is a linter. With that in mind, run it.",
    )
    .expect("write prose fixture");
    fs::write(
        directory.path().join("clean.py"),
        "try:\n    connect()\nexcept ConnectionError as error:\n    raise RetryError() from error\n",
    )
    .expect("write clean Python fixture");
    fs::write(
        directory.path().join("clean.md"),
        "Ultimately, benchmark data decides whether the optimization stays.",
    )
    .expect("write clean prose fixture");

    let result = scan_paths(&[directory.path().to_path_buf()], &ScanOptions::default())
        .expect("scan succeeds");
    let rule_ids: Vec<_> = result
        .findings
        .iter()
        .map(|finding| finding.rule_id)
        .collect();

    assert_eq!(result.scanned_files, 4);
    assert_eq!(rule_ids, ["VIBE001", "DEAD001"]);
}
