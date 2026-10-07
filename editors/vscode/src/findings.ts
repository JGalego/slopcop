import { execFile } from "node:child_process";
import * as path from "node:path";
import * as vscode from "vscode";
import { slopocopEnabled } from "./slopocop";

export type Severity = "error" | "warning" | "info";

/** One finding from `slopcop --format json`, located in the workspace. */
export interface Finding {
  uri: vscode.Uri;
  rule: string;
  module?: string;
  severity: Severity;
  range: vscode.Range;
  message: string;
  observation?: string;
  evidence?: string;
}

interface ReportedFinding {
  path: string;
  location: { line: number; column: number; end_line?: number; end_column?: number };
  rule_id: string;
  module: string;
  severity: Severity;
  message: string;
  evidence?: string | null;
  observation?: string | null;
}

interface Report {
  version: string;
  findings: ReportedFinding[];
}

type GroupBy = "file" | "rule";

type Node =
  | { kind: "group"; label: string; uri?: vscode.Uri; findings: Finding[] }
  | { kind: "finding"; finding: Finding; byRule: boolean };

const SEVERITY_ICONS: Record<Severity, vscode.ThemeIcon> = {
  error: new vscode.ThemeIcon("error", new vscode.ThemeColor("problemsErrorIcon.foreground")),
  warning: new vscode.ThemeIcon("warning", new vscode.ThemeColor("problemsWarningIcon.foreground")),
  info: new vscode.ThemeIcon("info", new vscode.ThemeColor("problemsInfoIcon.foreground")),
};
const GROUP_BY_KEY = "slopcop.groupBy";
/** The plain view, and the same findings in slopocop's container when that persona is on. */
const VIEW_IDS = ["slopcop.findings", "slopocop.findings"];
const RESCAN_DELAY = 500;

/**
 * Lists every finding in the workspace. The language server only lints open files, so this view
 * runs the command-line scanner on each workspace folder and scans again after saves and file
 * operations.
 */
export class FindingsView implements vscode.TreeDataProvider<Node>, vscode.Disposable {
  private findings: Finding[] = [];
  private groupBy: GroupBy;
  private scanning = false;
  private pending = false;
  private timer: NodeJS.Timeout | undefined;
  private readonly changed = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this.changed.event;
  private readonly scanned = new vscode.EventEmitter<void>();
  /** Fires after each workspace scan. */
  readonly onDidScan = this.scanned.event;
  private readonly views: vscode.TreeView<Node>[];
  private readonly disposables: vscode.Disposable[] = [];
  /** The version reported by the last successful scan. */
  version: string | undefined;
  /** Whether the last scan failed in any workspace folder. */
  failed = false;

  constructor(
    private readonly context: vscode.ExtensionContext,
    private readonly executable: () => string,
  ) {
    this.groupBy = context.workspaceState.get<GroupBy>(GROUP_BY_KEY, "file");
    this.views = VIEW_IDS.map((id) => vscode.window.createTreeView(id, { treeDataProvider: this, showCollapseAll: true }));
    const watcher = vscode.workspace.createFileSystemWatcher("**/.slopcop.toml");
    const schedule = () => this.schedule();
    this.disposables.push(
      ...this.views,
      this.changed,
      this.scanned,
      watcher,
      watcher.onDidCreate(schedule),
      watcher.onDidChange(schedule),
      watcher.onDidDelete(schedule),
      vscode.workspace.onDidSaveTextDocument((document) => {
        if (vscode.workspace.getWorkspaceFolder(document.uri)) schedule();
      }),
      vscode.workspace.onDidCreateFiles(schedule),
      vscode.workspace.onDidDeleteFiles(schedule),
      vscode.workspace.onDidRenameFiles(schedule),
      vscode.workspace.onDidChangeWorkspaceFolders(schedule),
    );
    void this.setGroupBy(this.groupBy);
  }

  dispose(): void {
    clearTimeout(this.timer);
    for (const disposable of this.disposables) disposable.dispose();
  }

  async setGroupBy(groupBy: GroupBy): Promise<void> {
    this.groupBy = groupBy;
    await this.context.workspaceState.update(GROUP_BY_KEY, groupBy);
    await vscode.commands.executeCommand("setContext", GROUP_BY_KEY, groupBy);
    this.changed.fire();
  }

  /** The number of findings from the last scan, or undefined before the first one. */
  get count(): number | undefined {
    return this.version === undefined && !this.failed ? undefined : this.findings.length;
  }

  /** Findings from the last scan that cover the given position, most severe first. */
  findingsAt(uri: vscode.Uri, position: vscode.Position): Finding[] {
    return this.findings.filter((finding) => finding.uri.toString() === uri.toString() && finding.range.contains(position));
  }

