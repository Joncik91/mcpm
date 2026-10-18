//! Stdio health checks: spawn the server, send JSON-RPC `initialize`, wait for
//! the matching response. Runs on plain threads (no async runtime).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::shell;
use crate::types::{Ctx, HealthResult, HealthStatus, McpServer, Transport};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// A published MCP spec revision; servers negotiate down if they need to.
const PROTOCOL_VERSION: &str = "2025-06-18";
/// Give up on servers that flood stdout without ever answering.
const MAX_STDOUT: usize = 1 << 20;
const STDERR_TAIL: usize = 4096;

/// Run a health check synchronously.
pub fn check(server: &McpServer, ctx: &Ctx, timeout: Duration) -> HealthStatus {
    match &server.transport {
        Transport::Stdio { command, args } => {
            let probe = Probe::new(command, args, &server.env, ctx);
            check_stdio(&probe, timeout)
        }
        _ => HealthStatus::Error("health checks support stdio servers only".to_string()),
    }
}

/// Run a health check on a background thread, sending the result on `tx`.
pub fn spawn_check(
    server: &McpServer,
    ctx: &Ctx,
    timeout: Duration,
    tx: mpsc::Sender<HealthResult>,
) {
    let server = server.clone();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let status = check(&server, &ctx, timeout);
        let _ = tx.send(HealthResult {
            id: server.id(),
            status,
            checked_at: Instant::now(),
        });
    });
}

/// The exact process to launch, after `${VAR}` expansion as MCP clients do it.
struct Probe {
    command: String,
    args: Vec<String>,
    env: BTreeMap<String, String>,
}

impl Probe {
    fn new(command: &str, args: &[String], env: &BTreeMap<String, String>, ctx: &Ctx) -> Probe {
        let lookup = |name: &str| -> Option<String> {
            match name {
                "workspaceFolder" => Some(ctx.cwd.to_string_lossy().into_owned()),
                "userHome" => ctx.home.as_ref().map(|h| h.to_string_lossy().into_owned()),
                _ => std::env::var(name.strip_prefix("env:").unwrap_or(name)).ok(),
            }
        };
        let x = |s: &str| shell::expand_vars(s, &lookup);
        Probe {
            command: x(command),
            args: args.iter().map(|a| x(a)).collect(),
            env: env.iter().map(|(k, v)| (k.clone(), x(v))).collect(),
        }
    }
}

fn initialize_msg() -> String {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "mcpm", "version": env!("CARGO_PKG_VERSION")}
        }
    })
    .to_string()
}

enum ReadEnd {
    Eof,
    Failed(String),
}

fn check_stdio(probe: &Probe, timeout: Duration) -> HealthStatus {
    if probe.command.trim().is_empty() {
        return HealthStatus::Error("no command configured".to_string());
    }
    let mut child = match spawn(probe) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return HealthStatus::Error(format!("command not found: {}", probe.command));
        }
        Err(e) => return HealthStatus::Error(format!("failed to start {}: {}", probe.command, e)),
    };

    // MCP stdio is newline-delimited JSON. Keep stdin open until we're done:
    // EOF makes many servers exit before answering.
    let mut stdin = child.stdin.take();
    if let Some(w) = stdin.as_mut() {
        let _ = w.write_all(format!("{}\n", initialize_msg()).as_bytes());
        let _ = w.flush();
    }

    let stderr_tail = Arc::new(Mutex::new(Vec::new()));
    if let Some(mut err) = child.stderr.take() {
        let tail = Arc::clone(&stderr_tail);
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = err.read(&mut buf) {
                if n == 0 {
                    break;
                }
                if let Ok(mut t) = tail.lock() {
                    t.extend_from_slice(&buf[..n]);
                    let excess = t.len().saturating_sub(STDERR_TAIL);
                    t.drain(..excess);
                }
            }
        });
    }

    let (tx, rx) = mpsc::channel();
    if let Some(mut out) = child.stdout.take() {
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            let mut output = Vec::new();
            loop {
                match out.read(&mut buf) {
                    Ok(0) => {
                        let _ = tx.send(Err(ReadEnd::Eof));
                        return;
                    }
                    Ok(n) => {
                        output.extend_from_slice(&buf[..n]);
                        if let Some(status) = scan(&output) {
                            let _ = tx.send(Ok(status));
                            return;
                        }
                        if output.len() > MAX_STDOUT {
                            let _ = tx.send(Err(ReadEnd::Failed(
                                "over 1 MB of output without an initialize response".into(),
                            )));
                            return;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(ReadEnd::Failed(e.to_string())));
                        return;
                    }
                }
            }
        });
    }

    let status = match rx.recv_timeout(timeout) {
        Ok(Ok(status)) => status,
        Ok(Err(ReadEnd::Failed(e))) => HealthStatus::Error(e),
        Ok(Err(ReadEnd::Eof)) | Err(mpsc::RecvTimeoutError::Disconnected) => {
            let exit = wait_briefly(&mut child, Duration::from_millis(500));
            // Give the stderr reader a moment to collect the last words.
            std::thread::sleep(Duration::from_millis(50));
            let mut msg = match exit {
                Some(code) => format!("exited ({}) without responding", code),
                None => "closed stdout without responding".to_string(),
            };
            if let Some(line) = last_line(&stderr_tail) {
                msg.push_str(": ");
                msg.push_str(&line);
            }
            HealthStatus::Error(msg)
        }
        Err(mpsc::RecvTimeoutError::Timeout) => HealthStatus::Timeout(timeout),
    };

    drop(stdin);
    terminate(&mut child);
    status
}

