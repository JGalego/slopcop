//! A Language Server Protocol front end for editors.
//!
//! The server keeps the text of open documents, lints each one with the same scanner and
//! configuration discovery as `--stdin`, and publishes the findings as diagnostics. Hover shows
//! the rule reference, and a quick fix inserts a suppression directive for the user to justify.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lsp_server::{Connection, ErrorCode, Message, Notification, ProtocolError, Request, Response};
use percent_encoding::percent_decode_str;
use serde_json::{Value, json};

use crate::config::{CONFIG_FILE_NAME, Config};
use crate::language::{Language, SourceType, classify};
use crate::model::{Finding, Location, Module, Severity};
use crate::rules::metadata_registry;
use crate::scanner::{ScanOptions, SourceFile, scan_sources};

/// Serves the protocol on standard input and output until the client asks it to exit.
///
/// # Errors
///
/// Returns an error when the client breaks the protocol or the standard streams fail.
pub fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (connection, io_threads) = Connection::stdio();
    serve(&connection)?;
    drop(connection);
    io_threads.join()?;
    Ok(())
}

/// Serves the protocol on an established connection.
///
/// # Errors
///
/// Returns an error when initialization or shutdown does not follow the protocol.
pub fn serve(connection: &Connection) -> Result<(), ProtocolError> {
    let (id, _params) = connection.initialize_start()?;
    connection.initialize_finish(
        id,
        json!({
            "capabilities": {
                "positionEncoding": "utf-16",
                "textDocumentSync": {
                    "openClose": true,
                    "change": 1,
                    "save": { "includeText": false }
                },
                "hoverProvider": true,
                "codeActionProvider": { "codeActionKinds": ["quickfix"] }
            },
            "serverInfo": { "name": "slopcop", "version": env!("CARGO_PKG_VERSION") }
        }),
    )?;

    let mut server = Server::default();
    for message in &connection.receiver {
        let replies = match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    return Ok(());
                }
                vec![Message::Response(server.request(request))]
            }
            Message::Notification(notification) => server.notification(&notification),
            Message::Response(_) => Vec::new(),
        };
        for reply in replies {
            if connection.sender.send(reply).is_err() {
                return Ok(());
            }
        }
    }
    Ok(())
}

struct Document {
    path: PathBuf,
    text: String,
    findings: Vec<Finding>,
}

#[derive(Default)]
struct Server {
    documents: HashMap<String, Document>,
    reported_config_error: Option<String>,
}

impl Server {
    fn request(&self, request: Request) -> Response {
        let result = match request.method.as_str() {
            "textDocument/hover" => self.hover(&request.params),
            "textDocument/codeAction" => self.code_actions(&request.params),
            _ => {
                return Response::new_err(
                    request.id,
                    ErrorCode::MethodNotFound as i32,
                    format!("unsupported request {}", request.method),
                );
            }
        };
        Response::new_ok(request.id, result)
    }

    fn notification(&mut self, notification: &Notification) -> Vec<Message> {
        let params = &notification.params;
        let uri = params["textDocument"]["uri"].as_str().unwrap_or_default();
        match notification.method.as_str() {
            "textDocument/didOpen" => {
                let Some(path) = file_path(uri) else {
                    return Vec::new();
                };
                let text = params["textDocument"]["text"].as_str().unwrap_or_default();
                self.documents.insert(
                    uri.to_owned(),
                    Document {
                        path,
                        text: text.to_owned(),
                        findings: Vec::new(),
                    },
                );
                self.lint(&[uri.to_owned()])
            }
            "textDocument/didChange" => {
                let changes = params["contentChanges"].as_array();
                let text = changes
                    .and_then(|changes| changes.last())
                    .and_then(|change| change["text"].as_str());
                match (self.documents.get_mut(uri), text) {
                    (Some(document), Some(text)) => {
                        text.clone_into(&mut document.text);
                        self.lint(&[uri.to_owned()])
                    }
                    _ => Vec::new(),
                }
            }
            "textDocument/didSave" => {
                let is_config = file_path(uri)
                    .is_some_and(|path| path.file_name() == Some(CONFIG_FILE_NAME.as_ref()));
                if is_config {
                    self.lint_all()
                } else {
                    self.lint(&[uri.to_owned()])
                }
            }
            "workspace/didChangeWatchedFiles" => self.lint_all(),
            "textDocument/didClose" => {
                self.documents.remove(uri);
                vec![publish(uri, &[])]
            }
            _ => Vec::new(),
        }
    }

    fn lint_all(&mut self) -> Vec<Message> {
        let uris: Vec<_> = self.documents.keys().cloned().collect();
        self.lint(&uris)
    }

