// SPDX-License-Identifier: MIT
//! MCP adapter over `fp-automation`, served by the FreePlots executable.
//!
//! The server speaks newline-delimited JSON-RPC 2.0 on stdin/stdout (the MCP
//! stdio transport). Every tool drives the real FreePlots widgets through
//! AccessKit; no coordinates or screenshots are involved in the semantic
//! commands. Diagnostics go to stderr so stdout stays a clean protocol stream.

use std::io::{BufRead, Write};

use fp_automation::Automation;
use serde_json::{Value, json};

/// Runs the MCP server from command-line arguments.
///
/// `--http <addr>` serves JSON-RPC over HTTP; anything else uses stdio.
pub fn run(args: &[String]) {
    let mut http: Option<String> = None;
    let mut token: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--http" => {
                http = args.get(index + 1).cloned();
                index += 2;
            }
            "--token" => {
                token = args.get(index + 1).cloned();
                index += 2;
            }
            _ => index += 1,
        }
    }
    if let Some(address) = http {
        serve_http(&address, token);
    } else {
        serve_stdio();
    }
}

/// Serves MCP over stdio, forwarding the semantic tools to a live bridge.
///
/// The client runs this proxy locally; the editor runs on `host:port` (the
/// live bridge started with `FreePlots.exe --bridge`). Every tool executes in
/// the visible application, so the result appears on screen and in the undo
/// history.
///
/// # Errors
///
/// Returns the connection error when the bridge cannot be reached.
pub fn serve_live(host: &str, port: u16, token: Option<String>) -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                let _ = writeln!(
                    stdout,
                    "{}",
                    json!({"jsonrpc":"2.0","id":Value::Null,"error":{"code":-32700,"message":error.to_string()}})
                );
                let _ = stdout.flush();
                continue;
            }
        };
        if let Some(response) = handle_live(host, port, token.as_deref(), &request) {
            let _ = writeln!(stdout, "{response}");
            let _ = stdout.flush();
        }
    }
    Ok(())
}

fn handle_live(host: &str, port: u16, token: Option<&str>, request: &Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    if id.is_none() || method.starts_with("notifications/") {
        return None;
    }
    let id = id.unwrap_or(Value::Null);
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "freeplots-live", "version": env!("CARGO_PKG_VERSION")}
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": live_tools()})),
        "tools/call" => call_live(
            host,
            port,
            token,
            request.get("params").unwrap_or(&Value::Null),
        ),
        other => Err(format!("unknown method {other:?}")),
    };
    Some(match result {
        Ok(value) => json!({"jsonrpc": "2.0", "id": id, "result": value}),
        Err(message) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32602, "message": message}
        }),
    })
}

fn call_live(host: &str, port: u16, token: Option<&str>, params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or("tools/call needs a tool name")?;
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    let mut command = match name {
        "freeplots_describe" => json!({"cmd": "describe"}),
        "freeplots_probability" => json!({"cmd": "probability"}),
        "freeplots_capability" => json!({"cmd": "capability"}),
        "freeplots_controlchart" => json!({"cmd": "controlchart"}),
        "freeplots_regression" => json!({"cmd": "regression"}),
        "freeplots_anova" => json!({"cmd": "anova"}),
        "freeplots_normality" => json!({"cmd": "normality"}),
        "freeplots_open_csv" => json!({"cmd": "open_csv"}),
        "freeplots_session" => json!({"cmd": "session"}),
        "freeplots_status" => json!({"cmd": "status"}),
        "freeplots_screenshot" => json!({"cmd": "screenshot", "inline": true}),
        other => return Err(format!("unknown tool {other:?}")),
    };
    if let Value::Object(extra) = arguments
        && let Value::Object(map) = &mut command
    {
        for (key, value) in extra {
            map.insert(key, value);
        }
    }
    let reply = bridge_request(host, port, token, &command)?;
    if reply.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err(reply
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("the live application rejected the command")
            .to_owned());
    }
    let text = if let Some(session) = reply.get("session").and_then(Value::as_str) {
        session.to_owned()
    } else if let Some(status) = reply.get("status").and_then(Value::as_str) {
        status.to_owned()
    } else if let Some(png) = reply.get("png_base64").and_then(Value::as_str) {
        format!(
            "screenshot ({}x{}): {png}",
            reply.get("width").and_then(Value::as_u64).unwrap_or(0),
            reply.get("height").and_then(Value::as_u64).unwrap_or(0)
        )
    } else {
        reply.to_string()
    };
    Ok(json!({"content": [{"type": "text", "text": text}]}))
}

