# slopcop for Visual Studio Code

This extension shows [slopcop](https://github.com/JGalego/slopcop) findings while you edit. It starts `slopcop lsp` and leaves the linting to it, so the editor reports what a command-line scan of the same file reports, with the same `.slopcop.toml`.

- Findings appear as diagnostics in the editor and the Problems panel.
- Hovering a finding shows the rule's rationale, suggestion, and false-positive notes.
- The quick fix inserts a suppression directive above the finding. Write the reason after `--`; a directive without one does nothing.

Saving or changing a `.slopcop.toml` lints the open files again.

## Requirements

Platform-specific builds of the extension include the slopcop binary. Otherwise, install slopcop and make sure it is on `PATH`:

```sh
curl -fsSL https://raw.githubusercontent.com/JGalego/slopcop/main/install/install.sh | sh
```

On Windows, use `install.ps1` from the same directory, or run `cargo install slopcop`.

## Settings

| Setting | Meaning |
| --- | --- |
| `slopcop.path` | Path to the `slopcop` executable. When empty, the extension uses its bundled binary, then `slopcop` on `PATH`. It can only be set in user settings, so a workspace cannot choose which program runs. |
| `slopcop.trace.server` | Log language server messages in the slopcop output channel. |

Run **slopcop: Restart Language Server** from the Command Palette after installing a new slopcop binary.
