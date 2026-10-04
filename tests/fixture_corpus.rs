use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use slopcop::rules::registry;
use slopcop::{ScanOptions, scan_paths};

fn fixture(directory: &str, rule_id: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(directory);
    fs::read_dir(&root)
        .expect("fixture directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| stem.eq_ignore_ascii_case(rule_id))
        })
        .unwrap_or_else(|| panic!("missing {directory} fixture for {rule_id}"))
}

#[test]
fn every_rule_has_positive_and_negative_fixtures() {
    for rule in registry() {
        let rule_id = rule.metadata().id;
        let slop = scan_paths(&[fixture("slop", rule_id)], &ScanOptions::default())
            .expect("scan slop fixture");
        assert!(
            slop.findings
                .iter()
                .any(|finding| finding.rule_id == rule_id),
            "{rule_id} did not trigger on its slop fixture"
        );

        let clean = scan_paths(&[fixture("clean", rule_id)], &ScanOptions::default())
            .expect("scan clean fixture");
        assert!(
            clean
                .findings
                .iter()
                .all(|finding| finding.rule_id != rule_id),
            "{rule_id} triggered on its clean fixture: {:?}",
            clean.findings
        );
    }
}

#[test]
fn clean_corpus_is_finding_free_and_slop_corpus_covers_every_rule() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let clean =
        scan_paths(&[root.join("clean")], &ScanOptions::default()).expect("scan clean corpus");
    assert!(
        clean.findings.is_empty(),
        "clean corpus findings: {:?}",
        clean.findings
    );

    let slop = scan_paths(&[root.join("slop")], &ScanOptions::default()).expect("scan slop corpus");
    let observed: BTreeSet<_> = slop
        .findings
        .iter()
        .map(|finding| finding.rule_id)
        .collect();
    let expected: BTreeSet<_> = registry()
        .into_iter()
        .map(|rule| rule.metadata().id)
        .collect();
    assert_eq!(observed, expected);
}
