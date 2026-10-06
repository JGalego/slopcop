import { execFile } from "node:child_process";
import * as path from "node:path";
import * as vscode from "vscode";
import { type Finding, type FindingsView, type Severity, findingOf, relativePath } from "./findings";

const REPO_URL = "https://github.com/JGalego/slopcop";
const MAX_FIELD = 1500;
const CONTEXT_LINES = 2;
const SEVERITIES: Record<number, Severity> = {
  [vscode.DiagnosticSeverity.Error]: "error",
  [vscode.DiagnosticSeverity.Warning]: "warning",
  [vscode.DiagnosticSeverity.Information]: "info",
};

/**
 * Registers commands that open prefilled GitHub issue forms. Nothing is sent from the editor: the
 * browser shows the form, and the user reviews and submits it.
 */
export function registerReports(view: FindingsView, executable: () => string): vscode.Disposable[] {
  const version = async () => `slopcop ${view.version ?? (await installedVersion(executable()))} (VS Code extension)`;

  const reportFinding = (template: "false_positive.yml" | "most_wanted.yml") => async (node?: unknown) => {
    const finding = findingOf(node) ?? (await findingAtCursor(view));
    if (!finding) {
      void vscode.window.showInformationMessage("Place the cursor on a slopcop finding, or pick one in the Findings view.");
      return;
    }
    const document = await vscode.workspace.openTextDocument(finding.uri);
    const file = relativePath(finding.uri);
    const link = await permalink(finding.uri, finding.range.start.line + 1);
    const { start, end } = finding.range;
    const snippet = document.getText(new vscode.Range(Math.max(0, start.line - CONTEXT_LINES), 0, end.line + CONTEXT_LINES + 1, 0)).replace(/^(\s*\n)+/, "").trimEnd();
    const output = [`${file}:${start.line + 1}:${start.character + 1}  ${finding.rule}  ${finding.severity}`, finding.message];
    if (finding.observation) output.push(`observed: ${finding.observation}`);
    const fields: Record<string, string | undefined> = {
      rule: finding.rule,
      version: await version(),
      snippet,
      finding: output.join("\n"),
    };
    if (template === "false_positive.yml") {
      fields.title = `false positive: ${finding.rule} in ${file}`;
      fields.context = link ? `${file} at ${link}` : file;
    } else {
      fields.title = `most wanted: ${finding.rule}`;
      fields.context = file;
      fields.source = link;
    }
    await openIssue(template, fields);
  };

  return [
    vscode.commands.registerCommand("slopcop.reportFalsePositive", reportFinding("false_positive.yml")),
    vscode.commands.registerCommand("slopcop.nominate", reportFinding("most_wanted.yml")),
    // The editor context menu passes the document's URI; the view title passes nothing.
    vscode.commands.registerCommand("slopcop.reportMissedSlop", async (resource?: unknown) => {
      const editor = vscode.window.activeTextEditor;
      const fields: Record<string, string | undefined> = { version: await version() };
      if (resource instanceof vscode.Uri && editor?.document.uri.toString() === resource.toString()) {
        const selection = editor.selection;
        const range = selection.isEmpty ? editor.document.lineAt(selection.active.line).range : selection;
        fields.snippet = editor.document.getText(range);
        fields.context = `${relativePath(editor.document.uri)} (${editor.document.languageId})`;
      }
      await openIssue("false_negative.yml", fields);
    }),
    vscode.commands.registerCommand("slopcop.reportBug", async () =>
      openIssue("bug_report.yml", { version: await version(), environment: `VS Code ${vscode.version} on ${process.platform}` })),
    vscode.commands.registerCommand("slopcop.requestFeature", () => openIssue("feature_request.yml", {})),
    ...trackFindingAtCursor(view),
  ];
}

