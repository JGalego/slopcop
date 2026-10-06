#![cfg(feature = "lsp")]

use std::fs;
use std::io::{BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use lsp_server::{Message, Notification, Request, RequestId};
use serde_json::{Value, json};

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i32,
}

impl Client {
    /// Starts `slopcop lsp` in `root` and completes the initialize handshake.
    fn start(root: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_slopcop"))
            .arg("lsp")
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("start language server");
        let mut client = Self {
            stdin: child.stdin.take().expect("stdin"),
            stdout: BufReader::new(child.stdout.take().expect("stdout")),
            child,
            next_id: 0,
        };
        let initialized = client.request("initialize", json!({ "capabilities": {} }));
        assert_eq!(initialized["serverInfo"]["name"], "slopcop");
        assert_eq!(initialized["capabilities"]["hoverProvider"], true);
        client.notify("initialized", json!({}));
        client
    }

    fn open(&mut self, uri: &str, text: &str) -> Value {
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": "python", "version": 1, "text": text } }),
        );
        self.diagnostics()
    }

    fn change(&mut self, uri: &str, text: &str) -> Value {
        self.notify(
            "textDocument/didChange",
            json!({ "textDocument": { "uri": uri }, "contentChanges": [{ "text": text }] }),
        );
        self.diagnostics()
    }

    fn shutdown(mut self) {
        assert_eq!(self.request("shutdown", Value::Null), Value::Null);
        self.notify("exit", Value::Null);
        assert!(self.child.wait().expect("server exit").success());
    }

    fn send(&mut self, message: &Message) {
        message.write(&mut self.stdin).expect("write message");
        self.stdin.flush().expect("flush message");
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(&Message::Notification(Notification::new(
            method.to_owned(),
            params,
        )));
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = RequestId::from(self.next_id);
        self.send(&Message::Request(Request::new(
            id.clone(),
            method.to_owned(),
            params,
        )));
        loop {
            if let Message::Response(response) = self.receive() {
                assert_eq!(response.id, id);
                return response.response_result.expect("successful response");
            }
        }
    }

    fn receive(&mut self) -> Message {
        Message::read(&mut self.stdout)
            .expect("read message")
            .expect("server message")
    }

    fn diagnostics(&mut self) -> Value {
        loop {
            if let Message::Notification(notification) = self.receive() {
                if notification.method == "textDocument/publishDiagnostics" {
                    return notification.params;
                }
            }
        }
    }
}

fn file_uri(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    let path = path.replace(' ', "%20");
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

const SOURCE: &str = "def run():\n    # TODO: implement retries\n    return 1\n";

#[test]
fn language_server_publishes_explains_and_suppresses_findings() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path();
    fs::create_dir_all(root.join("my app")).expect("create project");
    fs::write(root.join("my app/.slopcop.toml"), "[slopcop]\n").expect("write config");
    let uri = file_uri(&root.join("my app/main.py"));
    let mut client = Client::start(root);

    let published = client.open(&uri, SOURCE);
    assert_eq!(published["uri"], uri);
    let diagnostic = &published["diagnostics"][0];
    assert_eq!(diagnostic["code"], "DEAD002");
    assert_eq!(diagnostic["source"], "slopcop");
    assert_eq!(diagnostic["severity"], 2);
    assert_eq!(
        diagnostic["range"],
        json!({ "start": { "line": 1, "character": 6 }, "end": { "line": 1, "character": 29 } })
    );

    let hover = client.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 1, "character": 8 } }),
    );
    let hover_text = hover["contents"]["value"].as_str().expect("hover markdown");
    assert!(hover_text.starts_with("**DEAD002** · deadweight · warning"));
    assert!(hover_text.contains("**False positives:**"));
    let nothing = client.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 2, "character": 4 } }),
    );
    assert_eq!(nothing, Value::Null);

    let actions = client.request(
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": uri },
            "range": { "start": { "line": 1, "character": 0 }, "end": { "line": 1, "character": 0 } },
            "context": { "diagnostics": [diagnostic] }
        }),
    );
    let action = &actions[0];
    assert_eq!(action["kind"], "quickfix");
    let edit = &action["edit"]["changes"][uri.as_str()][0];
    assert_eq!(
        edit["newText"], "    # slopcop: ignore DEAD002 -- \n",
        "the directive keeps the line's indentation and leaves the reason to the author"
    );
    assert_eq!(edit["range"]["start"], json!({ "line": 1, "character": 0 }));

    let justified = SOURCE.replace(
        "    # TODO",
        "    # slopcop: ignore DEAD002 -- tracked in issue 12\n    # TODO",
    );
    assert_eq!(client.change(&uri, &justified)["diagnostics"], json!([]));
    client.shutdown();
}

#[test]
fn language_server_follows_configuration_changes() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path();
    let config = root.join(".slopcop.toml");
    fs::write(&config, "[slopcop]\n").expect("write config");
    let uri = file_uri(&root.join("main.py"));
    let mut client = Client::start(root);
    assert_eq!(client.open(&uri, SOURCE)["diagnostics"][0]["severity"], 2);

    fs::write(&config, "[slopcop.rules]\nDEAD002 = \"error\"\n").expect("update config");
    assert_eq!(client.change(&uri, SOURCE)["diagnostics"][0]["severity"], 1);

    fs::write(&config, "[slopcop.rules]\nNOPE001 = \"error\"\n").expect("break config");
    client.notify(
        "textDocument/didSave",
        json!({ "textDocument": { "uri": file_uri(&config) } }),
    );
    let Message::Notification(error) = client.receive() else {
        panic!("expected a configuration error message");
    };
    assert_eq!(error.method, "window/showMessage");
    assert!(
        error.params["message"]
            .as_str()
            .expect("message")
            .contains("NOPE001")
    );
    assert_eq!(client.diagnostics()["diagnostics"], json!([]));

    let unsaved = client.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": "untitled:Untitled-1" }, "position": { "line": 0, "character": 0 } }),
    );
    assert_eq!(unsaved, Value::Null);

    client.notify(
        "textDocument/didClose",
        json!({ "textDocument": { "uri": uri } }),
    );
    assert_eq!(client.diagnostics()["diagnostics"], json!([]));
    client.shutdown();
}
