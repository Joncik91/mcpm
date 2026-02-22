use std::collections::HashMap;

use crate::types::ClientKind;

// ---------------------------------------------------------------------------
// Mode — top-level modal state
// ---------------------------------------------------------------------------

pub enum Mode {
    Normal,
    AddWizard(AddWizard),
    RemoveConfirm(RemoveConfirm),
    SyncSelect(SyncSelect),
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Normal
    }
}

// ---------------------------------------------------------------------------
// Add Server Wizard
// ---------------------------------------------------------------------------

pub struct AddWizard {
    pub step: AddStep,
    pub name: String,
    pub command: String,
    pub args: String,
    pub env_lines: Vec<String>,
    pub env_input: String,
    pub clients: Vec<(ClientKind, bool)>,
    pub cursor: usize,
    pub error: Option<String>,
}

#[derive(PartialEq)]
pub enum AddStep {
    Name,
    Command,
    Args,
    EnvVars,
    Clients,
    Confirm,
}

impl AddWizard {
    pub fn new() -> Self {
        let clients = ClientKind::writable()
            .iter()
            .map(|c| {
                let default_on = matches!(c, ClientKind::ClaudeCodeProject);
                (c.clone(), default_on)
            })
            .collect();
        AddWizard {
            step: AddStep::Name,
            name: String::new(),
            command: String::new(),
            args: String::new(),
            env_lines: Vec::new(),
            env_input: String::new(),
            clients,
            cursor: 0,
            error: None,
        }
    }

    /// Get the current text input buffer for the active step
    pub fn current_input(&self) -> &str {
        match self.step {
            AddStep::Name => &self.name,
            AddStep::Command => &self.command,
            AddStep::Args => &self.args,
            AddStep::EnvVars => &self.env_input,
            _ => "",
        }
    }

    /// Push a character to the current input buffer
    pub fn push_char(&mut self, c: char) {
        self.error = None;
        match self.step {
            AddStep::Name => self.name.push(c),
            AddStep::Command => self.command.push(c),
            AddStep::Args => self.args.push(c),
            AddStep::EnvVars => self.env_input.push(c),
            _ => {}
        }
    }

    /// Backspace on current input
    pub fn pop_char(&mut self) {
        match self.step {
            AddStep::Name => { self.name.pop(); }
            AddStep::Command => { self.command.pop(); }
            AddStep::Args => { self.args.pop(); }
            AddStep::EnvVars => { self.env_input.pop(); }
            _ => {}
        }
    }

    /// Advance to next step. Returns true if validation passed.
    pub fn advance(&mut self) -> bool {
        match self.step {
            AddStep::Name => {
                if self.name.trim().is_empty() {
                    self.error = Some("Server name cannot be empty".to_string());
                    return false;
                }
                self.step = AddStep::Command;
            }
            AddStep::Command => {
                if self.command.trim().is_empty() {
                    self.error = Some("Command cannot be empty".to_string());
                    return false;
                }
                self.step = AddStep::Args;
            }
            AddStep::Args => {
                self.step = AddStep::EnvVars;
            }
            AddStep::EnvVars => {
                if self.env_input.is_empty() {
                    // Empty line → done with env vars
                    self.step = AddStep::Clients;
                    self.cursor = 0;
                } else {
                    // Validate KEY=VALUE format
                    if self.env_input.contains('=') {
                        self.env_lines.push(self.env_input.clone());
                        self.env_input.clear();
                    } else {
                        self.error = Some("Format: KEY=VALUE".to_string());
                        return false;
                    }
                }
            }
            AddStep::Clients => {
                let any_selected = self.clients.iter().any(|(_, sel)| *sel);
                if !any_selected {
                    self.error = Some("Select at least one client".to_string());
                    return false;
                }
                self.step = AddStep::Confirm;
            }
            AddStep::Confirm => {
                // handled externally (y to confirm)
            }
        }
        true
    }

