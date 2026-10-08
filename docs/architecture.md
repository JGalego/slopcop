# Architecture

`slopcop` is a Rust library with a thin command-line binary. The library boundary keeps rule behavior independently testable and lets future editor or service integrations reuse the scanner without parsing terminal output.

## Data flow

```text
paths, commit message, or Git selection
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
text / JSON / SARIF / GitHub / HTML reporter
```

## Components

`discovery` walks explicit files and directories with Git ignore semantics. It deduplicates paths, prunes dependency and build directories, and applies configured globs before file reads.

`language` classifies supported code, documentation, text, and configuration extensions. Unknown UTF-8 files can be discovered but do not receive prose rules.

`analysis` creates byte-for-byte aligned views. A code view masks comments and strings for structural checks. A prose view keeps documentation, comments, and docstrings while masking Markdown front matter, HTML comments, nested code fences, and literals. reStructuredText files and Python docstrings also mask directives, literal and doctest blocks, roles, and field markers, Rust documentation comments mask their fenced examples, and plain-text files mask blocks indented by a tab or four spaces below a lead-in that ends with a colon. Paragraphs end at lines without words, and sentences never cross a paragraph. Newlines and byte lengths remain stable, so findings map back to source without a side table.

`scanner` rejects oversized, binary, invalid UTF-8, generated, and conservatively detected minified JavaScript or TypeScript files before analysis. Rayon distributes independent files across workers. Rules run sequentially within one file and share lazily cached lowercase text, sentence spans, paragraph spans, and word counts.

`rules` owns the registry and the `Rule` trait. The CLI does not contain detection logic. Metadata travels with each rule and powers `rules`, `explain`, SARIF descriptors, configuration validation, and fixture coverage checks.

`papertrail` reads one `commit-msg` file or NUL-delimited commit records from `git log`. Its rules share metadata, severity overrides, reporters, and exit thresholds with artifact rules, but use a commit-specific context so file discovery remains unchanged.

`git` reads staged content directly from the index. Working-tree comparisons read local content, while base comparisons read committed `HEAD` blobs. Both parse zero-context diff hunks and retain findings whose source line intersects an added range. Selection, configuration, and hunk lookup use normalized absolute paths; reports use invocation-relative paths where possible.

`reporting` consumes sorted findings. Each finding carries a range from its reported position to the end of that line's content, since rules report one position and judge the line or construct that starts there. JSON includes scan counts and both ends of each range; SARIF includes every rule descriptor and full regions; GitHub output escapes workflow-command control characters; HTML is a self-contained page that escapes all source text and runs no scripts.

## Determinism

Parallel workers may finish in any order. The scanner sorts findings by path, line, column, and rule ID before reporting. The registry is sorted by stable ID. Rules make no network requests. The default build uses no randomized or model-backed process; the optional `polygraph` module, described below, is the only exception and is not compiled unless a build asks for it.

Timing appears only in the benchmark command. Normal JSON, SARIF, and HTML output contain no timestamps, host data, or unstable identifiers.

## Polygraph

`polygraph` is an opt-in module for checks that need a model. The `polygraph` cargo feature compiles it, `[slopcop.polygraph] enabled = true` or `--polygraph` turns it on, and a scan that asks for it without a model exits with code 2 instead of skipping it. Nothing is downloaded during a scan.

The embedding rules use a static embedding model, potion-base-8M, reduced to 128 dimensions and quantized to int8 with one global scale. The file is pinned by SHA-256. A text's vector is the element-wise sum of its tokens' rows, found by a pure-Rust reimplementation of the BERT tokenizer. Similarity compares two integer vectors by squaring both sides of the cosine inequality in 128-bit integers, so the decision path has no floating-point type and gives the same answer on every CPU and in WebAssembly. Golden tests pin the tokenizer and the dot products to the reference implementation.

`POLY004` compares files, so each file hands its comparable paragraphs to one pass after the scan. Up to a fixed paragraph count every pair is compared; above it, fixed integer hyperplanes pick candidate pairs and the exact cosine decides.

The language-model rules need the `polygraph-lm` feature and a directory with SmolLM2-135M, which candle runs in `f32` on the CPU. Their scores are reproducible on one build and CPU family but not bit for bit across CPUs, so the rules are informational. Inference runs on its own thread pool, one pass at a time: sharing the scanner's pool would deadlock, because its workers wait for the model lock while candle waits for them.

## Performance choices

Files are read once. Compiled regular expressions live in `OnceLock` values. Cheap classification and binary checks happen before allocation-heavy analysis. Prose tokenization and segmentation are lazy, so code-only checks do not pay for unused document statistics.

The scanner favors a conservative local heuristic over whole-program analysis. A detector that needs compiler-quality control flow belongs in a compiler plugin or language linter, not this executable.
