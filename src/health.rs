use std::collections::HashMap;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::types::{HealthResult, HealthStatus, McpServer, Transport};

const TIMEOUT: Duration = Duration::from_secs(5);

const INITIALIZE_MSG: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-05","capabilities":{},"clientInfo":{"name":"mcpm","version":"1.1.0"}}}"#;

/// Run a health check synchronously. Returns the HealthResult.
pub fn check_server(index: usize, server: &McpServer) -> HealthResult {
    let status = match &server.transport {
        Transport::Stdio { command, args } => check_stdio(command, args, &server.env),
        _ => HealthStatus::Error("health check only supports stdio servers".to_string()),
    };
    HealthResult {
        server_index: index,
        status,
        checked_at: Instant::now(),
    }
}

/// Spawn a health check in a background thread, sending result on tx.
pub fn spawn_health_check(
    index: usize,
    server: &McpServer,
    tx: mpsc::Sender<HealthResult>,
) {
    let transport = server.transport.clone();
    let env = server.env.clone();
    std::thread::spawn(move || {
        let status = match &transport {
            Transport::Stdio { command, args } => check_stdio(command, args, &env),
            _ => HealthStatus::Error("health check only supports stdio servers".to_string()),
        };
        let _ = tx.send(HealthResult {
            server_index: index,
            status,
            checked_at: Instant::now(),
        });
    });
}

fn check_stdio(
    command: &str,
    args: &[String],
    env: &Option<HashMap<String, String>>,
) -> HealthStatus {
    // Spawn the server process
    let mut cmd = Command::new(command);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(env_map) = env {
        cmd.envs(env_map);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return if e.kind() == std::io::ErrorKind::NotFound {
                HealthStatus::Error(format!("command not found: {}", command))
            } else {
                HealthStatus::Error(e.to_string())
            };
        }
    };

    // Write initialize message to stdin
    let _stdin_handle = child.stdin.take().and_then(|mut stdin| {
        // Send bare JSON with trailing newline — this is the most compatible
        // format. Content-Length framing can cause issues with some SDK
        // implementations that use line-based stdin readers.
        let msg = format!("{}\n", INITIALIZE_MSG);
        let _ = stdin.write_all(msg.as_bytes());
        let _ = stdin.flush();
        // Keep stdin alive — dropping it sends EOF which causes many MCP
        // servers (e.g. @modelcontextprotocol/sdk) to shut down immediately.
        Some(stdin)
    });

    // Read stdout with timeout
    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return HealthStatus::Error("failed to capture stdout".to_string());
        }
    };

    let (read_tx, read_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut stdout = stdout;
        let mut buf = vec![0u8; 8192];
        let mut output = Vec::new();
        loop {
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    output.extend_from_slice(&buf[..n]);
                    // Check if we have a complete JSON response yet
                    if let Some(status) = try_parse_response(&output) {
                        let _ = read_tx.send(Ok(status));
                        return;
                    }
                }
                Err(e) => {
                    let _ = read_tx.send(Err(e.to_string()));
                    return;
                }
            }
        }
        // EOF reached — try to parse whatever we got
        match try_parse_response(&output) {
            Some(status) => {
                let _ = read_tx.send(Ok(status));
            }
            None if output.is_empty() => {
                let _ = read_tx.send(Err("no response from server".to_string()));
            }
            None => {
                let preview = String::from_utf8_lossy(&output[..output.len().min(200)]);
                let _ = read_tx.send(Err(format!("invalid response: {}", preview)));
            }
        }
    });

    let result = match read_rx.recv_timeout(TIMEOUT) {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => HealthStatus::Error(e),
        Err(_) => HealthStatus::Timeout,
    };

    let _ = child.kill();
    let _ = child.wait();

    result
}

/// Try to extract a valid initialize response from the accumulated output.
/// Handles both bare JSON and Content-Length framed responses.
fn try_parse_response(data: &[u8]) -> Option<HealthStatus> {
    let text = std::str::from_utf8(data).ok()?;

    // Try to find JSON in the output — skip any Content-Length headers
    let json_start = text.find('{')?;
    let json_text = &text[json_start..];

    // Try to parse as JSON
    let val: serde_json::Value = serde_json::from_str(json_text).ok()?;

    // Check for JSON-RPC response with result
    if val.get("result").is_some() {
        let server_name = val["result"]["serverInfo"]["name"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let server_version = val["result"]["serverInfo"]["version"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        Some(HealthStatus::Healthy {
            server_name,
            server_version,
        })
    } else if let Some(err) = val.get("error") {
        let msg = err["message"].as_str().unwrap_or("unknown error");
        Some(HealthStatus::Error(format!("server error: {}", msg)))
    } else {
        None
    }
}
