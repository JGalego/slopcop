import * as fs from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";
import {
  LanguageClient,
  type LanguageClientOptions,
  type ServerOptions,
} from "vscode-languageclient/node";
import { FindingsView } from "./findings";
import { registerReports } from "./reports";

// Mirrors the extensions that slopcop classifies in src/language.rs. Other files are not sent to
// the server, which would ignore them anyway.
const EXTENSIONS = [
  "py", "pyi", "js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts", "go", "rs", "java", "c", "h",
  "cc", "cpp", "cxx", "hh", "hpp", "hxx", "cs", "rb", "php", "swift", "kt", "kts", "sh", "bash",
  "zsh", "fish", "md", "mdx", "rst", "adoc", "asciidoc", "txt", "yaml", "yml", "json", "toml", "xml",
];

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const executable = () => serverCommand(context);
  const findings = new FindingsView(context, executable);
  context.subscriptions.push(
    findings,
    vscode.commands.registerCommand("slopcop.restart", () => restart(context)),
    vscode.commands.registerCommand("slopcop.refresh", () => findings.refresh()),
    vscode.commands.registerCommand("slopcop.groupByFile", () => findings.setGroupBy("file")),
    vscode.commands.registerCommand("slopcop.groupByRule", () => findings.setGroupBy("rule")),
    ...registerReports(findings, executable),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("slopcop.path")) {
        void findings.refresh();
        return restart(context);
      }
    }),
  );
  void findings.refresh();
  await start(context);
}

export async function deactivate(): Promise<void> {
  await stop();
}

async function restart(context: vscode.ExtensionContext): Promise<void> {
  await stop();
  await start(context);
}

async function stop(): Promise<void> {
  const running = client;
  client = undefined;
  await running?.stop();
}

/** Prefers the configured path, then a binary packaged with the extension, then `PATH`. */
function serverCommand(context: vscode.ExtensionContext): string {
  const configured = vscode.workspace.getConfiguration("slopcop").get<string>("path", "").trim();
  if (configured) {
    return configured;
  }
  const executable = process.platform === "win32" ? "slopcop.exe" : "slopcop";
  const bundled = context.asAbsolutePath(path.join("bin", executable));
  if (!fs.existsSync(bundled)) {
    return "slopcop";
  }
  if (process.platform !== "win32") {
    try {
      // VSIX archives do not reliably keep the executable bit.
      fs.chmodSync(bundled, 0o755);
    } catch {
      // A read-only install keeps the mode it was unpacked with.
    }
  }
  return bundled;
}

async function start(context: vscode.ExtensionContext): Promise<void> {
  const command = serverCommand(context);
  const serverOptions: ServerOptions = { command, args: ["lsp"] };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: "file", pattern: `**/*.{${EXTENSIONS.join(",")}}` },
      { scheme: "file", pattern: "**/README" },
    ],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/.slopcop.toml"),
    },
  };
  const candidate = new LanguageClient("slopcop", "slopcop", serverOptions, clientOptions);
  try {
    await candidate.start();
    client = candidate;
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    const choice = await vscode.window.showErrorMessage(
      `slopcop could not start "${command} lsp" (${reason}). Install slopcop or set slopcop.path.`,
      "Open Settings",
    );
    if (choice) {
      await vscode.commands.executeCommand("workbench.action.openSettings", "slopcop.path");
    }
  }
}
