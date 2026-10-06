<p align="center"><img src="images/icon.png" width="128" height="128" alt="slopcop logo"></p>

# slopcop for Visual Studio Code

**Slop stops here.** We don't care whether AI wrote it. We care whether it's slop.

[slopcop](https://github.com/JGalego/slopcop) finds observable quality problems in source code, comments, and documentation. It is deterministic, runs without a network or model, and reports the evidence behind each finding. A human can write slop. AI can write excellent code. Authorship is not the question.

The editor runs two families of rules:

- **deadweight**: code and prose that exist without doing useful work, such as swallowed exceptions, placeholder markers, empty functions, trivial assertions, and comments that restate the code.
- **vibecheck**: patterns in prose and comments, such as stock transitions, assistant framing, hedging, forced symmetry, and uniform rhythm. One "robust" is fine. Density and repetition are not.

The third family, **papertrail**, checks commit history, so it runs only from the command line. The [rule index](https://github.com/JGalego/slopcop/blob/main/docs/rules/README.md) lists every rule, and the [browser demo](https://slopcop.me) scans any public GitHub repository.

## Features

This extension shows slopcop findings while you edit. It starts `slopcop lsp` and leaves the linting to it, so the editor reports what a command-line scan of the same file reports, with the same `.slopcop.toml`.

- Findings appear as diagnostics in the editor and the Problems panel.
- Hovering a finding shows the rule's rationale, suggestion, and false-positive notes.
- The quick fix inserts a suppression directive above the finding. Write the reason after `--`; a directive without one does nothing.

Saving or changing a `.slopcop.toml` lints the open files again.

## Findings view

The language server lints only open files. To see every finding in the workspace, open the slopcop icon in the Activity Bar. The Findings view runs `slopcop` on each workspace folder, shows the total as a badge, and scans again after a save, a file operation, or a `.slopcop.toml` change. It reads files from disk, so unsaved edits appear after a save. Group findings by file or by rule from the view's toolbar, and select a finding to open it.

## Feedback

Each finding in the view has buttons to report a false positive or nominate it for the Most Wanted gallery. In the editor, right-click a finding for the same commands, or right-click a selection to report missed slop. The view's toolbar reports missed slop, and its overflow menu reports a bug or requests a feature.

These commands open a prefilled GitHub issue form in the browser with the rule, the slopcop version, the excerpt around the finding, and a link to the line on GitHub when the file is in a GitHub clone. Nothing is sent from the editor: review the form, including the excerpt, before you submit it.

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
