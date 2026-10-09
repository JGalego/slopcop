# Configuration

Configuration discovery starts in the current working directory and checks each parent in order until it reaches the filesystem root. With `--stdin`, discovery starts in the directory of `--stdin-filename` instead, so an editor gets the configuration that governs the file whatever its own working directory is. Pass `--config FILE` to use a specific file. Relative path patterns are evaluated from the configuration file's directory.

## Complete example

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

[slopcop.polygraph]
enabled = false
model = "models/polygraph-ec9c31b3ba4a.bin"
language-model = "models/smollm2-135m"

[slopcop.rules]
VIBE003 = "info"
DEAD004 = "error"
VIBE018 = "off"

[slopcop.ignore]
paths = [
  "vendor/**",
  "generated/**",
]

[slopcop.files]
include = ["src/**", "docs/**", "README.md"]
exclude = ["docs/archive/**"]
```

## Project settings

`fail-level` controls exit code `1`. The accepted values are `info`, `warning`, and `error`. Findings below the threshold are still printed. The default is `warning`.

`max-file-size` is the largest file read by the scanner, in bytes. The default is 1,000,000. Larger files are counted as skipped.

## Modules and rules

All modules are enabled by default. `deadweight` and `vibecheck` inspect repository artifacts; `papertrail` inspects commit messages and history through its explicit commands. A module switch prevents all of its rules from running, which avoids their analysis cost as well as their findings. A project may override an individual rule with `info`, `warning`, `error`, or `off`.

Rule IDs are validated while loading configuration. A typo is a usage error rather than a silently ignored setting. Configuration that leaves no enabled rules is also rejected.

## Polygraph

`polygraph` is off by default and needs a build with the `polygraph` feature, so `cargo install slopcop --features polygraph`. Turn it on with `enabled = true` or `--polygraph`; `--no-polygraph` overrides the file. A scan that asks for polygraph and cannot find a valid model exits with code 2.

The embedding model is looked up in this order, and the first source that names a path is the only one used: `--polygraph-model FILE`, the `SLOPCOP_POLYGRAPH_MODEL` environment variable, `model` in this section (relative to the configuration file), and `slopcop/polygraph-<hash>.bin` in the cache directory. `make polygraph-model` downloads it there and checks its SHA-256.

`POLY001` and `POLY002` also need the `polygraph-lm` feature, which needs Rust 1.87 or newer because candle does, and a directory with `model.safetensors` and `tokenizer.json` from HuggingFaceTB/SmolLM2-135M at revision 93efa2f097d58c2a74874c7e644dbc9b0cee75a2, named by `--polygraph-lm DIR`, `SLOPCOP_POLYGRAPH_LM`, or `language-model` in this section. They run only when one is named, because scoring takes roughly a second per 90 tokens on a laptop CPU. `make polygraph-lm` downloads the files. At most 1,024 tokens of each document are scored.

Each file is checked against a pinned SHA-256, and a mismatch is an error. Rule IDs from this module are valid in `[slopcop.rules]` in every build, so one configuration file works with and without the feature.

The website exposes the embedding and language-model tiers as separate opt-ins. The latter warns before downloading about 259 MB from the pinned Hugging Face revision; it runs in the browser's worker, can take several minutes, and may produce slightly different informational findings on different devices.

## Path selection

Discovery honors `.gitignore` and the conventional global Git excludes. It also skips `.git`, dependency and third-party directories, virtual environments, caches, build output, binaries, invalid UTF-8, files above the size limit, and common generated-file headers.

Files that `.gitattributes` marks `linguist-vendored` or `linguist-generated` are skipped too, so a repository that already tells GitHub which code is imported or generated does not need to repeat it in `slopcop.toml`. Attributes follow Git's rules: later lines and deeper `.gitattributes` files override earlier ones, and `-linguist-vendored` or `linguist-generated=false` restores a path. When the scan root sits inside a Git work tree, the `.gitattributes` files between the root and the top of the tree apply as well.

`slopcop.ignore.paths` and `slopcop.files.exclude` are combined. `slopcop.files.include` is an allowlist when it is non-empty. Patterns use Git-style glob syntax and `/` separators. A bare filename matches at any depth; a leading `/` anchors it to the configuration directory. A trailing `/` selects a directory and its descendants. Negated patterns override earlier matches within the same list, subject to Git's parent-directory exclusion rules.

Explicit files still pass through project path filters, binary detection, the size limit, and generated-file detection, but not the directory skips or `.gitattributes`. Overlapping input paths are deduplicated before scanning.

## Suppressions

An inline suppression names one or more rules and includes a reason after `--`:

```python
# slopcop: ignore DEAD004 -- optional compatibility probe
return None
```

Multiple IDs use commas:

```typescript
// slopcop: ignore DEAD002, DEAD005 -- framework extension point
```

The directive applies to its own line and the next line. `ignore all`, missing reasons, malformed IDs, and file-wide wildcard directives have no effect. Use path configuration for reviewed repository-wide exclusions.

## Failure behavior

Exit codes are stable:

| Code | Meaning |
| ---: | --- |
| 0 | No finding reached `fail-level` |
| 1 | At least one finding reached `fail-level` |
| 2 | Invalid arguments, Git state, or configuration |
| 3 | Scanner or output failure |

Machine-readable formats write findings to standard output and diagnostics to standard error. Color is enabled only for terminal text output and is disabled when `NO_COLOR` is set.
