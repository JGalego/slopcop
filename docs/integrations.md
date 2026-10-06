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

## Editors

`slopcop lsp` starts a language server on standard input and output. Any editor with a Language Server Protocol client can use it, and the findings match a command-line scan of the same file: every rule judges one file at a time, and the server uses the same configuration discovery as `--stdin`, starting in the directory of the file being edited.

The server provides:

- Diagnostics when a file is opened or edited. Each one names its rule and links to the rule reference.
- Hover text with the rule's description, rationale, suggestion, and false-positive notes.
- A quick fix that inserts a suppression directive above the finding, in the file's comment syntax. The directive does nothing until you write the reason after `--`.

Saving `.slopcop.toml` in the editor lints every open file again. Configuration errors are shown once as an editor message, and diagnostics stay empty until the file is fixed. Documents without a `file:` URI, such as unsaved buffers, are not linted because they have no directory to discover configuration from.

The language server is a default Cargo feature. Build without it with `cargo install slopcop --no-default-features`.

### Neovim

Neovim 0.11 and later:

```lua
vim.lsp.config("slopcop", {
  cmd = { "slopcop", "lsp" },
  filetypes = { "python", "javascript", "typescript", "go", "rust", "markdown" },
  root_markers = { ".slopcop.toml", ".git" },
})
vim.lsp.enable("slopcop")
```

### Helix

In `languages.toml`, define the server and add it to each language. Listing `language-servers` replaces the defaults, so keep the servers you already use:

```toml
[language-server.slopcop]
command = "slopcop"
args = ["lsp"]

[[language]]
name = "python"
language-servers = ["pylsp", "slopcop"]
```

### Emacs

With Eglot:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '((python-mode python-ts-mode markdown-mode) . ("slopcop" "lsp"))))
```

Eglot runs one server per major mode, so this replaces another Python server in those modes.

### JetBrains IDEs

Install the LSP4IJ plugin, add a new language server with the command `slopcop lsp`, and map it to the file types you want linted.
