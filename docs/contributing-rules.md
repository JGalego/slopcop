# Rule development

A rule earns an ID when it detects an observable quality problem with a useful explanation and a defensible quiet case. Similarity to model output is motivation, not evidence.

## Choose the module

Use `DEAD` for artifacts that add little behavior or information: placeholders, no-op wrappers, empty handling, duplicated logic, or vacuous tests and docs.

Use `VIBE` for measurable prose structure: phrase clusters, density, repetition, punctuation, sentence rhythm, paragraph shape, or local restatement.

Use `TRAIL` for deterministic commit-message or history hygiene. Message-file checks must remain compatible with `commit-msg` hooks; history-only checks may inspect commit ranges but must not block creation of temporary autosquash commits.

Use `POLY` only for a check that needs a model file, and read the polygraph section of the architecture notes first. A `POLY` rule on the static embedding model must decide with integer arithmetic only. A rule on the language model is informational, reports the sentences behind its finding instead of a score for the document, and runs only when a language model is configured. Calibrate a threshold by reading a random sample of findings from the field-test projects and record the sample's precision with the rule.

Do not add syntax, formatting, type, or unused-import checks already owned by compilers and mainstream linters.

## Choose a signal

Write the positive and clean examples first. State what is counted and why the threshold separates them. Prefer combinations of these signals:

- minimum occurrence count;
- ratio to words, sentences, or paragraphs;
- multiple distinct phrase forms;
- proximity within one section or document;
- repeated prefixes or templates;
- minimum substantial block size;
- source-type or test-path context.

An isolated ordinary word should rarely trigger. Thresholds should be constants visible beside the implementation or clearly named in a shared helper.

## Implement the rule

Add immutable `RuleMetadata` in the owning module. Reuse `ScanContext` views and cached spans. Emit the narrowest useful source location and include an observation such as a count or ratio.

A rule that counts specific words or phrases lists a replacement for each one in `RuleMetadata::replacements`, and its observation names the replacements for the expressions that matched. Keep the table and the phrase list in one constant, as the vocabulary rules do with `expressions(TABLE)`. A replacement may be an instruction, such as "cut it" or "give the number", when no single word fits. Record the source of a new phrase list in [reference sources](rules/references.md).

Compile regular expressions once. Avoid parsing the same document twice, cloning source text, filesystem access, network access, nondeterministic iteration in output, and locks in the per-file hot path.

Severity guidance:

| Severity | Use |
| --- | --- |
| `info` | Weak but reviewable structural smell |
| `warning` | Suspicious pattern with a useful action |
| `error` | Clearly useless or failure-hiding construct |

Confidence describes the rule's observation, not authorship. Use `low` for weak document-level shape, `medium` for contextual density, and `high` for direct structural evidence.

## Prove behavior

Add a focused unit test with at least one positive and negative case. For `DEAD` and `VIBE` rules, add one file named after the lowercased rule ID to both fixture directories:

```text
tests/fixtures/slop/vibe019.md
tests/fixtures/clean/vibe019.md
```

The registry-driven corpus test fails when either file is missing, the positive file does not emit its rule, any rule fires on its own clean control, the aggregate clean corpus has a cross-rule finding, or the aggregate slop corpus misses a registered ID.

Add a regression fixture for a real false positive. Do not solve repository self-lint by disabling the rule or excluding source; anti-bypass tests reject that shortcut.

For `TRAIL` rules, add commit-context unit tests and a Git integration test when the rule depends on history. History-only rules must stay quiet in the `commit-message` command.

## Document and measure

Add the rule to [the rule index](rules/README.md) with its default severity, confidence, trigger, and primary false-positive boundary. Confirm `slopcop explain RULE_ID` is complete.

Run:

```sh
cargo test --all-targets --all-features
cargo run --release -- benchmark
cargo run --release -- .
```

Report benchmark changes when a detector adds a pass, a regular expression, or a new allocation per file.