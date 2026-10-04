# Architecture

`slopcop` is a Rust library with a thin command-line binary. The library boundary keeps rule behavior independently testable and lets future editor or service integrations reuse the scanner without parsing terminal output.

## Data flow

```text
paths or Git selection
        |
        v
ignore-aware discovery / index blob loading
        |
        v
source classification and early skips
        |
        v
offset-preserving code and prose views
        |
        v
parallel rule evaluation
        |
        v
suppressions and severity overrides
        |
        v
stable finding sort
        |
        v
text / JSON / SARIF / GitHub reporter
```

## Components

`discovery` walks explicit files and directories with Git ignore semantics. It deduplicates paths, prunes dependency and build directories, and applies configured globs before file reads.

`language` classifies supported code, documentation, text, and configuration extensions. Unknown UTF-8 files can be discovered but do not receive prose rules.

`analysis` creates byte-for-byte aligned views. A code view masks comments and strings for structural checks. A prose view keeps documentation, comments, and docstrings while masking code fences and literals. Newlines and byte lengths remain stable, so findings map back to source without a side table.

`scanner` rejects oversized, binary, invalid UTF-8, and generated files before analysis. Rayon distributes independent files across workers. Rules run sequentially within one file and share lazily cached lowercase text, sentence spans, paragraph spans, and word counts.

`rules` owns the registry and the `Rule` trait. The CLI does not contain detection logic. Metadata travels with each rule and powers `rules`, `explain`, SARIF descriptors, configuration validation, and fixture coverage checks.

`git` reads staged content directly from the index. Working-tree comparisons read local content, while base comparisons read committed `HEAD` blobs. Both parse zero-context diff hunks and retain findings whose source line intersects an added range. Selection, configuration, and hunk lookup use normalized absolute paths; reports use invocation-relative paths where possible.

`reporting` consumes sorted findings. JSON includes scan counts; SARIF includes every rule descriptor; GitHub output escapes workflow-command control characters.

## Determinism

Parallel workers may finish in any order. The scanner sorts findings by path, line, column, and rule ID before reporting. The registry is sorted by stable ID. Rules make no network requests and use no randomized or model-backed process.

Timing appears only in the benchmark command. Normal JSON and SARIF output contain no timestamps, host data, or unstable identifiers.

## Performance choices

Files are read once. Compiled regular expressions live in `OnceLock` values. Cheap classification and binary checks happen before allocation-heavy analysis. Prose tokenization and segmentation are lazy, so code-only checks do not pay for unused document statistics.

The scanner favors a conservative local heuristic over whole-program analysis. A detector that needs compiler-quality control flow belongs in a compiler plugin or language linter, not this executable.