fn bridge_request(
    host: &str,
    port: u16,
    token: Option<&str>,
    command: &Value,
) -> Result<Value, String> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpStream;

    let mut stream = TcpStream::connect((host, port))
        .map_err(|error| format!("cannot reach the live bridge at {host}:{port}: {error}"))?;
    let mut payload = command.clone();
    if let Some(token) = token
        && let Value::Object(map) = &mut payload
    {
        map.insert("token".to_owned(), Value::String(token.to_owned()));
    }
    writeln!(stream, "{payload}").map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&line).map_err(|error| error.to_string())
}

fn live_tools() -> Vec<Value> {
    vec![
        json!({"name":"freeplots_describe","description":"Descriptive statistics of a column in the live worksheet.","inputSchema":{"type":"object","properties":{"column":{"type":"string"},"by":{"type":"string"}},"required":["column"]}}),
        json!({"name":"freeplots_probability","description":"Probability plot of a column in the live worksheet.","inputSchema":{"type":"object","properties":{"column":{"type":"string"},"distribution":{"type":"string"}},"required":["column"]}}),
        json!({"name":"freeplots_capability","description":"Process capability in the live worksheet.","inputSchema":{"type":"object","properties":{"column":{"type":"string"},"lsl":{"type":"number"},"usl":{"type":"number"},"target":{"type":"number"},"confidence":{"type":"number"},"distribution":{"type":"string"}},"required":["column"]}}),
        json!({"name":"freeplots_controlchart","description":"Control chart in the live worksheet.","inputSchema":{"type":"object","properties":{"column":{"type":"string"},"chart":{"type":"string"},"subgroup":{"type":"integer"},"subgroup_column":{"type":"string"}},"required":["column","chart"]}}),
        json!({"name":"freeplots_regression","description":"Least-squares regression in the live worksheet.","inputSchema":{"type":"object","properties":{"response":{"type":"string"},"predictors":{"type":"string"}},"required":["response","predictors"]}}),
        json!({"name":"freeplots_anova","description":"One-way ANOVA in the live worksheet.","inputSchema":{"type":"object","properties":{"response":{"type":"string"},"factor":{"type":"string"}},"required":["response","factor"]}}),
        json!({"name":"freeplots_normality","description":"Anderson-Darling normality test in the live worksheet.","inputSchema":{"type":"object","properties":{"column":{"type":"string"}},"required":["column"]}}),
        json!({"name":"freeplots_open_csv","description":"Open a CSV file in the live application.","inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}),
        json!({"name":"freeplots_session","description":"Read the live Session report.","inputSchema":{"type":"object","properties":{}}}),
        json!({"name":"freeplots_status","description":"Read the live status line.","inputSchema":{"type":"object","properties":{}}}),
        json!({"name":"freeplots_screenshot","description":"Screenshot of the live window (PNG, base64).","inputSchema":{"type":"object","properties":{}}}),
    ]
}

/// Serves JSON-RPC 2.0 over HTTP (one request per connection).
///
/// `POST /mcp` with a JSON-RPC body returns the JSON-RPC response; `GET /`
/// returns a small health document so a client can check the bridge.
pub fn serve_http(address: &str, token: Option<String>) {
    use std::io::{BufRead, BufReader, Read};
    use std::net::TcpListener;

    let public = !address.starts_with("127.0.0.1") && !address.starts_with("localhost");
    if !public_bind_allows(public, &token) {
        eprintln!("freeplots: refusing to listen on {address} without --token <secret>");
        std::process::exit(1);
    }
    let listener = match TcpListener::bind(address) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("freeplots: cannot bind {address}: {error}");
            std::process::exit(1);
        }
    };
    eprintln!("freeplots: listening on http://{address}/mcp");
    let mut automation = Automation::new();
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut reader = BufReader::new(match stream.try_clone() {
            Ok(clone) => clone,
            Err(_) => continue,
        });
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).is_err() {
            continue;
        }
        let mut length = 0usize;
        let mut authorization = String::new();
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header).is_err() || header == "\r\n" || header.is_empty() {
                break;
            }
            let lowered = header.to_ascii_lowercase();
            if let Some(value) = lowered.strip_prefix("content-length:") {
                length = value.trim().parse().unwrap_or(0);
            }
            if lowered.starts_with("authorization:") {
                authorization = header
                    .split_once(':')
                    .map(|(_, value)| value.trim().to_owned())
                    .unwrap_or_default();
            }
        }
        let authorised = match token.as_deref() {
            None => true,
            Some(expected) => bearer_authorises(&authorization, expected),
        };
        if !authorised {
            let body = "{\"error\":\"unauthorized\"}";
            let _ = write!(
                stream,
                "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.flush();
            continue;
        }
        let mut body = vec![0u8; length];
        if length > 0 && reader.read_exact(&mut body).is_err() {
            continue;
        }
        let response = if request_line.starts_with("GET ") {
            let tool_count = tools().len();
            json!({
                "status": "ok",
                "server": "freeplots",
                "tools": tool_count
            })
            .to_string()
        } else {
            match serde_json::from_slice::<Value>(&body) {
                Ok(request) => handle(&mut automation, &request)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "{}".to_owned()),
                Err(error) => json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {"code": -32700, "message": error.to_string()}
                })
                .to_string(),
            }
        };
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response.len(),
            response
        );
        let _ = stream.flush();
    }
}

