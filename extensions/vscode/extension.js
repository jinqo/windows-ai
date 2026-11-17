// Windows AI for VS Code: sends selections, files and diagnostics to the
// desktop assistant over the authenticated local bridge. Nothing is sent
// unless the user runs a command.
const vscode = require("vscode");
const { buildCapture, formatDiagnostics, sendCapture } = require("./bridge");

function settings() {
  const c = vscode.workspace.getConfiguration("windowsAi");
  return { maxChars: c.get("maxChars", 100000), includePath: c.get("includeFilePath", true) };
}

function selectionOrLine(editor) {
  const sel = editor.selection;
  if (!sel.isEmpty) return editor.document.getText(sel);
  return editor.document.lineAt(sel.active.line).text;
}

async function deliver(payload) {
  try {
    await sendCapture(payload);
    vscode.window.setStatusBarMessage("$(sparkle) Sent to Windows AI", 3000);
  } catch (e) {
    vscode.window.showErrorMessage(`Windows AI: ${e.message}`);
  }
}

function codeCommand(action) {
  return () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor) return vscode.window.showWarningMessage("Windows AI: open a file first.");
    const text = selectionOrLine(editor);
    if (!text.trim()) return vscode.window.showWarningMessage("Windows AI: select some code first.");
    const doc = editor.document;
    return deliver(buildCapture({ kind: "code", content: text, language: doc.languageId, filePath: doc.uri.scheme === "file" ? doc.uri.fsPath : null, action, ...settings() }));
  };
}

function activate(context) {
  const reg = (id, fn) => context.subscriptions.push(vscode.commands.registerCommand(id, fn));
  reg("windowsAi.ask", codeCommand("ask"));
  reg("windowsAi.explain", codeCommand("explain"));
  reg("windowsAi.debug", codeCommand("debug"));
  reg("windowsAi.refactor", codeCommand("refactor"));
  reg("windowsAi.document", codeCommand("document"));
  reg("windowsAi.sendFile", () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor) return;
    const doc = editor.document;
    return deliver(buildCapture({ kind: "code", content: doc.getText(), language: doc.languageId, filePath: doc.uri.fsPath, action: "ask", ...settings() }));
  });
  reg("windowsAi.sendDiagnostics", () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor) return;
    const doc = editor.document;
    const diags = vscode.languages.getDiagnostics(doc.uri);
    if (!diags.length) return vscode.window.showInformationMessage("Windows AI: no problems reported for this file.");
    const lines = doc.getText().split(/\r?\n/);
    return deliver(buildCapture({ kind: "diagnostics", content: formatDiagnostics(diags, lines), language: doc.languageId, filePath: doc.uri.fsPath, action: "debug", ...settings() }));
  });
}

function deactivate() {}

module.exports = { activate, deactivate };
