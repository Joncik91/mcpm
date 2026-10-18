use std::collections::BTreeMap;

use serde_json::Value;

use crate::config_writer;
use crate::shell;
use crate::types::{ClientKind, McpServer, Transport};

// ---------------------------------------------------------------------------
// Mode — top-level modal state
// ---------------------------------------------------------------------------

#[derive(Default)]
pub enum Mode {
    #[default]
    Normal,
    AddWizard(AddWizard),
    RemoveConfirm(RemoveConfirm),
    SyncSelect(SyncSelect),
    Help,
    Errors {
        scroll: u16,
    },
}

// ---------------------------------------------------------------------------
// Checklist — shared multi-select used by every modal
// ---------------------------------------------------------------------------

pub struct Checklist<T> {
    pub items: Vec<(T, bool)>,
    pub cursor: usize,
}

impl<T: Clone> Checklist<T> {
    pub fn new(items: impl IntoIterator<Item = (T, bool)>) -> Self {
        Checklist {
            items: items.into_iter().collect(),
            cursor: 0,
        }
    }

    pub fn toggle(&mut self) {
        if let Some((_, sel)) = self.items.get_mut(self.cursor) {
            *sel = !*sel;
        }
    }

    pub fn toggle_all(&mut self) {
        let all = self.items.iter().all(|(_, s)| *s);
        self.items.iter_mut().for_each(|(_, s)| *s = !all);
    }

    pub fn up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn down(&mut self) {
        if self.cursor + 1 < self.items.len() {
            self.cursor += 1;
        }
    }

    pub fn selected(&self) -> Vec<T> {
        self.items
            .iter()
            .filter(|(_, s)| *s)
            .map(|(t, _)| t.clone())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Add Server Wizard
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    Stdio,
    Http,
    Sse,
}

impl TransportKind {
    pub const ALL: [TransportKind; 3] = [
        TransportKind::Stdio,
        TransportKind::Http,
        TransportKind::Sse,
    ];

    pub fn describe(&self) -> &'static str {
        match self {
            TransportKind::Stdio => "stdio  (local command)",
            TransportKind::Http => "http   (streamable HTTP URL)",
            TransportKind::Sse => "sse    (legacy SSE URL)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddStep {
    Name,
    TransportType,
    Command,
    Args,
    Url,
    Headers,
    EnvVars,
    Clients,
    Confirm,
}

pub struct AddWizard {
    pub step: AddStep,
    pub name: String,
    pub transport: TransportKind,
    pub command: String,
    pub args: String,
    pub url: String,
    /// Input line for the Headers / EnvVars list steps
    pub pair_input: String,
    pub headers: BTreeMap<String, String>,
    pub env: BTreeMap<String, String>,
    pub clients: Checklist<ClientKind>,
    pub error: Option<String>,
    /// Selected clients that already have a server of this name in the exact
    /// scope the write targets. Computed when entering Confirm.
    pub overwrite_clients: Vec<ClientKind>,
}

impl AddWizard {
    /// `targets` are the clients that can currently accept a new server.
    pub fn new(targets: &[ClientKind]) -> Self {
        let any_project = targets.contains(&ClientKind::ClaudeCodeProject);
        AddWizard {
            step: AddStep::Name,
            name: String::new(),
            transport: TransportKind::Stdio,
            command: String::new(),
            args: String::new(),
            url: String::new(),
            pair_input: String::new(),
            headers: BTreeMap::new(),
            env: BTreeMap::new(),
            clients: Checklist::new(targets.iter().map(|c| {
                let on = if any_project {
                    *c == ClientKind::ClaudeCodeProject
                } else {
                    false
                };
                (*c, on)
            })),
            error: None,
            overwrite_clients: Vec::new(),
        }
    }

    /// Mutable text buffer of the active step, if it takes text input.
    pub fn input_mut(&mut self) -> Option<&mut String> {
        match self.step {
            AddStep::Name => Some(&mut self.name),
            AddStep::Command => Some(&mut self.command),
            AddStep::Args => Some(&mut self.args),
            AddStep::Url => Some(&mut self.url),
            AddStep::Headers | AddStep::EnvVars => Some(&mut self.pair_input),
            _ => None,
        }
    }