    /// Toggle checkbox at current cursor position
    pub fn toggle_client(&mut self) {
        if let Some((_, sel)) = self.clients.get_mut(self.cursor) {
            *sel = !*sel;
        }
    }

    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.clients.len() {
            self.cursor += 1;
        }
    }

    /// Parse collected data into args vec and env map
    pub fn parsed_args(&self) -> Vec<String> {
        if self.args.trim().is_empty() {
            Vec::new()
        } else {
            self.args.split_whitespace().map(String::from).collect()
        }
    }

    pub fn parsed_env(&self) -> HashMap<String, String> {
        self.env_lines
            .iter()
            .filter_map(|line| {
                let (k, v) = line.split_once('=')?;
                Some((k.trim().to_string(), v.trim().to_string()))
            })
            .collect()
    }

    pub fn selected_clients(&self) -> Vec<ClientKind> {
        self.clients
            .iter()
            .filter(|(_, sel)| *sel)
            .map(|(c, _)| c.clone())
            .collect()
    }

    pub fn step_label(&self) -> &'static str {
        match self.step {
            AddStep::Name => "Server Name",
            AddStep::Command => "Command",
            AddStep::Args => "Arguments (space-separated)",
            AddStep::EnvVars => "Environment Variables",
            AddStep::Clients => "Install to Clients",
            AddStep::Confirm => "Confirm",
        }
    }
}

// ---------------------------------------------------------------------------
// Remove Confirm
// ---------------------------------------------------------------------------

pub struct RemoveConfirm {
    pub server_name: String,
    pub clients: Vec<(ClientKind, bool)>,
    pub cursor: usize,
    pub step: RemoveStep,
}

#[derive(PartialEq)]
pub enum RemoveStep {
    SelectClients,
    Confirm,
}

impl RemoveConfirm {
    pub fn new(server_name: String, clients_with_server: Vec<ClientKind>) -> Self {
        let clients = clients_with_server
            .into_iter()
            .map(|c| (c, true)) // pre-select all
            .collect();
        RemoveConfirm {
            server_name,
            clients,
            cursor: 0,
            step: RemoveStep::SelectClients,
        }
    }

    pub fn toggle_client(&mut self) {
        if let Some((_, sel)) = self.clients.get_mut(self.cursor) {
            *sel = !*sel;
        }
    }

    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.clients.len() {
            self.cursor += 1;
        }
    }

    pub fn selected_clients(&self) -> Vec<ClientKind> {
        self.clients
            .iter()
            .filter(|(_, sel)| *sel)
            .map(|(c, _)| c.clone())
            .collect()
    }

    pub fn advance(&mut self) -> bool {
        match self.step {
            RemoveStep::SelectClients => {
                if self.selected_clients().is_empty() {
                    return false;
                }
                self.step = RemoveStep::Confirm;
                true
            }
            RemoveStep::Confirm => true,
        }
    }
}

// ---------------------------------------------------------------------------
// Sync Select
// ---------------------------------------------------------------------------

pub struct SyncSelect {
    pub server_name: String,
    pub server_value: serde_json::Value,
    pub targets: Vec<(ClientKind, bool)>,
    pub cursor: usize,
}

impl SyncSelect {
    pub fn new(
        server_name: String,
        server_value: serde_json::Value,
        missing_clients: Vec<ClientKind>,
    ) -> Self {
        let targets = missing_clients.into_iter().map(|c| (c, false)).collect();
        SyncSelect {
            server_name,
            server_value,
            targets,
            cursor: 0,
        }
    }

    pub fn toggle_client(&mut self) {
        if let Some((_, sel)) = self.targets.get_mut(self.cursor) {
            *sel = !*sel;
        }
    }

    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.targets.len() {
            self.cursor += 1;
        }
    }

    pub fn selected_clients(&self) -> Vec<ClientKind> {
        self.targets
            .iter()
            .filter(|(_, sel)| *sel)
            .map(|(c, _)| c.clone())
            .collect()
    }
}
