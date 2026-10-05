# Changelog

This project records user-visible changes in this file and follows Semantic Versioning.

## Unreleased

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