  private schedule(): void {
    clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.refresh(), RESCAN_DELAY);
  }

  /** Scans the workspace. A request made during a scan runs once that scan finishes. */
  async refresh(): Promise<void> {
    if (this.scanning) {
      this.pending = true;
      return;
    }
    this.scanning = true;
    try {
      await vscode.window.withProgress({ location: { viewId: VIEW_IDS[slopocopEnabled() ? 1 : 0] } }, async () => {
        do {
          this.pending = false;
          await this.scan();
        } while (this.pending);
      });
    } finally {
      this.scanning = false;
    }
  }

  private async scan(): Promise<void> {
    const folders = (vscode.workspace.workspaceFolders ?? []).filter((folder) => folder.uri.scheme === "file");
    const findings: Finding[] = [];
    const failures: string[] = [];
    for (const folder of folders) {
      try {
        const report = await run(this.executable(), folder.uri.fsPath);
        this.version = report.version;
        for (const reported of report.findings) findings.push(locate(folder, reported));
      } catch (error) {
        failures.push(`${folders.length > 1 ? folder.name + ": " : ""}${error instanceof Error ? error.message : String(error)}`);
      }
    }
    findings.sort((a, b) => compare(a.uri.fsPath, b.uri.fsPath) || a.range.start.compareTo(b.range.start));
    this.findings = findings;
    this.failed = failures.length > 0;

    const count = findings.length;
    const noun = slopocopEnabled() ? (count === 1 ? "suspect" : "suspects") : `slopcop ${count === 1 ? "finding" : "findings"}`;
    for (const view of this.views) {
      view.badge = count ? { value: count, tooltip: `${count} ${noun}` } : undefined;
      view.message = failures.length ? `slopcop could not scan the workspace: ${failures.join("; ")}` : undefined;
    }
    await vscode.commands.executeCommand("setContext", "slopcop.clean", folders.length > 0 && count === 0 && failures.length === 0);
    this.changed.fire();
    this.scanned.fire();
  }

  getChildren(node?: Node): Node[] {
    if (node?.kind === "group") {
      return node.findings.map((finding) => ({ kind: "finding", finding, byRule: this.groupBy === "rule" }));
    }
    if (node) return [];
    const groups = new Map<string, Finding[]>();
    for (const finding of this.findings) {
      const key = this.groupBy === "file" ? finding.uri.toString() : finding.rule;
      groups.set(key, [...(groups.get(key) ?? []), finding]);
    }
    return [...groups.values()]
      .map((members): Node => this.groupBy === "file"
        ? { kind: "group", label: relativePath(members[0].uri), uri: members[0].uri, findings: members }
        : { kind: "group", label: members[0].rule, findings: members })
      .sort((a, b) => compare(a.kind === "group" ? a.label : "", b.kind === "group" ? b.label : ""));
  }

  getTreeItem(node: Node): vscode.TreeItem {
    if (node.kind === "group") {
      const first = node.findings[0];
      const count = String(node.findings.length);
      if (node.uri) {
        const item = new vscode.TreeItem(node.uri, vscode.TreeItemCollapsibleState.Expanded);
        const directory = path.posix.dirname(node.label);
        item.description = directory === "." ? count : `${directory} · ${count}`;
        item.tooltip = node.label;
        return item;
      }
      const item = new vscode.TreeItem(node.label, vscode.TreeItemCollapsibleState.Expanded);
      item.description = `${count} · ${first.module ?? ""}`;
      item.iconPath = SEVERITY_ICONS[first.severity];
      item.tooltip = first.message;
      return item;
    }

    const { finding, byRule } = node;
    const { line, character } = finding.range.start;
    const position = `[Ln ${line + 1}, Col ${character + 1}]`;
    const item = new vscode.TreeItem(byRule ? relativePath(finding.uri) : finding.message);
    item.description = byRule ? position : `${finding.rule} ${position}`;
    item.iconPath = SEVERITY_ICONS[finding.severity];
    item.tooltip = tooltip(finding);
    item.contextValue = "finding";
    item.command = {
      title: "Open Finding",
      command: "vscode.open",
      arguments: [finding.uri, { selection: finding.range } satisfies vscode.TextDocumentShowOptions],
    };
    return item;
  }
}

/** Returns the finding behind a tree item, for commands run from the view. */
export function findingOf(node: unknown): Finding | undefined {
  const candidate = node as Node | undefined;
  return candidate?.kind === "finding" ? candidate.finding : undefined;
}

export function relativePath(uri: vscode.Uri): string {
  return vscode.workspace.asRelativePath(uri, (vscode.workspace.workspaceFolders?.length ?? 0) > 1);
}

function run(command: string, cwd: string): Promise<Report> {
  return new Promise((resolve, reject) => {
    execFile(command, ["--format", "json", "."], { cwd, maxBuffer: 256 * 1024 * 1024 }, (error, stdout, stderr) => {
      // slopcop exits with status 1 when it reports findings, so the output decides success.
      try {
        resolve(JSON.parse(stdout) as Report);
      } catch {
        reject(new Error(stderr.trim() || error?.message || "slopcop printed no report."));
      }
    });
  });
}

function locate(folder: vscode.WorkspaceFolder, reported: ReportedFinding): Finding {
  const { line, column, end_line = line, end_column = column } = reported.location;
  return {
    uri: vscode.Uri.file(path.resolve(folder.uri.fsPath, reported.path)),
    rule: reported.rule_id,
    module: reported.module,
    severity: reported.severity,
    range: new vscode.Range(line - 1, column - 1, end_line - 1, end_column - 1),
    message: reported.message,
    observation: reported.observation ?? undefined,
    evidence: reported.evidence ?? undefined,
  };
}

function tooltip(finding: Finding): vscode.MarkdownString {
  const text = new vscode.MarkdownString();
  text.appendMarkdown(`**${finding.rule}** · ${finding.severity}${finding.module ? " · " + finding.module : ""}\n\n`);
  text.appendText(finding.message);
  if (finding.observation) text.appendText(`\nobserved: ${finding.observation}`);
  if (finding.evidence) text.appendCodeblock(finding.evidence);
  return text;
}

function compare(a: string, b: string): number {
  return a.localeCompare(b, undefined, { numeric: true });
}