    pub fn input(&self) -> &str {
        match self.step {
            AddStep::Name => &self.name,
            AddStep::Command => &self.command,
            AddStep::Args => &self.args,
            AddStep::Url => &self.url,
            AddStep::Headers | AddStep::EnvVars => &self.pair_input,
            _ => "",
        }
    }

    pub fn is_text_step(&self) -> bool {
        matches!(
            self.step,
            AddStep::Name
                | AddStep::Command
                | AddStep::Args
                | AddStep::Url
                | AddStep::Headers
                | AddStep::EnvVars
        )
    }

    pub fn insert_str(&mut self, s: &str) {
        self.error = None;
        // Single-line inputs: control characters (e.g. pasted newlines) become spaces.
        let clean: String = s
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        if let Some(buf) = self.input_mut() {
            buf.push_str(&clean);
        }
    }

    /// Backspace. On an empty list-step input, pulls the last entry back for editing.
    pub fn backspace(&mut self) {
        self.error = None;
        if matches!(self.step, AddStep::Headers | AddStep::EnvVars) && self.pair_input.is_empty() {
            let (map, sep) = if self.step == AddStep::Headers {
                (&mut self.headers, ": ")
            } else {
                (&mut self.env, "=")
            };
            if let Some((k, v)) = map.pop_last() {
                self.pair_input = format!("{}{}{}", k, sep, v);
            }
            return;
        }
        if let Some(buf) = self.input_mut() {
            buf.pop();
        }
    }

    pub fn clear_input(&mut self) {
        if let Some(buf) = self.input_mut() {
            buf.clear();
        }
    }

    pub fn delete_word(&mut self) {
        if let Some(buf) = self.input_mut() {
            let trimmed = buf.trim_end().len();
            buf.truncate(trimmed);
            let cut = buf.rfind(char::is_whitespace).map_or(0, |i| i + 1);
            buf.truncate(cut);
        }
    }

    pub fn transport_up(&mut self) {
        let i = TransportKind::ALL
            .iter()
            .position(|t| *t == self.transport)
            .unwrap_or(0);
        self.transport = TransportKind::ALL[i.saturating_sub(1)];
    }

    pub fn transport_down(&mut self) {
        let i = TransportKind::ALL
            .iter()
            .position(|t| *t == self.transport)
            .unwrap_or(0);
        self.transport = TransportKind::ALL[(i + 1).min(TransportKind::ALL.len() - 1)];
    }