fn spawn(probe: &Probe) -> std::io::Result<Child> {
    let build = |program: &str, pre: &[&str]| {
        let mut cmd = Command::new(program);
        cmd.args(pre)
            .args(&probe.args)
            .envs(&probe.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Own process group, so `npx` → `node` grandchildren die with it.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        cmd
    };
    match build(&probe.command, &[]).spawn() {
        // Windows only resolves `.exe` on PATH; `npx` & co. are `.cmd` shims.
        Err(e)
            if cfg!(windows)
                && e.kind() == std::io::ErrorKind::NotFound
                && std::path::Path::new(&probe.command).extension().is_none() =>
        {
            build("cmd", &["/C", &probe.command]).spawn()
        }
        other => other,
    }
}

fn terminate(child: &mut Child) {
    #[cfg(unix)]
    if let Ok(pid) = i32::try_from(child.id()) {
        // SAFETY: plain syscall; negative pid targets the process group we
        // created at spawn. The child is not yet reaped, so the id is ours.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn wait_briefly(child: &mut Child, max: Duration) -> Option<String> {
    let deadline = Instant::now() + max;
    while Instant::now() < deadline {
        if let Ok(Some(st)) = child.try_wait() {
            return Some(match st.code() {
                Some(c) => format!("exit code {}", c),
                None => st.to_string(),
            });
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

fn last_line(tail: &Mutex<Vec<u8>>) -> Option<String> {
    let t = tail.lock().ok()?;
    let text = String::from_utf8_lossy(&t);
    let line = text.lines().rev().map(str::trim).find(|l| !l.is_empty())?;
    let mut s: String = line.chars().take(160).collect();
    if line.chars().count() > 160 {
        s.push('…');
    }
    Some(s)
}

/// Scan accumulated stdout for the response to our `initialize` (id 1).
/// Tolerates log lines, notifications and server requests before it, and both
/// newline-delimited and `Content-Length`-framed messages. `None` = need more.
fn scan(buf: &[u8]) -> Option<HealthStatus> {
    let mut i = 0;
    loop {
        while i < buf.len() && buf[i].is_ascii_whitespace() {
            i += 1;
        }
        let rest = &buf[i..];
        if rest.is_empty() {
            return None;
        }
        if rest.len() >= 15 && rest[..15].eq_ignore_ascii_case(b"content-length:") {
            let (hdr_len, sep) = find(rest, b"\r\n\r\n")
                .map(|p| (p, 4))
                .or_else(|| find(rest, b"\n\n").map(|p| (p, 2)))?;
            let len = String::from_utf8_lossy(&rest[15..hdr_len])
                .lines()
                .next()
                .and_then(|v| v.trim().parse::<usize>().ok());
            if let Some(n) = len {
                let body = hdr_len + sep;
                let msg = rest.get(body..body + n)?;
                if let Some(s) = serde_json::from_slice(msg)
                    .ok()
                    .as_ref()
                    .and_then(as_response)
                {
                    return Some(s);
                }
                i += body + n;
                continue;
            }
        } else if rest[0] == b'{' {
            let mut stream = serde_json::Deserializer::from_slice(rest).into_iter::<Value>();
            match stream.next() {
                Some(Ok(v)) => {
                    if let Some(s) = as_response(&v) {
                        return Some(s);
                    }
                    i += stream.byte_offset();
                    continue;
                }
                Some(Err(e)) if e.is_eof() => return None,
                _ => {}
            }
        }
        // Not a message (log output, garbage): skip the line.
        i += find(rest, b"\n")? + 1;
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn as_response(v: &Value) -> Option<HealthStatus> {
    if v.get("id") != Some(&json!(1)) {
        return None;
    }
    if let Some(result) = v.get("result") {
        let s = |p: &str| {
            result
                .pointer(p)
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        return Some(HealthStatus::Healthy {
            server_name: s("/serverInfo/name").unwrap_or_else(|| "unknown".into()),
            server_version: s("/serverInfo/version").unwrap_or_else(|| "unknown".into()),
            protocol_version: s("/protocolVersion"),
        });
    }
    let err = v.get("error")?;
    let msg = err
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("unknown error");
    Some(HealthStatus::Error(
        match err.get("code").and_then(Value::as_i64) {
            Some(code) => format!("server error {}: {}", code, msg),
            None => format!("server error: {}", msg),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","serverInfo":{"name":"fake","version":"1.0"}}}"#;

    fn healthy(name: &str) -> HealthStatus {
        HealthStatus::Healthy {
            server_name: name.into(),
            server_version: "1.0".into(),
            protocol_version: Some("2025-06-18".into()),
        }
    }

    #[test]
    fn scan_skips_noise_before_response() {
        let out = format!(
            "Starting server...\n{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/message\"}}\n{{bad json}}\n{{\"id\":2,\"result\":{{}}}}\n{}\n",
            OK
        );
        assert_eq!(scan(out.as_bytes()), Some(healthy("fake")));
    }

    #[test]
    fn scan_waits_for_partial_messages() {
        assert_eq!(scan(&OK.as_bytes()[..40]), None);
        assert_eq!(scan(b"log line without newline"), None);
    }

    #[test]
    fn scan_handles_content_length_framing() {
        let framed = format!("Content-Length: {}\r\n\r\n{}", OK.len(), OK);
        assert_eq!(scan(framed.as_bytes()), Some(healthy("fake")));
        assert_eq!(scan(&framed.as_bytes()[..framed.len() - 3]), None);
    }

    #[test]
    fn scan_reports_error_response() {
        let e = br#"{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"bad version"}}"#;
        assert_eq!(
            scan(e),
            Some(HealthStatus::Error(
                "server error -32602: bad version".into()
            ))
        );
    }

    #[cfg(unix)]
    mod process {
        use super::*;
        use crate::testutil::TempDir;

        fn sh_server(script: &str) -> McpServer {
            McpServer {
                name: "t".into(),
                client: crate::types::ClientKind::ClaudeCodeProject,
                source_path: "/x".into(),
                transport: Transport::Stdio {
                    command: "sh".into(),
                    args: vec![
                        "-c".into(),
                        script.into(),
                        "sh".into(),
                        "${MCPM_UNSET_VAR:-expanded}".into(),
                    ],
                },
                env: BTreeMap::new(),
                disabled: false,
                raw: Value::Null,
            }
        }

        #[test]
        fn healthy_server_with_var_expansion() {
            let t = TempDir::new();
            let script = r#"read line; echo "booting"; printf '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","serverInfo":{"name":"%s","version":"1.0"}}}\n' "$1"; sleep 30"#;
            let start = Instant::now();
            assert_eq!(
                check(&sh_server(script), &t.ctx(), DEFAULT_TIMEOUT),
                healthy("expanded")
            );
            assert!(
                start.elapsed() < Duration::from_secs(3),
                "must not wait for the server to exit"
            );
        }

        #[test]
        fn early_exit_reports_stderr() {
            let t = TempDir::new();
            let status = check(
                &sh_server("echo 'missing API key' >&2; exit 3"),
                &t.ctx(),
                DEFAULT_TIMEOUT,
            );
            let HealthStatus::Error(msg) = status else {
                panic!("{:?}", status)
            };
            assert!(
                msg.contains("exit code 3") && msg.contains("missing API key"),
                "{}",
                msg
            );
        }

        #[test]
        fn timeout_kills_whole_process_group() {
            let t = TempDir::new();
            let pidfile = t.home().join("pid");
            let script = format!(
                "sleep 300 & echo $! > '{}'; cat > /dev/null",
                pidfile.display()
            );
            let status = check(&sh_server(&script), &t.ctx(), Duration::from_millis(400));
            assert_eq!(status, HealthStatus::Timeout(Duration::from_millis(400)));
            let pid: i32 = std::fs::read_to_string(&pidfile)
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            std::thread::sleep(Duration::from_millis(100));
            // Killed-but-unreaped (zombie, state Z) counts as dead: some
            // containers' PID 1 never reaps orphans.
            let alive = std::fs::read_to_string(format!("/proc/{}/stat", pid))
                .map(|st| {
                    st.rsplit(')')
                        .next()
                        .is_some_and(|r| !r.trim_start().starts_with('Z'))
                })
                .unwrap_or(false);
            assert!(!alive, "grandchild {} leaked", pid);
        }

        #[test]
        fn missing_command() {
            let t = TempDir::new();
            let mut s = sh_server("");
            s.transport = Transport::Stdio {
                command: "mcpm-no-such-binary".into(),
                args: vec![],
            };
            assert_eq!(
                check(&s, &t.ctx(), DEFAULT_TIMEOUT),
                HealthStatus::Error("command not found: mcpm-no-such-binary".into())
            );
        }
    }
}