pub fn serve_stdio() {
    let mut automation = Automation::new();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                let _ = writeln!(
                    stdout,
                    "{}",
                    json!({
                        "jsonrpc": "2.0",
                        "id": Value::Null,
                        "error": {"code": -32700, "message": error.to_string()}
                    })
                );
                let _ = stdout.flush();
                continue;
            }
        };
        let Some(response) = handle(&mut automation, &request) else {
            continue;
        };
        let _ = writeln!(stdout, "{response}");
        let _ = stdout.flush();
    }
}

fn handle(automation: &mut Automation, request: &Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    if id.is_none() || method.starts_with("notifications/") {
        return None;
    }
    let id = id.unwrap_or(Value::Null);
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "freeplots", "version": env!("CARGO_PKG_VERSION")}
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => call_tool(automation, request.get("params").unwrap_or(&Value::Null)),
        other => Err(format!("unknown method {other:?}")),
    };
    Some(match result {
        Ok(value) => json!({"jsonrpc": "2.0", "id": id, "result": value}),
        Err(message) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32602, "message": message}
        }),
    })
}

fn call_tool(automation: &mut Automation, params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or("tools/call needs a tool name")?;
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    match run_tool(automation, name, &arguments) {
        Ok(text) => Ok(json!({
            "content": [{"type": "text", "text": text}],
            "isError": false
        })),
        Err(message) => Ok(json!({
            "content": [{"type": "text", "text": message}],
            "isError": true
        })),
    }
}