    /// Advance to the next step. Returns true if validation passed.
    pub fn advance(&mut self) -> bool {
        match self.try_advance() {
            Ok(()) => {
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }

    fn try_advance(&mut self) -> Result<(), String> {
        self.step = match self.step {
            AddStep::Name => {
                let name = self.name.trim();
                if name.is_empty() {
                    return Err("Server name cannot be empty".into());
                }
                if name.chars().any(char::is_whitespace) {
                    return Err("Server name cannot contain spaces".into());
                }
                AddStep::TransportType
            }
            AddStep::TransportType => match self.transport {
                TransportKind::Stdio => AddStep::Command,
                _ => AddStep::Url,
            },
            AddStep::Command => {
                let words = shell::split(&self.command)?;
                let Some((cmd, rest)) = words.split_first() else {
                    return Err("Command cannot be empty".into());
                };
                // "npx -y pkg" typed as the command: keep `npx`, move the rest to args.
                if !rest.is_empty() {
                    let moved = shell::join(rest);
                    self.args = if self.args.trim().is_empty() {
                        moved
                    } else {
                        format!("{} {}", moved, self.args)
                    };
                }
                self.command = cmd.clone();
                AddStep::Args
            }
            AddStep::Args => {
                shell::split(&self.args)?;
                AddStep::EnvVars
            }
            AddStep::Url => {
                let url = self.url.trim().to_string();
                let rest = url
                    .strip_prefix("http://")
                    .or_else(|| url.strip_prefix("https://"))
                    .ok_or("URL must start with http:// or https://")?;
                if rest.is_empty() || rest.starts_with('/') {
                    return Err("URL is missing a host".into());
                }
                self.url = url;
                AddStep::Headers
            }
            AddStep::Headers | AddStep::EnvVars => {
                if self.pair_input.trim().is_empty() {
                    self.pair_input.clear();
                    self.clients.cursor = 0;
                    AddStep::Clients
                } else {
                    self.commit_pair()?;
                    self.step
                }
            }
            AddStep::Clients => {
                if self.clients.selected().is_empty() {
                    return Err("Select at least one client".into());
                }
                AddStep::Confirm
            }
            AddStep::Confirm => AddStep::Confirm,
        };
        Ok(())
    }

    /// Validate and store the current `KEY=VALUE` / `Header: value` line.
    fn commit_pair(&mut self) -> Result<(), String> {
        let headers = self.step == AddStep::Headers;
        let (sep, hint) = if headers {
            (':', "Format: Header-Name: value")
        } else {
            ('=', "Format: KEY=VALUE")
        };
        let (k, v) = self.pair_input.split_once(sep).ok_or(hint)?;
        let k = k.trim();
        if k.is_empty() || k.chars().any(|c| c.is_whitespace() || c == '=' || c == ':') {
            return Err(hint.into());
        }
        let v = if headers { v.trim() } else { v };
        let map = if headers {
            &mut self.headers
        } else {
            &mut self.env
        };
        map.insert(k.to_string(), v.to_string());
        self.pair_input.clear();
        Ok(())
    }

    /// Go back one step. Returns false when already at the first step.
    pub fn back(&mut self) -> bool {
        self.error = None;
        self.step = match self.step {
            AddStep::Name => return false,
            AddStep::TransportType => AddStep::Name,
            AddStep::Command | AddStep::Url => AddStep::TransportType,
            AddStep::Args => AddStep::Command,
            AddStep::EnvVars => AddStep::Args,
            AddStep::Headers => AddStep::Url,
            AddStep::Clients => match self.transport {
                TransportKind::Stdio => AddStep::EnvVars,
                _ => AddStep::Headers,
            },
            AddStep::Confirm => AddStep::Clients,
        };
        true
    }

    pub fn server_name(&self) -> String {
        self.name.trim().to_string()
    }

    /// The transport described by the wizard's inputs.
    pub fn transport_value(&self) -> Transport {
        match self.transport {
            TransportKind::Stdio => Transport::Stdio {
                command: self.command.trim().to_string(),
                args: shell::split(&self.args).unwrap_or_default(),
            },
            TransportKind::Http => Transport::Http {
                url: self.url.clone(),
                headers: self.headers.clone(),
            },
            TransportKind::Sse => Transport::Sse {
                url: self.url.clone(),
                headers: self.headers.clone(),
            },
        }
    }

    /// The JSON entry that will be written.
    pub fn server_value(&self) -> Value {
        let env = if self.transport == TransportKind::Stdio {
            self.env.clone()
        } else {
            BTreeMap::new()
        };
        config_writer::server_value(&self.transport_value(), &env)
    }

    pub fn step_label(&self) -> &'static str {
        match self.step {
            AddStep::Name => "Server Name",
            AddStep::TransportType => "Transport Type",
            AddStep::Command => "Command",
            AddStep::Args => "Arguments",
            AddStep::Url => "Server URL",
            AddStep::Headers => "HTTP Headers",
            AddStep::EnvVars => "Environment Variables",
            AddStep::Clients => "Install to Clients",
            AddStep::Confirm => "Confirm",
        }
    }
}

// ---------------------------------------------------------------------------
// Remove Confirm
// ---------------------------------------------------------------------------

#[derive(PartialEq)]
pub enum RemoveStep {
    SelectClients,
    Confirm,
}

pub struct RemoveConfirm {
    pub server_name: String,
    /// Each concrete config entry carrying this name.
    pub targets: Checklist<McpServer>,
    pub step: RemoveStep,
}

impl RemoveConfirm {
    /// Pre-selects only `focused` (the row the user acted on); other copies
    /// are listed but opt-in.
    pub fn new(server_name: String, instances: Vec<McpServer>, focused: &McpServer) -> Self {
        let focused_id = focused.id();
        let cursor = instances
            .iter()
            .position(|s| s.id() == focused_id)
            .unwrap_or(0);
        let mut targets = Checklist::new(instances.into_iter().map(|s| {
            let on = s.id() == focused_id;
            (s, on)
        }));
        targets.cursor = cursor;
        RemoveConfirm {
            server_name,
            targets,
            step: RemoveStep::SelectClients,
        }
    }
}

// ---------------------------------------------------------------------------
// Sync Select
// ---------------------------------------------------------------------------

pub struct SyncSelect {
    pub server_name: String,
    /// Origin shown in the title, e.g. "CC-Global"
    pub source_label: String,
    pub server_value: Value,
    pub targets: Checklist<ClientKind>,
}

impl SyncSelect {
    pub fn new(server: &McpServer, missing_clients: Vec<ClientKind>) -> Self {
        SyncSelect {
            server_name: server.name.clone(),
            source_label: server.origin_label(),
            server_value: server.raw.clone(),
            targets: Checklist::new(missing_clients.into_iter().map(|c| (c, false))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn wizard() -> AddWizard {
        AddWizard::new(ClientKind::writable())
    }

    fn type_and_enter(w: &mut AddWizard, s: &str) -> bool {
        w.insert_str(s);
        w.advance()
    }

    #[test]
    fn stdio_flow_moves_inline_args_and_keeps_quotes() {
        let mut w = wizard();
        assert!(type_and_enter(&mut w, " fs "));
        assert!(w.advance()); // stdio
        assert!(type_and_enter(&mut w, "npx -y @mcp/fs"));
        assert_eq!(w.command, "npx");
        assert_eq!(w.args, "-y @mcp/fs");
        w.args.push_str(" '/my docs'");
        assert!(w.advance());
        assert!(type_and_enter(&mut w, "TOKEN=a=b "));
        assert!(w.advance()); // empty line → clients
        assert_eq!(w.step, AddStep::Clients);
        assert_eq!(w.clients.selected(), vec![ClientKind::ClaudeCodeProject]);
        assert_eq!(w.server_name(), "fs");
        assert_eq!(
            w.server_value(),
            json!({"command": "npx", "args": ["-y", "@mcp/fs", "/my docs"], "env": {"TOKEN": "a=b "}})
        );
    }

    #[test]
    fn http_flow_collects_headers_not_env() {
        let mut w = wizard();
        type_and_enter(&mut w, "remote");
        w.transport_down();
        assert!(w.advance());
        assert_eq!(w.step, AddStep::Url);
        assert!(!type_and_enter(&mut w, "localhost:3000"));
        w.clear_input();
        assert!(type_and_enter(&mut w, "https://api.example.com/mcp"));
        assert_eq!(w.step, AddStep::Headers);
        assert!(!type_and_enter(&mut w, "no separator"));
        w.clear_input();
        assert!(type_and_enter(&mut w, "Authorization: Bearer x"));
        assert!(w.advance());
        assert_eq!(
            w.server_value(),
            json!({"type": "http", "url": "https://api.example.com/mcp", "headers": {"Authorization": "Bearer x"}})
        );
        assert!(w.back());
        assert_eq!(w.step, AddStep::Headers);
    }

    #[test]
    fn validation_errors() {
        let mut w = wizard();
        assert!(!type_and_enter(&mut w, "two words"));
        w.clear_input();
        type_and_enter(&mut w, "x");
        w.advance();
        assert!(!type_and_enter(&mut w, "node 'unterminated"));
        assert!(w.error.is_some());
    }

    #[test]
    fn backspace_on_empty_pair_line_reopens_last_entry() {
        let mut w = wizard();
        w.step = AddStep::EnvVars;
        type_and_enter(&mut w, "A=1");
        assert!(w.env.contains_key("A"));
        w.backspace();
        assert_eq!(w.pair_input, "A=1");
        assert!(w.env.is_empty());
    }

    #[test]
    fn pasted_newlines_do_not_break_single_line_input() {
        let mut w = wizard();
        w.insert_str("a\nb");
        assert_eq!(w.name, "a b");
    }
}
