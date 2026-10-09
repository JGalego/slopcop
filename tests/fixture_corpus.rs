use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use slopcop::rules::registry;
use slopcop::{ScanOptions, SourceFile, scan_paths, scan_sources};

/// Default options, with the polygraph rules on when the build has them. They need the model, so a
/// missing one fails the test with instructions instead of skipping the rules.
fn options() -> ScanOptions {
    #[allow(unused_mut)]
    let mut options = ScanOptions::default();
    #[cfg(feature = "polygraph")]
    {
        slopcop::polygraph::load_model(None, None).unwrap_or_else(|message| panic!("{message}"));
        options.config.polygraph_enabled = true;
    }
    options
}

fn fixture_rules() -> Vec<&'static slopcop::RuleMetadata> {
    registry()
        .into_iter()
        .map(|rule| rule.metadata())
        // The language-model rules are covered by `tests/polygraph_lm.rs`; running them through
        // the whole fixture corpus would make this coverage guard CPU-model-sized.
        .filter(|metadata| !matches!(metadata.id, "POLY001" | "POLY002"))
        .collect()
}

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
    for rule in fixture_rules() {
        let rule_id = rule.id;
        let slop = scan_paths(&[fixture("slop", rule_id)], &options()).expect("scan slop fixture");
        assert!(
            slop.findings
                .iter()
                .any(|finding| finding.rule_id == rule_id),
            "{rule_id} did not trigger on its slop fixture"
        );

        let clean =
            scan_paths(&[fixture("clean", rule_id)], &options()).expect("scan clean fixture");
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
fn fixture_findings_are_independent_of_line_endings() {
    for directory in ["clean", "slop"] {
        for rule in fixture_rules() {
            let path = fixture(directory, rule.id);
            let source = fs::read_to_string(&path)
                .expect("read fixture")
                .replace("\r\n", "\n");
            let scan = |bytes| {
                scan_sources(
                    vec![SourceFile {
                        path: path.clone(),
                        bytes,
                    }],
                    &options(),
                )
            };
            let lf = scan(source.as_bytes().to_vec());
            let crlf = scan(source.replace('\n', "\r\n").into_bytes());

            assert_eq!(lf.scanned_files, 1, "{}", path.display());
            assert_eq!(crlf.scanned_files, 1, "{}", path.display());
            assert_eq!(lf.findings, crlf.findings, "{}", path.display());
        }
    }
}

#[test]
fn clean_corpus_is_finding_free_and_slop_corpus_covers_every_rule() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let clean = scan_paths(&[root.join("clean")], &options()).expect("scan clean corpus");
    assert!(
        clean.findings.is_empty(),
        "clean corpus findings: {:?}",
        clean.findings
    );

    let slop = scan_paths(&[root.join("slop")], &options()).expect("scan slop corpus");
    let observed: BTreeSet<_> = slop
        .findings
        .iter()
        .map(|finding| finding.rule_id)
        .collect();
    let expected: BTreeSet<_> = registry()
        .into_iter()
        .map(|rule| rule.metadata())
        .filter(|metadata| !matches!(metadata.id, "POLY001" | "POLY002"))
        .map(|metadata| metadata.id)
        .collect();
    // `POLY004` compares files, so it is not in the per-file registry; the slop corpus holds a pair.
    let project_rules: BTreeSet<_> = observed.difference(&expected).copied().collect();
    assert!(
        project_rules.is_subset(&BTreeSet::from(["POLY004"])),
        "unexpected rules {project_rules:?}"
    );
    assert!(
        expected.is_subset(&observed),
        "{:?} did not fire",
        expected.difference(&observed)
    );
}