    fn lint(&mut self, uris: &[String]) -> Vec<Message> {
        let mut messages = Vec::new();
        for uri in uris {
            let Some(document) = self.documents.get_mut(uri) else {
                continue;
            };
            document.findings = match lint_document(&document.path, &document.text) {
                Ok(findings) => {
                    self.reported_config_error = None;
                    findings
                }
                Err(error) => {
                    // Report each distinct configuration error once rather than on every edit.
                    if self.reported_config_error.as_ref() != Some(&error) {
                        messages.push(Message::Notification(Notification::new(
                            "window/showMessage".to_owned(),
                            json!({ "type": 1, "message": format!("slopcop: {error}") }),
                        )));
                        self.reported_config_error = Some(error);
                    }
                    Vec::new()
                }
            };
            let diagnostics: Vec<_> = document
                .findings
                .iter()
                .map(|finding| diagnostic(&document.text, finding))
                .collect();
            messages.push(publish(uri, &diagnostics));
        }
        messages
    }

    /// Returns the open document and its findings that touch zero-based lines `start..=end`.
    fn findings_at(
        &self,
        params: &Value,
        start: u64,
        end: u64,
    ) -> Option<(&Document, Vec<&Finding>)> {
        let uri = params["textDocument"]["uri"].as_str()?;
        let document = self.documents.get(uri)?;
        let first = usize::try_from(start).ok()? + 1;
        let last = usize::try_from(end).ok()? + 1;
        let findings = document
            .findings
            .iter()
            .filter(|finding| finding.location.line <= last && finding.location.end_line >= first)
            .collect();
        Some((document, findings))
    }

    fn hover(&self, params: &Value) -> Value {
        let Some(line) = params["position"]["line"].as_u64() else {
            return Value::Null;
        };
        let Some((_, findings)) = self.findings_at(params, line, line) else {
            return Value::Null;
        };
        let sections: Vec<_> = findings.iter().map(|finding| hover_text(finding)).collect();
        if sections.is_empty() {
            return Value::Null;
        }
        json!({ "contents": { "kind": "markdown", "value": sections.join("\n\n---\n\n") } })
    }

    fn code_actions(&self, params: &Value) -> Value {
        let range = &params["range"];
        let (Some(start), Some(end)) = (
            range["start"]["line"].as_u64(),
            range["end"]["line"].as_u64(),
        ) else {
            return json!([]);
        };
        let Some((document, findings)) = self.findings_at(params, start, end) else {
            return json!([]);
        };
        let Some(comment) = comment_syntax(&document.path) else {
            return json!([]);
        };
        let uri = params["textDocument"]["uri"].as_str().unwrap_or_default();
        let newline = if document.text.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let mut seen = Vec::new();
        let mut actions = Vec::new();
        for finding in findings {
            let key = (finding.rule_id, finding.location.line);
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            let line_text = document
                .text
                .split('\n')
                .nth(finding.location.line - 1)
                .unwrap_or_default();
            let indent: String = line_text
                .chars()
                .take_while(|character| *character == ' ' || *character == '\t')
                .collect();
            let directive = comment.wrap(&format!("slopcop: ignore {} -- ", finding.rule_id));
            let insert_at = json!({ "line": finding.location.line - 1, "character": 0 });
            actions.push(json!({
                "title": format!("Suppress {} on this line (write the reason after --)", finding.rule_id),
                "kind": "quickfix",
                "diagnostics": [diagnostic(&document.text, finding)],
                "edit": {
                    "changes": {
                        uri: [{
                            "range": { "start": insert_at, "end": insert_at },
                            "newText": format!("{indent}{directive}{newline}")
                        }]
                    }
                }
            }));
        }
        Value::Array(actions)
    }
}

fn lint_document(path: &Path, text: &str) -> Result<Vec<Finding>, String> {
    if classify(path) == SourceType::Unknown {
        return Ok(Vec::new());
    }
    let config = Config::load_from(None, path.parent()).map_err(|error| error.to_string())?;
    let options = ScanOptions {
        max_file_size: config.max_file_size,
        config,
    };
    let source = SourceFile {
        path: path.to_path_buf(),
        bytes: text.as_bytes().to_vec(),
    };
    Ok(scan_sources(vec![source], &options).findings)
}

fn publish(uri: &str, diagnostics: &[Value]) -> Message {
    Message::Notification(Notification::new(
        "textDocument/publishDiagnostics".to_owned(),
        json!({ "uri": uri, "diagnostics": diagnostics }),
    ))
}

fn diagnostic(text: &str, finding: &Finding) -> Value {
    let message = match &finding.observation {
        Some(observation) => format!("{}\nobserved: {observation}", finding.message),
        None => finding.message.clone(),
    };
    json!({
        "range": range(text, &finding.location),
        "severity": match finding.severity {
            Severity::Error => 1,
            Severity::Warning => 2,
            Severity::Info => 3,
        },
        "code": finding.rule_id,
        "codeDescription": { "href": rule_reference(finding.module) },
        "source": "slopcop",
        "message": message
    })
}

fn rule_reference(module: Module) -> String {
    format!(
        "{}/blob/main/docs/rules/README.md#{module}",
        env!("CARGO_PKG_REPOSITORY")
    )
}

