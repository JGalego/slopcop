# Changelog

This project records user-visible changes in this file and follows Semantic Versioning.

## Unreleased

### Added

- The repository is a GitHub Action (`uses: JGalego/slopcop@<tag>`). It installs a checksum-verified release binary, reports findings on pull request changed lines as workflow annotations, scans everything on other events, and can upload SARIF to code scanning. slopcop's own CI lints itself through the action.
- `--stdin --stdin-filename PATH` lints one file read from standard input, such as an unsaved editor buffer. The filename selects the language and path filters, and configuration discovery starts in its directory.
- A missed-slop issue form reports quality problems that slopcop does not flag, and a Most Wanted form nominates striking findings for a gallery. The triage bot scans the snippet in both and says whether slopcop already catches it or whether the nominated rule fires.
- The browser demo links each finding to a prefilled Most Wanted nomination (`w`), and its results summary, empty state, and footer link to the missed-slop form.

## 0.2.0 - 2026-10-06

### Added

- Install scripts for macOS and Linux (`install/install.sh`) and Windows (`install/install.ps1`) download a release binary, verify its SHA-256 checksum, and install it without root or administrator rights. The browser demo's landing page and the README list them next to `cargo install`.
- `--format html` writes a self-contained HTML report with scan totals, findings grouped by file, a severity filter, and a reference entry for each rule that fired. The browser demo offers the same report as a download, with links to the scanned commit on GitHub.
- The browser demo links each finding to a prefilled false-positive issue, and its footer links to the bug, false-positive, feature, and security report forms. Unexpected scan errors offer a prefilled bug report.
- The browser demo's landing page shows a sample finding, how a scan works, the rule modules with live rule counts, and the CLI install commands. Navigation, actions, and feedback links have icons.
- The browser demo accepts an optional GitHub token, which raises the API limit from 60 to 5,000 requests per hour. It opens the token field when the anonymous limit runs out and reports a rejected token instead of a generic error.
- `VIBE020` reports clusters of rhetorical contrast templates such as "it's X, not Y", "the goal isn't X; it's Y", and "rather than X, Y".
- `VIBE021` reports repeated correlative emphasis such as "not only X, but also Y" and "everything from X to Y".
- `VIBE022` reports dense workplace productivity jargon such as "leverage", "streamline", "key stakeholders", and "drive alignment".
- `VIBE023` reports dense generic AI vocabulary such as "nuanced", "holistic", "meaningful", and "delve".
- `VIBE024` reports generalized moral framing such as "this is a reminder that", "a testament to", and "is only as good as the people using it".
- `VIBE025` reports dramatic characterizations such as "a pivotal moment", "a fundamental shift", and "a meaningful step forward".
- `VIBE026` reports restated question premises such as "you're essentially asking whether" and "the issue you're getting at".

### Changed

- Phrase-based `vibecheck` rules match typographic apostrophes and no longer count a phrase nested inside a longer listed phrase.
- `VIBE001` recognizes more stock pivot phrases and transitions, including "the real question is", "put differently", "the bottom line is", "more broadly", and "notably,", with either apostrophe. It no longer counts double-quoted examples or a literal "the key is" followed by an ordinary predicate.
- `VIBE002` recognizes more assistant-response phrases, such as "here's a breakdown", "you're absolutely right", and "let me know if you'd like", and counts standalone interjections such as "Absolutely!".
- `VIBE003` counts the importance labels "essential", "critical", "pivotal", "paramount", and "noteworthy".
- `VIBE004` recognizes hedging phrases such as "it can be argued", "one could argue", "to some extent", and "does not necessarily".
- `VIBE005` reports stock both-sides lines such as "there are valid arguments on both sides" and "neither approach is inherently better". Not-only/but-also pairs move to `VIBE021`.
- `VIBE006` recognizes structure announcements such as "first and foremost", "there are three key points", and "before we dive in".
- `VIBE008` reports staged reveals such as "The reason is simple: X.", "The result: X.", and short self-answered questions such as "What changed? X.".
- `VIBE011` reports repeated inline triads of abstract qualities such as "clear, concise, and compelling" and "speed, reliability, and flexibility".
- `VIBE012` counts more vague demonstratives, including "this dynamic", "this shift", "this reality", and "that perspective".
- `VIBE013` recognizes stock idioms such as "double-edged sword", "tip of the iceberg", "moving target", and "north star".
- `VIBE014` focuses on promotional characterizations such as "exciting opportunity", "strong foundation", "valuable insights", and "well-positioned to", and fires at four markers in three forms. Workplace productivity verbs move to `VIBE022`.
- `VIBE015` recognizes disclaimers such as "there is no one-size-fits-all answer", "it depends on the specific situation", and "consider your individual circumstances".
- `VIBE016` recognizes closers such as "the key takeaway", "the broader lesson", "this serves as a reminder", and "in the end".

## 0.1.1 - 2026-10-06

### Added

- Browser demo that scans public GitHub repositories with the WebAssembly build of the scanner, deployed to GitHub Pages.
- `Config::from_toml` for loading configuration text without reading the filesystem.

### Fixed

- reStructuredText prose no longer includes directives, literal and doctest blocks, roles, or field markers. Python docstrings get the same masking, and Rust documentation comments no longer include their fenced examples.
- Sentences and paragraphs no longer run across headings, code, or separate comments. Rhythm, symmetry, colon, and restatement rules no longer compare list entries, release notes, comments on different declarations, or walkthrough steps separated by examples.
- `DEAD005` no longer reads `interface{}` or a `= {}` default in a signature as an empty body.
- `DEAD008` no longer treats different string arguments as forwarded parameters.
- `DEAD007` no longer treats comments that differ only in symbols as duplicates.
- `DEAD006` no longer judges single lines of a longer comment block.
- Common idioms no longer trigger findings:
  - optional imports, exhausted iterators, and tests that raise or expect an exception on purpose (`DEAD001`, `DEAD004`)
  - predicates that answer `false` on an exception (`DEAD004`)
  - Python interface methods and guarded unsupported cases (`DEAD003`)
  - decorated framework handlers (`DEAD005`, `DEAD008`)
  - member facades (`DEAD008`)
  - stacked headings and indented Markdown code (`DEAD012`)
  - test code (`DEAD002`, `DEAD005`, `DEAD011`)

## 0.1.0 - 2026-10-05

### Fixed

- Reduced false positives from documented no-op handlers, constant fallbacks, framework hooks, non-executable `NotImplementedError` references, inline annotations, repeated duplicate-block reports, Markdown metadata, nested fences, decorative headings, and minified JavaScript bundles.

### Added

- `papertrail` commit-message and history checks with `commit-message` and `history` commands plus a `commit-msg` pre-commit hook.
- Initial `deadweight` and `vibecheck` modules with their stable rule catalogs.
- Ignore-aware parallel discovery for code, documentation, text, and configuration files.
- Exact staged-blob scanning and changed-hunk filtering for Git workflows.
- Text, JSON, SARIF 2.1.0, and GitHub annotation reporters.
- TOML configuration, severity overrides, failure thresholds, and reason-required suppressions.
- Self-hosting pre-commit and CI checks with positive and clean fixture corpora.
- Deterministic synthetic benchmark profiles and cross-platform release automation.