/** Keeps the `slopcop.onFinding` context key current, which shows the editor menu items. */
function trackFindingAtCursor(view: FindingsView): vscode.Disposable[] {
  const update = () => {
    const editor = vscode.window.activeTextEditor;
    const found = editor ? findingsAt(view, editor.document.uri, editor.selection.active).length > 0 : false;
    void vscode.commands.executeCommand("setContext", "slopcop.onFinding", found);
  };
  update();
  return [
    vscode.window.onDidChangeActiveTextEditor(update),
    vscode.window.onDidChangeTextEditorSelection(update),
    vscode.languages.onDidChangeDiagnostics(update),
    view.onDidChangeTreeData(update),
  ];
}

async function findingAtCursor(view: FindingsView): Promise<Finding | undefined> {
  const editor = vscode.window.activeTextEditor;
  if (!editor) return undefined;
  const findings = findingsAt(view, editor.document.uri, editor.selection.active);
  if (findings.length <= 1) return findings[0];
  const picked = await vscode.window.showQuickPick(
    findings.map((finding) => ({ label: finding.rule, description: finding.message, finding })),
    { placeHolder: "Several findings cover the cursor. Pick one." },
  );
  return picked?.finding;
}

/**
 * Prefers the language server's diagnostics, which follow unsaved edits, and falls back to the
 * last workspace scan for files the server has not linted.
 */
function findingsAt(view: FindingsView, uri: vscode.Uri, position: vscode.Position): Finding[] {
  const live = vscode.languages
    .getDiagnostics(uri)
    .filter((diagnostic) => diagnostic.source === "slopcop" && diagnostic.range.contains(position))
    .map((diagnostic): Finding => {
      const [message, observation] = diagnostic.message.split("\nobserved: ");
      const code = typeof diagnostic.code === "object" ? diagnostic.code.value : diagnostic.code;
      return { uri, rule: String(code), severity: SEVERITIES[diagnostic.severity] ?? "info", range: diagnostic.range, message, observation };
    });
  return live.length ? live : view.findingsAt(uri, position);
}

async function openIssue(template: string, fields: Record<string, string | undefined>): Promise<void> {
  // URLSearchParams writes spaces as "+", which GitHub can confuse with a literal plus sign.
  const query = [["template", template], ...Object.entries(fields)]
    .filter((entry): entry is [string, string] => Boolean(entry[1]))
    .map(([key, value]) => `${key}=${encodeURIComponent(value.length > MAX_FIELD ? value.slice(0, MAX_FIELD) + "\n…" : value)}`)
    .join("&");
  // openExternal accepts a string at runtime and opens it unchanged. A vscode.Uri would be
  // decoded and encoded again, which changes the escaped query.
  await vscode.env.openExternal(`${REPO_URL}/issues/new?${query}` as unknown as vscode.Uri);
}

/** Links to the line on GitHub at the checked-out commit, when the file is in a GitHub clone. */
async function permalink(uri: vscode.Uri, line: number): Promise<string | undefined> {
  const cwd = path.dirname(uri.fsPath);
  try {
    const [remote, commit, root] = await Promise.all([
      output("git", ["remote", "get-url", "origin"], cwd),
      output("git", ["rev-parse", "HEAD"], cwd),
      output("git", ["rev-parse", "--show-toplevel"], cwd),
    ]);
    const repository = remote.match(/github\.com[:/]([\w.-]+\/[\w.-]+?)(?:\.git)?$/)?.[1];
    if (!repository) return undefined;
    const file = path.relative(root, uri.fsPath).split(path.sep).map(encodeURIComponent).join("/");
    return `https://github.com/${repository}/blob/${commit}/${file}#L${line}`;
  } catch {
    // Not a Git checkout, or Git is not installed. The form asks for the context instead.
    return undefined;
  }
}

async function installedVersion(command: string): Promise<string> {
  try {
    return (await output(command, ["--version"])).replace(/^slopcop\s+/, "");
  } catch {
    return "unknown";
  }
}

function output(command: string, args: string[], cwd?: string): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(command, args, { cwd }, (error, stdout) => (error ? reject(error) : resolve(stdout.trim())));
  });
}
