# Contributing

Contributions should make findings more accurate, more explainable, or cheaper to compute. A smaller rule set with trustworthy output is better than a large catalog of weak guesses.

## Development setup

The minimum supported Rust version is 1.85. Install Rust with `rustup`, clone the repository, then run:

```sh
make bootstrap
```

This builds the workspace and installs the repository's local pre-commit hook. The bootstrap target uses an existing `pre-commit` executable or runs it through `uvx`.

The `polygraph` and `polygraph-lm` features add rules that need model files. `make check` builds with all features, so run `make polygraph-model` and `make polygraph-lm` once to download them into `~/.cache/slopcop`, then point the tests at them:

```sh
export SLOPCOP_POLYGRAPH_MODEL=~/.cache/slopcop/polygraph-ec9c31b3ba4a.bin
export SLOPCOP_POLYGRAPH_LM=~/.cache/slopcop/smollm2-135m
```

A test that needs a model fails with these instructions instead of skipping the rules.

Run the complete local gate before opening a pull request:

```sh
make check
```

The gate checks formatting, Clippy with warnings denied, all tests, and self-lint. Use `make benchmark` when scanner or rule-engine work could affect throughput. Use `make field` when a rule or analyzer change could move detections on real code; it scans the pinned projects in `benchmarks/field/projects.toml` and fails on any difference from the baseline. If the difference is intended, run `make field-baseline` and review the diff of `benchmarks/field/baseline.json` in the pull request. CI also tests Rust 1.85, the minimum supported compiler.

## Changes

Keep changes focused. New runtime dependencies need a clear startup-time, binary-size, or maintenance justification. Normal scans must not make network requests, download models, or emit telemetry.

Bug fixes need a regression test. False-positive fixes belong in both a focused unit test and the clean fixture corpus when they represent a reusable case.

## Rules

A rule contribution includes:

- a stable ID and module assignment;
- description, severity, confidence, message, suggestion, and rationale;
- at least one positive example and one false-positive note;
- focused unit tests;
- matching files in `tests/fixtures/slop` and `tests/fixtures/clean`;
- an entry in the rule index;
- benchmark evidence when the rule adds a new analysis pass.

Read the [rule development guide](docs/contributing-rules.md) before choosing an ID or threshold.

Papertrail rules use focused commit-message tests and Git integration tests instead of file fixtures.

## Pull requests

Describe the observable problem, the chosen signal, and the cases that should remain quiet. Include measurements for performance work. Avoid claims about who authored an artifact; findings must describe what the scanner observed.

## Issue automation

Two bots help with issues. Neither closes, assigns, or edits issues.

- **Issue triage** (`.github/workflows/triage.yml`) runs when someone outside the maintainers opens an issue. For false-positive, missed-slop, and Most Wanted reports it scans the reported snippet with slopcop built from `main` and says whether the rule fires. With a `GEMINI_API_KEY` secret, Gemini adds a collapsed summary, missing details, possibly related issues, and labels from a fixed list. It posts one comment and updates that comment on reruns. Rerun it from the Actions tab with the issue number. Set the `GEMINI_MODEL` variable to change the model.
- **Claude** (`.github/workflows/claude.yml`) answers only when a maintainer mentions `@claude` in an issue or pull request comment. It needs the [Claude GitHub App](https://github.com/apps/claude) installed on the repository and a `CLAUDE_CODE_OAUTH_TOKEN` secret from `claude setup-token`, or an `ANTHROPIC_API_KEY` secret.
