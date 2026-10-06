<div align="center">
  <img src="docs/assets/slopcop.svg" width="128" height="128" alt="slopcop logo">
  <h1>slopcop</h1>
  <p><strong>Slop stops here.</strong></p>
  <p>
    <a href="https://github.com/JGalego/slopcop/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/JGalego/slopcop/ci.yml?branch=main&amp;style=flat-square&amp;label=build"></a>
    <a href="https://crates.io/crates/slopcop"><img alt="crates.io" src="https://img.shields.io/crates/v/slopcop?style=flat-square"></a>
    <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/badge/license-MIT-171717?style=flat-square"></a>
    <img alt="AI probability: nope" src="https://img.shields.io/badge/AI%20probability-nope-c7ff3d?style=flat-square&amp;labelColor=171717">
    <img alt="bullshit budget: zero" src="https://img.shields.io/badge/bullshit%20budget-zero-c7ff3d?style=flat-square&amp;labelColor=171717">
  </p>
</div>

We don't care whether AI wrote it. We care whether it's slop.

`slopcop` finds observable quality problems in source code, comments, documentation, and repository configuration. It is deterministic, runs without a network or model, and reports the evidence behind each finding. A human can write slop. AI can write excellent code. Authorship is not the question.

```text
$ slopcop .

README.md:42:1  VIBE001  warning
Formulaic transitions are unusually dense.

src/api.py:118:1  DEAD001  error
Exception is swallowed without logging, rethrowing, or handling.

2 finding(s). Nice try, robot.
```

To try it without installing anything, open the [browser demo](https://jgalego.github.io/slopcop/) and enter a public GitHub repository. The scanner runs locally as WebAssembly.

## Getting started

Install from crates.io:

```sh
cargo install slopcop
```

Or install the current source revision:

```sh
cargo install --git https://github.com/JGalego/slopcop
```

Run a repository scan:

```sh
slopcop .
```

Initialize a conservative config and pre-commit entry:

```sh
slopcop init
```

Useful commands:

```sh
slopcop src/ README.md
slopcop --staged
slopcop --diff
slopcop --changed --base origin/main
slopcop commit-message .git/COMMIT_EDITMSG
slopcop history --base origin/main
slopcop explain VIBE001
slopcop rules
slopcop benchmark
```

Exit code `0` means clean or below the configured failure threshold. Findings at or above that threshold return `1`; usage and configuration errors return `2`; internal failures return `3`.

## What it checks

**deadweight** finds code and prose that appear to exist without doing useful work: swallowed exceptions, escaped placeholders, empty concrete functions, trivial assertions, redundant comments, pointless Boolean branches, duplicated blocks, and empty documentation sections.

**papertrail** checks commit messages and branch history for placeholder subjects and autosquash commits that should not reach an integration branch.

**vibecheck** measures patterns in prose and code comments: clustered stock transitions, assistant framing, generic modifier density, repeated sentence openings, forced symmetry, restatement, disclaimer clusters, and unusually uniform rhythm. One use of “robust” or “ultimately” is not a finding. Density and repetition are.

The rule index documents every stable rule. `slopcop explain RULE_ID` prints rationale, examples, false-positive notes, and configuration guidance.

## Configuration

Projects use `.slopcop.toml`:

```toml
[slopcop]
fail-level = "warning"
max-file-size = 1000000

[slopcop.deadweight]
enabled = true

[slopcop.papertrail]
enabled = true

[slopcop.vibecheck]
enabled = true

[slopcop.rules]
VIBE003 = "info"
DEAD004 = "error"

[slopcop.ignore]
paths = ["vendor/**", "generated/**"]
```

Rule values are `info`, `warning`, `error`, or `off`. Configuration discovery walks from the current directory toward the filesystem root. Unknown rule IDs and configurations that disable every rule are rejected. The complete schema is documented in [configuration](docs/configuration.md).

## Suppressions

Suppress a named rule on the same or following line and include a reason:

```python
# slopcop: ignore DEAD004 -- optional compatibility probe
return None
```

Wildcard suppressions and reason-free directives do nothing. Broad exclusions belong in project configuration where reviewers can see them.

## Git and pre-commit

`--staged` reads blobs from the Git index, not the working tree. `--diff` scans working-tree additions and modified hunk lines. `--changed --base REV` compares committed `HEAD` content with its merge base, independently of local edits. Git path arguments are relative to the current directory.

```yaml
repos:
  - repo: https://github.com/JGalego/slopcop
    rev: v0.1.0
    hooks:
      - id: slopcop
      - id: slopcop-commit-msg
```

  The normal hook scans filenames supplied by pre-commit, which hides unstaged edits during commit checks. The `commit-msg` hook checks the proposed subject but deliberately allows `fixup!` and `squash!` while a series is being prepared. `slopcop history --base REV` checks committed branch history and reports those autosquash markers. Install both stages with `pre-commit install --hook-type pre-commit --hook-type commit-msg`. `pre-commit run --all-files` checks all tracked files. This repository uses local entries; `make bootstrap` builds the binary and installs both hook types with `pre-commit` or `uvx`.

## CI and machine output

JSON, SARIF 2.1.0, and GitHub workflow annotations are built in:

```sh
slopcop . --format json
slopcop . --format sarif > slopcop.sarif
slopcop . --format github
```

The included CI workflow runs formatting, Clippy, tests, package verification, release-mode self-lint, and the benchmark smoke check. Self-hosting tests require every module and rule to remain enabled and verify that all source and documentation artifacts are discovered and scanned without size or generated-file skips.

## Performance

Normal scans make no network requests, download no models, and send no telemetry. Discovery respects `.gitignore`, skips common dependency and build directories, rejects binary, oversized, generated, and minified bundle files early, and scans files in parallel. Rules reuse cached prose, sentence, and paragraph analysis.

Repeated release runs on the development x86_64 Linux host scanned the 4,096-file mixed benchmark in **63.7–78.8 ms** at **51,961–64,286 files/sec**. Treat machine-specific numbers as samples, not promises. Reproduce all five profiles with `slopcop benchmark`; methodology and results live in [benchmarks](benchmarks/README.md).

## Philosophy

`slopcop` does not estimate an “AI probability.” It reports concrete artifacts and calibrated rule confidence. The defaults favor high signal over high recall; suspicious style is normally a warning, while clearly useless constructs can be errors.

It complements compilers, formatters, and conventional linters. Syntax errors belong to those tools. `slopcop` asks why a wrapper, comment, fallback, test, or paragraph exists at all.

## Contributing

Run the full local gate with:

```sh
make check
```

Each rule needs stable metadata, implementation tests, a positive fixture, a clean control, documentation, and a false-positive note. Start with [CONTRIBUTING.md](CONTRIBUTING.md) and the [rule development guide](docs/contributing-rules.md).

## License

MIT. See [LICENSE](LICENSE).