fn hover_text(finding: &Finding) -> String {
    let Some(metadata) = metadata_registry()
        .into_iter()
        .find(|metadata| metadata.id == finding.rule_id)
    else {
        return format!("**{}**: {}", finding.rule_id, finding.message);
    };
    let replacements = if metadata.replacements.is_empty() {
        String::new()
    } else {
        let pairs: Vec<String> = metadata
            .replacements
            .iter()
            .map(|(expression, replacement)| format!("`{expression}` → {replacement}"))
            .collect();
        format!("\n\n**Replacements:** {}", pairs.join("; "))
    };
    format!(
        "**{}** · {} · {}\n\n{}\n\n{}\n\n**Suggestion:** {}{replacements}\n\n**False positives:** {}",
        metadata.id,
        metadata.module,
        finding.severity,
        metadata.description,
        metadata.rationale,
        metadata.suggestion,
        metadata.false_positives,
    )
}

/// Converts a one-based, character-counted location into a zero-based LSP range in UTF-16 code
/// units, the encoding every client supports.
fn range(text: &str, location: &Location) -> Value {
    json!({
        "start": position(text, location.line, location.column),
        "end": position(text, location.end_line, location.end_column)
    })
}

fn position(text: &str, line: usize, column: usize) -> Value {
    let line_text = text
        .split('\n')
        .nth(line.saturating_sub(1))
        .unwrap_or_default();
    let character: usize = line_text
        .chars()
        .take(column.saturating_sub(1))
        .map(char::len_utf16)
        .sum();
    json!({ "line": line.saturating_sub(1), "character": character })
}

/// Maps a `file:` URI to a local path. Other schemes, such as unsaved buffers, have no path for
/// configuration discovery and are not linted.
fn file_path(uri: &str) -> Option<PathBuf> {
    let encoded = uri.strip_prefix("file://")?;
    let decoded = percent_decode_str(encoded).decode_utf8().ok()?;
    let path = decoded.strip_prefix("localhost").unwrap_or(&decoded);
    let bytes = path.as_bytes();
    // `file:///C:/src` names a Windows drive, so the leading slash is not part of the path.
    if bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':' {
        return Some(PathBuf::from(&path[1..]));
    }
    if !path.starts_with('/') {
        // A host name other than localhost names a network share.
        return Some(PathBuf::from(format!("//{path}")));
    }
    Some(PathBuf::from(path))
}

enum CommentSyntax {
    Line(&'static str),
    Block(&'static str, &'static str),
}

impl CommentSyntax {
    fn wrap(&self, text: &str) -> String {
        match self {
            Self::Line(prefix) => format!("{prefix} {text}"),
            Self::Block(open, close) => format!("{open} {text}{close}"),
        }
    }
}

fn comment_syntax(path: &Path) -> Option<CommentSyntax> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match classify(path) {
        SourceType::Code(Language::Python | Language::Ruby | Language::Shell) => {
            Some(CommentSyntax::Line("#"))
        }
        SourceType::Code(_) => Some(CommentSyntax::Line("//")),
        SourceType::Documentation => match extension.as_str() {
            "rst" => Some(CommentSyntax::Line("..")),
            "adoc" | "asciidoc" => Some(CommentSyntax::Line("//")),
            _ => Some(CommentSyntax::Block("<!--", "-->")),
        },
        SourceType::Configuration => match extension.as_str() {
            "yaml" | "yml" | "toml" => Some(CommentSyntax::Line("#")),
            "xml" => Some(CommentSyntax::Block("<!--", "-->")),
            _ => None,
        },
        SourceType::Text | SourceType::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uris_map_to_local_paths() {
        assert_eq!(
            file_path("file:///home/me/a%20b.py"),
            Some(PathBuf::from("/home/me/a b.py"))
        );
        assert_eq!(
            file_path("file:///c%3A/src/app.py"),
            Some(PathBuf::from("c:/src/app.py"))
        );
        assert_eq!(
            file_path("file://localhost/srv/app.py"),
            Some(PathBuf::from("/srv/app.py"))
        );
        assert_eq!(
            file_path("file://server/share/app.py"),
            Some(PathBuf::from("//server/share/app.py"))
        );
        assert_eq!(file_path("untitled:Untitled-1"), None);
    }

    #[test]
    fn positions_count_utf16_code_units() {
        let text = "first\n😀 é todo\n";
        let location = Location {
            line: 2,
            column: 3,
            end_line: 2,
            end_column: 9,
        };
        assert_eq!(
            range(text, &location),
            json!({
                "start": { "line": 1, "character": 3 },
                "end": { "line": 1, "character": 9 }
            })
        );
    }

    #[test]
    fn suppressions_use_the_file_comment_syntax() {
        let directive = "slopcop: ignore DEAD002 -- ";
        let wrap =
            |path: &str| comment_syntax(Path::new(path)).map(|syntax| syntax.wrap(directive));
        assert_eq!(
            wrap("a.py").as_deref(),
            Some("# slopcop: ignore DEAD002 -- ")
        );
        assert_eq!(
            wrap("a.rs").as_deref(),
            Some("// slopcop: ignore DEAD002 -- ")
        );
        assert_eq!(
            wrap("a.md").as_deref(),
            Some("<!-- slopcop: ignore DEAD002 -- -->")
        );
        assert_eq!(wrap("a.json"), None);
    }
}
