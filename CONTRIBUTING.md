# Contributing

Contributions should make findings more accurate, more explainable, or cheaper to compute. A smaller rule set with trustworthy output is better than a large catalog of weak guesses.

## Development setup

The minimum supported Rust version is 1.85. Install Rust with `rustup`, clone the repository, then run:

```sh
make bootstrap
```

This builds the workspace and installs the repository's local pre-commit hook. The bootstrap target uses an existing `pre-commit` executable or runs it through `uvx`.

Run the complete local gate before opening a pull request:

```sh
make check
```

The gate checks formatting, Clippy with warnings denied, all tests, and self-lint. Use `make benchmark` when scanner or rule-engine work could affect throughput. CI also tests Rust 1.85, the minimum supported compiler.

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

## Pull requests

Describe the observable problem, the chosen signal, and the cases that should remain quiet. Include measurements for performance work. Avoid claims about who authored an artifact; findings must describe what the scanner observed.
