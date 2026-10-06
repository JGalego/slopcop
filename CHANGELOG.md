# Changelog

This project records user-visible changes in this file and follows Semantic Versioning.

## Unreleased

### Added

- `VIBE020` reports clusters of rhetorical contrast templates such as "it's X, not Y", "the goal isn't X; it's Y", and "rather than X, Y".

### Changed

- Phrase-based `vibecheck` rules match typographic apostrophes and no longer count a phrase nested inside a longer listed phrase.
- `VIBE001` recognizes more stock pivot phrases and transitions, including "the real question is", "put differently", "the bottom line is", "more broadly", and "notably,", with either apostrophe. It no longer counts double-quoted examples or a literal "the key is" followed by an ordinary predicate.

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
