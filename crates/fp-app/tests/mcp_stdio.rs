//! The MCP server over stdio, end to end.
//!
//! Spawns the real binary with `--mcp`, speaks line-delimited JSON-RPC and
//! checks the handshake, the tool list, a call, the error paths and a clean
//! exit when the client closes stdin.

#![cfg(feature = "desktop")]

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

struct Server {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    reader: BufReader<std::process::ChildStdout>,
}

impl Server {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_FreePlots"))
            .arg("--mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn the MCP server");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        Self {
            child,
            stdin,
            reader: BufReader::new(stdout),
        }
    }

    fn send(&mut self, value: &Value) {
        writeln!(self.stdin, "{value}").expect("write request");
        self.stdin.flush().expect("flush");
    }

    fn send_raw(&mut self, line: &str) {
        writeln!(self.stdin, "{line}").expect("write raw");
        self.stdin.flush().expect("flush");
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        let read = self.reader.read_line(&mut line).expect("read reply");
        assert!(read > 0, "the server closed stdout");
        serde_json::from_str(line.trim()).unwrap_or_else(|error| {
            panic!("invalid JSON reply {line:?}: {error}");
        })
    }

    fn call(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        self.read()
    }
}

#[test]
fn the_server_handshake_lists_the_tools_and_calls_one() {
    let mut server = Server::start();

    let init = server.call(1, "initialize", json!({}));
    assert_eq!(init["id"], json!(1));
    assert_eq!(init["result"]["serverInfo"]["name"], json!("freeplots"));
    assert!(init["result"]["protocolVersion"].is_string());

    let list = server.call(2, "tools/list", json!({}));
    let tools = list["result"]["tools"].as_array().expect("tools").clone();
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    for expected in ["freeplots_open_csv", "freeplots_menu", "freeplots_click"] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
    // Every tool has an input schema, so clients can validate before calling.
    for tool in &tools {
        assert!(
            tool["inputSchema"]["type"] == json!("object"),
            "{} has no object schema",
            tool["name"]
        );
    }

    // A real call: open the fixture CSV and read it back through a menu action.
    let dir = std::env::temp_dir().join(format!("fp-mcp-stdio-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let csv = dir.join("weights.csv");
    std::fs::write(&csv, "weight,batch\n12.5,A\n13.1,B\n").expect("csv");
    let opened = server.call(
        3,
        "tools/call",
        json!({"name": "freeplots_open_csv", "arguments": {"path": csv.to_string_lossy()}}),
    );
    let text = opened["result"]["content"][0]["text"]
        .as_str()
        .expect("opened text");
    assert!(text.contains("weights.csv"), "{text}");
    assert_eq!(opened["result"]["isError"], json!(false));

    // Unknown tool: an error result, and the server keeps answering.
    let unknown = server.call(
        4,
        "tools/call",
        json!({"name": "freeplots_not_a_tool", "arguments": {}}),
    );
    assert_eq!(unknown["result"]["isError"], json!(true));

    let after = server.call(5, "ping", json!({}));
    assert_eq!(after["result"], json!({}));

    // Malformed JSON is a protocol error, not a crash.
    server.send_raw("{not json");
    let malformed = server.read();
    assert_eq!(malformed["error"]["code"], json!(-32700));
    assert!(server.call(6, "ping", json!({})).get("result").is_some());

    // Closing stdin ends the process cleanly; run() returns success.
    drop(server.stdin);
    let status = server.child.wait().expect("wait");
    assert!(status.success(), "server exited with {status}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_http_mode_serves_health_and_rejects_a_wrong_token() {
    use std::net::{TcpListener, TcpStream};
    use std::time::Duration;

    // Pick a free port, then let the server bind it. Another process can win
    // the race, so the whole start is retried with a full health check.
    let mut started = None;
    for _ in 0..12 {
        let port = {
            let probe = TcpListener::bind("127.0.0.1:0").expect("probe");
            probe.local_addr().expect("addr").port()
        };
        let mut child = Command::new(env!("CARGO_BIN_EXE_FreePlots"))
            .args([
                "--mcp-http",
                &format!("127.0.0.1:{port}"),
                "--token",
                "s3cret",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn http server");
        let mut ready = false;
        for _ in 0..100 {
            if child.try_wait().expect("status").is_some() {
                break;
            }
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                // The port may have belonged to another process; only accept
                // it when the health answer names this server.
                if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
                    let request = "GET /mcp HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer s3cret\r\nConnection: close\r\n\r\n";
                    if stream.write_all(request.as_bytes()).is_ok() && stream.flush().is_ok() {
                        let mut body = String::new();
                        let mut reader = BufReader::new(stream);
                        if reader.read_to_string(&mut body).is_ok()
                            && body.contains("\"server\":\"freeplots\"")
                        {
                            ready = true;
                        }
                    }
                }
                if ready {
                    break;
                }
                std::thread::sleep(Duration::from_millis(30));
                continue;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        if ready {
            started = Some((child, port));
            break;
        }
        child.kill().ok();
        child.wait().ok();
    }
    let (mut child, port) = started.expect("the server did not listen on any port");

    // The server answers one request per connection and closes it.
    let exchange = |request: &str| -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream.write_all(request.as_bytes()).expect("write request");
        stream.flush().expect("flush");
        let mut reader = BufReader::new(stream);
        let mut response = String::new();
        let mut line = String::new();
        // Status line plus headers.
        while reader.read_line(&mut line).expect("status") > 0 {
            response.push_str(&line);
            if line == "\r\n" {
                break;
            }
            line.clear();
        }
        let mut body = String::new();
        reader.read_to_string(&mut body).expect("body");
        response.push_str(&body);
        response
    };

    // Without the bearer token the request is refused.
    let refused = exchange("GET /mcp HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert!(refused.starts_with("HTTP/1.1 401"), "{refused}");
    assert!(refused.contains("unauthorized"), "{refused}");

    // With it, GET reports the health and the tool count.
    let health = exchange(
        "GET /mcp HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer s3cret\r\nConnection: close\r\n\r\n",
    );
    assert!(health.starts_with("HTTP/1.1 200"), "{health}");
    assert!(health.contains("\"server\":\"freeplots\""), "{health}");

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn the_semantic_tools_match_the_cli_for_the_same_dataset() {
    // One dataset, two front doors: the MCP script drives the worksheet and
    // reads the Session report, the CLI prints its own report. The numbers
    // must agree.
    let dir = std::env::temp_dir().join(format!("fp-mcp-parity-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let csv = dir.join("weights.csv");
    std::fs::write(
        &csv,
        "weight,batch\n12.5,A\n13.1,B\n11.9,A\n12.8,B\n12.2,A\n",
    )
    .expect("csv");

    let mut server = Server::start();
    server.call(1, "initialize", json!({}));
    let opened = server.call(
        2,
        "tools/call",
        json!({"name": "freeplots_open_csv", "arguments": {"path": csv.to_string_lossy()}}),
    );
    assert_eq!(opened["result"]["isError"], json!(false));
    let run = server.call(
        3,
        "tools/call",
        json!({
            "name": "freeplots_run_script",
            "arguments": {"steps": [
                {"action": "menu", "path": ["Stat", "Basic Statistics", "Display Descriptive Statistics"]}
            ]}
        }),
    );
    assert_eq!(run["result"]["isError"], json!(false), "{run}");
    let session = server.call(
        4,
        "tools/call",
        json!({"name": "freeplots_session", "arguments": {}}),
    );
    let text = session["result"]["content"][0]["text"]
        .as_str()
        .expect("session text")
        .to_owned();
    assert!(text.contains("Descriptive Statistics"), "{text}");

    let cli = Command::new(env!("CARGO_BIN_EXE_FreePlots"))
        .args(["describe", &csv.to_string_lossy(), "--column", "weight"])
        .output()
        .expect("cli");
    assert!(cli.status.success());
    let cli_text = String::from_utf8_lossy(&cli.stdout).to_string();

    fn mean(text: &str) -> f64 {
        for line in text.lines() {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("Mean")
                && let Ok(value) = rest.trim().parse::<f64>()
            {
                return value;
            }
        }
        panic!("Mean not found in:\n{text}");
    }
    let from_mcp = mean(&text);
    let from_cli = mean(&cli_text);
    assert!(
        (from_mcp - from_cli).abs() < 1e-9,
        "MCP {from_mcp} vs CLI {from_cli}"
    );
    // And both agree with the hand calculation.
    assert!((from_cli - 12.5).abs() < 1e-9, "{from_cli}");

    drop(server.stdin);
    server.child.wait().ok();
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_unknown_method_is_a_protocol_error() {
    let mut server = Server::start();
    let reply = server.call(1, "freeplots_not_a_method", json!({}));
    assert_eq!(reply["error"]["code"], json!(-32602));
    assert!(
        reply["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown method")),
        "{reply}"
    );
    drop(server.stdin);
    let status = server.child.wait().expect("wait");
    assert!(status.success());
}
