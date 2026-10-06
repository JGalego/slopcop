# Integrations

slopcop runs as a single binary with no network access, so every integration below starts the same executable and reads its exit code or one of its report formats.

## GitHub Actions

The repository is also a GitHub Action. It downloads a release binary, verifies its SHA-256 checksum, and reports findings as workflow annotations on the pull request diff.

```yaml
name: slopcop

on:
  pull_request:
  push:
    branches: [main]

permissions:
  contents: read

jobs:
  slopcop:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: JGalego/slopcop@main
```

Pin the action to a release tag or a commit SHA in real workflows. When the action is pinned to a release tag, it installs the binary from that same release; otherwise it installs the latest release unless `version` says which one.

| Input | Default | Meaning |
| --- | --- | --- |
| `version` | the pinned tag, or the latest release | Release tag to install, such as `v0.2.0`. |
| `binary` | | Path to an existing executable. Nothing is downloaded. |
| `paths` | `.` | Whitespace-separated files or directories, relative to the repository root. |
| `mode` | `auto` | `auto` reports findings on changed lines for pull requests and scans everything for other events. `changed` and `all` force one behavior. |
| `base` | the pull request base commit | Base revision for changed-line mode. |
| `config` | discovered from the repository root | Path to a `.slopcop.toml`. |
| `sarif` | `false` | Also upload a SARIF report to GitHub code scanning. |
| `sarif-file` | `slopcop.sarif` | Where the SARIF report is written. |

The action sets two outputs: `findings`, the number of findings reported, and `exit-code`, which follows the CLI's exit codes. The step fails when a finding reaches the configured `fail-level`.

Changed-line mode is what makes adoption practical in an existing repository: a pull request is judged on the lines it adds, not on everything that came before it. The default `actions/checkout` clone has one commit, so the action fetches more history until the merge base with `base` is reachable. Checking out with `fetch-depth: 0` skips that step.

GitHub shows at most ten warning and ten error annotations per step. For larger reports, or to track findings over time, enable SARIF upload. It needs `security-events: write`, and private repositories also need GitHub Advanced Security:

```yaml
permissions:
  contents: read
  security-events: write

steps:
  - uses: actions/checkout@v5
  - uses: JGalego/slopcop@main
    with:
      sarif: true
```

slopcop's own CI runs the action against a binary built from the same commit, through the `binary` input, and tests the release download on Linux, macOS, and Windows.
