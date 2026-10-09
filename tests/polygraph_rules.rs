#![cfg(feature = "polygraph")]

//! Behavior of the polygraph rules that compare files. Needs the model file: set
//! `SLOPCOP_POLYGRAPH_MODEL` or run `make polygraph-model`.

use std::path::PathBuf;

use slopcop::{ScanOptions, SourceFile, scan_sources};

fn options() -> ScanOptions {
    slopcop::polygraph::load_model(None, None).unwrap_or_else(|message| panic!("{message}"));
    let mut options = ScanOptions::default();
    options.config.polygraph_enabled = true;
    options
}

fn files(entries: &[(&str, &str)]) -> Vec<SourceFile> {
    entries
        .iter()
        .map(|(path, text)| SourceFile {
            path: PathBuf::from(path),
            bytes: text.as_bytes().to_vec(),
        })
        .collect()
}

fn poly004(entries: &[(&str, &str)]) -> Vec<(String, usize)> {
    scan_sources(files(entries), &options())
        .findings
        .iter()
        .filter(|finding| finding.rule_id == "POLY004")
        .map(|finding| (finding.path.display().to_string(), finding.location.line))
        .collect()
}

const INSTALL: &str = "Download the latest release archive from the project page, unpack it into a directory on your path, and run the setup command once so that the configuration folder and the default settings file are created for your user account.\n";
const INSTALL_REWORDED: &str = "Download the newest release archive from the project page, unpack it into a directory on your path, and run the setup command one time so that the configuration folder and the default settings file are created for your user account.\n";
const UPGRADE: &str = "Before upgrading across a major version, export your saved profiles with the backup command, because the on-disk format changes and older profiles are migrated only once. Keep the exports for thirty days.\n";

#[test]
fn a_reworded_copy_in_another_file_is_reported_once_at_the_later_file() {
    assert_eq!(
        poly004(&[("docs/a.md", INSTALL), ("docs/b.md", INSTALL_REWORDED)]),
        [("docs/b.md".to_owned(), 1)]
    );
}

#[test]
fn distinct_paragraphs_and_same_file_repeats_are_quiet() {
    assert_eq!(
        poly004(&[("docs/a.md", INSTALL), ("docs/b.md", UPGRADE)]),
        []
    );
    let repeated = format!("{INSTALL}\n{INSTALL_REWORDED}");
    assert_eq!(poly004(&[("docs/a.md", &repeated)]), []);
}

#[test]
fn licenses_lists_and_short_paragraphs_are_skipped() {
    assert_eq!(
        poly004(&[("docs/a.md", INSTALL), ("LICENSE.md", INSTALL)]),
        []
    );
    assert_eq!(
        poly004(&[("docs/a.md", INSTALL), ("third_party/notes.md", INSTALL)]),
        []
    );
    let list = format!("- {}", INSTALL.trim_end());
    assert_eq!(poly004(&[("docs/a.md", &list), ("docs/b.md", &list)]), []);
    assert_eq!(
        poly004(&[
            ("docs/a.md", "Run setup once.\n"),
            ("docs/b.md", "Run setup once.\n")
        ]),
        []
    );
}

#[test]
fn a_directive_suppresses_the_paragraph_below_it() {
    let suppressed =
        format!("<!-- slopcop: ignore POLY004 -- shared security note -->\n{INSTALL_REWORDED}");
    assert_eq!(
        poly004(&[("docs/a.md", INSTALL), ("docs/b.md", &suppressed)]),
        []
    );
}

#[test]
fn results_do_not_depend_on_file_order() {
    let forward = poly004(&[("docs/a.md", INSTALL), ("docs/b.md", INSTALL_REWORDED)]);
    let backward = poly004(&[("docs/b.md", INSTALL_REWORDED), ("docs/a.md", INSTALL)]);
    assert_eq!(forward, backward);
}