fn run_tool(automation: &mut Automation, name: &str, arguments: &Value) -> Result<String, String> {
    match name {
        "freeplots_open_csv" => {
            let path = string_arg(arguments, "path")?;
            automation
                .load_csv(&path)
                .map_err(|error| error.to_string())?;
            Ok(format!("Loaded {path} ({}).", automation.status()))
        }
        "freeplots_menu" => {
            let path = string_list(arguments, "path")?;
            let borrowed: Vec<&str> = path.iter().map(String::as_str).collect();
            automation
                .open_menu(&borrowed)
                .map_err(|error| error.to_string())?;
            Ok(format!("Opened {}.", path.join(" > ")))
        }
        "freeplots_click" => {
            let label = string_arg(arguments, "label")?;
            automation
                .click(&label)
                .map_err(|error| error.to_string())?;
            Ok(format!("Clicked {label:?}."))
        }
        "freeplots_set_text" => {
            let label = string_arg(arguments, "label")?;
            let value = string_arg(arguments, "value")?;
            automation
                .set_text(&label, &value)
                .map_err(|error| error.to_string())?;
            Ok(format!("Set {label:?} to {value:?}."))
        }
        "freeplots_select" => {
            let label = string_arg(arguments, "label")?;
            let option = string_arg(arguments, "option")?;
            automation
                .select(&label, &option)
                .map_err(|error| error.to_string())?;
            Ok(format!("Selected {option:?} in {label:?}."))
        }
        "freeplots_press_key" => {
            let name = string_arg(arguments, "key")?;
            let key = match name.to_ascii_lowercase().as_str() {
                "escape" | "esc" => egui::Key::Escape,
                "enter" | "return" => egui::Key::Enter,
                "tab" => egui::Key::Tab,
                other => return Err(format!("unsupported key {other:?}")),
            };
            automation.press_key(key);
            Ok(format!("Pressed {name}."))
        }
        "freeplots_windows" => Ok(automation.window_titles().join("\n")),
        "freeplots_session" => Ok(automation.session().to_owned()),
        "freeplots_status" => Ok(automation.status().to_owned()),
        "freeplots_widgets" => {
            serde_json::to_string_pretty(&automation.widgets()).map_err(|error| error.to_string())
        }
        "freeplots_unlabeled" => serde_json::to_string_pretty(&automation.unlabeled_widgets())
            .map_err(|error| error.to_string()),
        "freeplots_screenshot" => {
            let path = string_arg(arguments, "path")?;
            automation
                .screenshot(&path)
                .map_err(|error| error.to_string())?;
            Ok(format!("Saved the current frame to {path}."))
        }
        "freeplots_plot" => Ok(plot_summary(automation)),
        "freeplots_run_script" => run_script(automation, arguments),
        other => Err(format!("unknown tool {other:?}")),
    }
}

fn run_script(automation: &mut Automation, arguments: &Value) -> Result<String, String> {
    let steps = arguments
        .get("steps")
        .and_then(Value::as_array)
        .ok_or("freeplots_run_script needs a steps array")?;
    let mut log = Vec::with_capacity(steps.len());
    for (index, step) in steps.iter().enumerate() {
        let action = step
            .get("action")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("step {} has no action", index + 1))?;
        let outcome = match action {
            "menu" => {
                let path = string_list(step, "path")?;
                let borrowed: Vec<&str> = path.iter().map(String::as_str).collect();
                automation
                    .open_menu(&borrowed)
                    .map(|()| format!("menu {}", path.join(" > ")))
            }
            "click" => {
                let label = string_arg(step, "label")?;
                automation
                    .click(&label)
                    .map(|()| format!("click {label:?}"))
            }
            "set_text" => {
                let label = string_arg(step, "label")?;
                let value = string_arg(step, "value")?;
                automation
                    .set_text(&label, &value)
                    .map(|()| format!("set {label:?} = {value:?}"))
            }
            "select" => {
                let label = string_arg(step, "label")?;
                let option = string_arg(step, "option")?;
                automation
                    .select(&label, &option)
                    .map(|()| format!("select {option:?} in {label:?}"))
            }
            other => Err(fp_automation::Error::from(format!(
                "unknown action {other:?}"
            ))),
        };
        match outcome {
            Ok(description) => log.push(format!("{}. {description}", index + 1)),
            Err(error) => {
                log.push(format!("{}. FAILED: {error}", index + 1));
                return Err(log.join("\n"));
            }
        }
    }
    log.push(format!("status: {}", automation.status()));
    Ok(log.join("\n"))
}

