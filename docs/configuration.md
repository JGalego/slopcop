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

## Path selection

Discovery honors `.gitignore` and the conventional global Git excludes. It also skips `.git`, dependency and third-party directories, virtual environments, caches, build output, binaries, invalid UTF-8, files above the size limit, and common generated-file headers.

`slopcop.ignore.paths` and `slopcop.files.exclude` are combined. `slopcop.files.include` is an allowlist when it is non-empty. Patterns use Git-style glob syntax and `/` separators. A bare filename matches at any depth; a leading `/` anchors it to the configuration directory. A trailing `/` selects a directory and its descendants. Negated patterns override earlier matches within the same list, subject to Git's parent-directory exclusion rules.

Explicit files still pass through project path filters, binary detection, the size limit, and generated-file detection. Overlapping input paths are deduplicated before scanning.

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
