import * as vscode from "vscode";
import type { FindingsView } from "./findings";

const WALKTHROUGH = "JGalego.slopcop#slopocop";

/** Whether the opt-in slopocop persona is on. It changes presentation only, never findings. */
export function slopocopEnabled(): boolean {
  return vscode.workspace.getConfiguration("slopcop").get<boolean>("slopocop", false);
}

/** Shows slopocop's patrol count in the status bar and greets him when the setting turns on. */
export function registerSlopocop(view: FindingsView): vscode.Disposable[] {
  const item = vscode.window.createStatusBarItem("slopcop.slopocop", vscode.StatusBarAlignment.Left);
  item.name = "slopocop";
  item.command = "workbench.view.extension.slopocop";

  const update = () => {
    const count = view.count;
    if (!slopocopEnabled() || view.failed) {
      item.hide();
      return;
    }
    if (count === undefined) {
      item.text = "$(record) On patrol";
      item.tooltip = "slopocop is sweeping the workspace.";
    } else if (count === 0) {
      item.text = "$(record) Area secure";
      item.tooltip = "slopocop: area secure. Thank you for your cooperation.";
    } else {
      const noun = count === 1 ? "suspect" : "suspects";
      item.text = `$(record) ${count} ${noun}`;
      item.tooltip = `slopocop: ${count} ${noun} in custody. Your move.`;
    }
    item.show();
  };

  update();
  return [
    item,
    view.onDidScan(update),
    vscode.commands.registerCommand("slopocop.directives", () =>
      vscode.commands.executeCommand("workbench.action.openWalkthrough", WALKTHROUGH, false)),
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (!event.affectsConfiguration("slopcop.slopocop")) return;
      update();
      void view.refresh();
      if (!slopocopEnabled()) return;
      const choice = await vscode.window.showInformationMessage("slopocop online. Your move.", "Read the Directives");
      if (choice) await vscode.commands.executeCommand("slopocop.directives");
    }),
  ];
}