fn plot_summary(automation: &Automation) -> String {
    let Some(plot) = automation.plot() else {
        return "No plot in the current session.".to_owned();
    };
    let panels = plot.panels.len();
    format!(
        "title: {}\nx: {} ({:?})\ny: {} ({:?})\npoints: {}\nlines: {}\nbands: {}\nseries: {}\npanels: {panels}",
        plot.title,
        plot.x_axis.label,
        plot.x_axis.scale,
        plot.y_axis.label,
        plot.y_axis.scale,
        plot.points.len(),
        plot.lines.len(),
        plot.bands.len(),
        plot.series.len(),
    )
}

fn string_arg(arguments: &Value, name: &str) -> Result<String, String> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("missing string argument {name:?}"))
}

fn string_list(arguments: &Value, name: &str) -> Result<Vec<String>, String> {
    arguments
        .get(name)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("missing array argument {name:?}"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{name:?} must contain strings"))
        })
        .collect()
}

fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": "freeplots_open_csv",
            "description": "Open a CSV file in the FreePlots worksheet.",
            "inputSchema": {
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"]
            }
        }),
        json!({
            "name": "freeplots_menu",
            "description": "Click a menu path, e.g. [\"Stat\", \"Control Charts\", \"P/NP/C/U Chart…\"]. Submenu arrows are handled automatically.",
            "inputSchema": {
                "type": "object",
                "properties": {"path": {"type": "array", "items": {"type": "string"}}},
                "required": ["path"]
            }
        }),
        json!({
            "name": "freeplots_click",
            "description": "Click the first visible widget with the exact accessible label (button, menu item, check box…).",
            "inputSchema": {
                "type": "object",
                "properties": {"label": {"type": "string"}},
                "required": ["label"]
            }
        }),
        json!({
            "name": "freeplots_set_text",
            "description": "Replace the text of the field with the given label (click, select all, type).",
            "inputSchema": {
                "type": "object",
                "properties": {"label": {"type": "string"}, "value": {"type": "string"}},
                "required": ["label", "value"]
            }
        }),
        json!({
            "name": "freeplots_select",
            "description": "Open the combo box with the given label and choose an option by its exact label.",
            "inputSchema": {
                "type": "object",
                "properties": {"label": {"type": "string"}, "option": {"type": "string"}},
                "required": ["label", "option"]
            }
        }),
        json!({
            "name": "freeplots_press_key",
            "description": "Press a key: escape (closes the focused dialog), enter or tab.",
            "inputSchema": {
                "type": "object",
                "properties": {"key": {"type": "string", "enum": ["escape", "enter", "tab"]}},
                "required": ["key"]
            }
        }),
        json!({
            "name": "freeplots_windows",
            "description": "List the titles of the open dialog windows.",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "freeplots_session",
            "description": "Return the Session report text.",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "freeplots_status",
            "description": "Return the status-bar text (errors appear here).",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "freeplots_widgets",
            "description": "List the visible widgets with role, label, value and bounds. Use it to discover the labels of the current dialog.",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "freeplots_plot",
            "description": "Summarise the current plot (title, axes, points, lines, panels).",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "freeplots_unlabeled",
            "description": "List the interactive widgets of the current frame that have no accessible label (the labelling checklist).",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "freeplots_screenshot",
            "description": "Render the current frame to a PNG. The shipped build uses the live window bridge; the headless renderer needs the headless-screenshots feature.",
            "inputSchema": {
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"]
            }
        }),
        json!({
            "name": "freeplots_run_script",
            "description": "Run a sequence of menu/click/set_text/select steps; stops at the first failure and returns the log.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "action": {"type": "string", "enum": ["menu", "click", "set_text", "select"]},
                                "path": {"type": "array", "items": {"type": "string"}},
                                "label": {"type": "string"},
                                "value": {"type": "string"},
                                "option": {"type": "string"}
                            },
                            "required": ["action"]
                        }
                    }
                },
                "required": ["steps"]
            }
        }),
    ]
}

/// Whether a public bind is allowed: only with a non-empty token.
fn public_bind_allows(public: bool, token: &Option<String>) -> bool {
    !public || token.as_deref().is_some_and(|token| !token.is_empty())
}

/// Checks an `Authorization: Bearer <token>` header against the secret.
fn bearer_authorises(authorization: &str, expected: &str) -> bool {
    authorization
        .strip_prefix("Bearer ")
        .is_some_and(|given| constant_time_eq(given.trim(), expected))
}

/// Compares two secrets without an early exit.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> String {
        format!(
            "{}/../fp-automation/tests/data/pchart.csv",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    #[test]
    fn the_token_gate_is_exact() {
        assert!(bearer_authorises("Bearer abc123", "abc123"));
        assert!(bearer_authorises("Bearer   abc123  ", "abc123"));
        assert!(!bearer_authorises("Bearer abc12", "abc123"));
        assert!(!bearer_authorises("Bearer abc124", "abc123"));
        assert!(!bearer_authorises("bearer abc123", "abc123"));
        assert!(!bearer_authorises("Basic abc123", "abc123"));
        assert!(!bearer_authorises("", "abc123"));
        assert!(constant_time_eq("secret", "secret"));
        assert!(!constant_time_eq("secre", "secret"));
        assert!(!constant_time_eq("secret", "secreT"));
        assert!(public_bind_allows(false, &None));
        assert!(!public_bind_allows(true, &None));
        assert!(!public_bind_allows(true, &Some(String::new())));
        assert!(public_bind_allows(true, &Some("x".to_owned())));
    }

    #[test]
    fn the_tool_list_covers_the_semantic_surface() {
        let names: Vec<String> = tools()
            .iter()
            .map(|tool| tool["name"].as_str().expect("name").to_owned())
            .collect();
        for expected in [
            "freeplots_open_csv",
            "freeplots_menu",
            "freeplots_click",
            "freeplots_set_text",
            "freeplots_select",
            "freeplots_session",
            "freeplots_status",
            "freeplots_widgets",
            "freeplots_plot",
            "freeplots_screenshot",
            "freeplots_run_script",
        ] {
            assert!(names.iter().any(|name| name == expected), "{names:?}");
        }
    }

    #[test]
    fn a_script_drives_the_attribute_dialog() {
        let mut automation = Automation::new();
        automation.load_csv(fixture()).expect("load csv");
        let log = run_script(
            &mut automation,
            &json!({
                "steps": [
                    {"action": "menu", "path": ["Stat", "Control Charts", "P/NP/C/U Chart…"]},
                    {"action": "select", "label": "Chart:", "option": "P"},
                    {"action": "select", "label": "Defects column:", "option": "defects"},
                    {"action": "set_text", "label": "Constant size:", "value": "50"},
                    {"action": "click", "label": "Run"}
                ]
            }),
        )
        .expect("script");
        assert!(log.contains("1. menu Stat > Control Charts"), "{log}");
        assert!(
            automation.session().contains("P Chart"),
            "{}",
            automation.session()
        );
        assert!(automation.plot().is_some());
    }

    #[test]
    fn a_missing_widget_is_reported_without_killing_the_server() {
        let mut automation = Automation::new();
        let error = run_tool(
            &mut automation,
            "freeplots_click",
            &json!({"label": "No such button"}),
        )
        .expect_err("missing label");
        assert!(error.contains("no visible widget"), "{error}");
        // The session is still usable after the failed call.
        assert!(!automation.status().is_empty());
    }

    #[cfg(feature = "headless-screenshots")]
    #[test]
    fn the_screenshot_tool_writes_a_png() {
        let mut automation = Automation::new();
        let path = std::env::temp_dir().join(format!("freeplots-{}.png", std::process::id()));
        run_tool(
            &mut automation,
            "freeplots_screenshot",
            &json!({"path": path.display().to_string()}),
        )
        .expect("screenshot");
        let bytes = std::fs::read(&path).expect("read png");
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
        let _ = std::fs::remove_file(&path);
    }
}
