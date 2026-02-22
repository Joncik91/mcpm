use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::config_writer;
use crate::discovery::discover;
use crate::health;
use crate::types::{ClientKind, DiscoveryResult, HealthResult, HealthStatus, McpServer, Transport};
use crate::wizard::*;

pub struct App {
    pub result: DiscoveryResult,
    pub selected: usize,
    pub scroll_offset: usize,
    pub show_errors: bool,
    pub cwd: PathBuf,
    pub health_tx: mpsc::Sender<HealthResult>,
    pub health_rx: mpsc::Receiver<HealthResult>,
    pub checking_count: usize,
    pub mode: Mode,
    pub status_message: Option<String>,
    pub status_timer: u8, // frames to show status message
}

impl App {
    pub fn new(cwd: PathBuf) -> Self {
        let result = discover(&cwd);
        let (health_tx, health_rx) = mpsc::channel();
        App {
            result,
            selected: 0,
            scroll_offset: 0,
            show_errors: false,
            cwd,
            health_tx,
            health_rx,
            checking_count: 0,
            mode: Mode::Normal,
            status_message: None,
            status_timer: 0,
        }
    }

    pub fn refresh(&mut self) {
        self.result = discover(&self.cwd);
        if self.selected >= self.result.servers.len() {
            self.selected = self.result.servers.len().saturating_sub(1);
        }
        self.scroll_offset = 0;
    }

    pub fn selected_server(&self) -> Option<&McpServer> {
        self.result.servers.get(self.selected)
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.scroll_offset = 0;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.result.servers.len() {
            self.selected += 1;
            self.scroll_offset = 0;
        }
    }

    pub fn scroll_detail_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_detail_down(&mut self) {
        self.scroll_offset += 1;
    }

    pub fn check_selected(&mut self) {
        let idx = self.selected;
        if idx >= self.result.servers.len() {
            return;
        }
        if !self.result.servers[idx].transport.is_stdio() {
            return;
        }
        let server = self.result.servers[idx].clone();
        self.result.servers[idx].health = HealthStatus::Checking;
        self.checking_count += 1;
        health::spawn_health_check(idx, &server, self.health_tx.clone());
    }

    pub fn check_all(&mut self) {
        let servers: Vec<(usize, McpServer)> = self
            .result
            .servers
            .iter()
            .enumerate()
            .filter(|(_, s)| s.transport.is_stdio())
            .map(|(i, s)| (i, s.clone()))
            .collect();

        for (i, server) in &servers {
            self.result.servers[*i].health = HealthStatus::Checking;
            self.checking_count += 1;
            health::spawn_health_check(*i, server, self.health_tx.clone());
        }
    }

    pub fn poll_health(&mut self) {
        while let Ok(result) = self.health_rx.try_recv() {
            if let Some(server) = self.result.servers.get_mut(result.server_index) {
                server.health = result.status;
                server.last_checked = Some(result.checked_at);
            }
            self.checking_count = self.checking_count.saturating_sub(1);
        }
    }

    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some(msg);
        self.status_timer = 15; // ~3 seconds at 200ms poll
    }

    pub fn tick_status(&mut self) {
        if self.status_timer > 0 {
            self.status_timer -= 1;
            if self.status_timer == 0 {
                self.status_message = None;
            }
        }
    }

    /// Find all clients that have a server with the given name
    pub fn clients_with_server(&self, name: &str) -> Vec<ClientKind> {
        self.result
            .servers
            .iter()
            .filter(|s| s.name == name)
            .map(|s| s.client.clone())
            .collect()
    }

    /// Find writable clients that DON'T have a server with the given name
    pub fn clients_without_server(&self, name: &str) -> Vec<ClientKind> {
        let have: HashSet<ClientKind> = self.clients_with_server(name).into_iter().collect();
        ClientKind::writable()
            .iter()
            .filter(|c| !have.contains(c))
            .cloned()
            .collect()
    }

    /// Build a server's JSON value from its transport + env
    pub fn server_to_value(&self, server: &McpServer) -> serde_json::Value {
        match &server.transport {
            Transport::Stdio { command, args } => {
                config_writer::build_server_value(
                    command,
                    args,
                    &server.env.clone().unwrap_or_default(),
                )
            }
            _ => serde_json::json!({}),
        }
    }
}

/// Returns (should_exit, need_editor_path)
/// When need_editor_path is Some, the caller should exit TUI, run editor, re-enter TUI.
pub fn handle_event(app: &mut App) -> std::io::Result<(bool, Option<PathBuf>)> {
    app.poll_health();
    app.tick_status();

    if event::poll(std::time::Duration::from_millis(200))? {
        if let Event::Key(key @ KeyEvent { kind: KeyEventKind::Press, .. }) = event::read()? {
            // Ctrl-C always exits
            if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
                return Ok((true, None));
            }

            match &app.mode {
                Mode::Normal => return handle_normal(app, key),
                Mode::AddWizard(_) => handle_add_wizard(app, key),
                Mode::RemoveConfirm(_) => handle_remove(app, key),
                Mode::SyncSelect(_) => handle_sync(app, key),
            }
        }
    }
    Ok((false, None))
}

fn handle_normal(app: &mut App, key: KeyEvent) -> std::io::Result<(bool, Option<PathBuf>)> {
    match key.code {
        KeyCode::Char('q') => return Ok((true, None)),
        KeyCode::Char('r') => app.refresh(),
        KeyCode::Char('!') => app.show_errors = !app.show_errors,
        KeyCode::Char('h') => app.check_selected(),
        KeyCode::Char('H') => app.check_all(),
        KeyCode::Up | KeyCode::Char('k') => app.move_up(),
        KeyCode::Down | KeyCode::Char('j') => app.move_down(),
        KeyCode::PageUp => app.scroll_detail_up(),
        KeyCode::PageDown => app.scroll_detail_down(),
        KeyCode::Char('a') => {
            app.mode = Mode::AddWizard(AddWizard::new());
        }
        KeyCode::Char('d') => {
            if let Some(server) = app.selected_server() {
                let name = server.name.clone();
                let clients = app.clients_with_server(&name);
                // Filter to deletable clients (writable + plugins)
                let mut deletable: HashSet<ClientKind> =
                    ClientKind::writable().iter().cloned().collect();
                deletable.insert(ClientKind::ClaudeCodePlugin);
                let writable_clients: Vec<ClientKind> =
                    clients.into_iter().filter(|c| deletable.contains(c)).collect();
                if writable_clients.is_empty() {
                    app.set_status("No writable configs for this server".to_string());
                } else {
                    app.mode = Mode::RemoveConfirm(RemoveConfirm::new(name, writable_clients));
                }
            }
        }
        KeyCode::Char('s') => {
            if let Some(server) = app.selected_server() {
                let name = server.name.clone();
                let value = app.server_to_value(server);
                let missing = app.clients_without_server(&name);
                if missing.is_empty() {
                    app.set_status("Server already in all clients".to_string());
                } else {
                    app.mode = Mode::SyncSelect(SyncSelect::new(name, value, missing));
                }
            }
        }
        KeyCode::Char('e') => {
            // Open selected server's config in $EDITOR
            if let Some(server) = app.selected_server() {
                let client = server.client.clone();
                if let Some(path) = client.config_path(&app.cwd) {
                    if path.exists() {
                        return Ok((false, Some(path)));
                    } else {
                        app.set_status(format!("Config file doesn't exist: {}", path.display()));
                    }
                }
            }
        }
        _ => {}
    }
    Ok((false, None))
}

fn handle_add_wizard(app: &mut App, key: KeyEvent) {
    let Mode::AddWizard(ref mut wiz) = app.mode else {
        return;
    };

    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        _ => match wiz.step {
            AddStep::Name | AddStep::Command | AddStep::Args | AddStep::EnvVars => {
                match key.code {
                    KeyCode::Char(c) => wiz.push_char(c),
                    KeyCode::Backspace => wiz.pop_char(),
                    KeyCode::Enter => {
                        wiz.advance();
                    }
                    _ => {}
                }
            }
            AddStep::Clients => match key.code {
                KeyCode::Up | KeyCode::Char('k') => wiz.cursor_up(),
                KeyCode::Down | KeyCode::Char('j') => wiz.cursor_down(),
                KeyCode::Char(' ') => wiz.toggle_client(),
                KeyCode::Enter => {
                    wiz.advance();
                }
                _ => {}
            },
            AddStep::Confirm => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    execute_add(app);
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    app.mode = Mode::Normal;
                }
                _ => {}
            },
        },
    }
}

fn execute_add(app: &mut App) {
    let Mode::AddWizard(ref wiz) = app.mode else {
        return;
    };

    let name = wiz.name.trim().to_string();
    let args = wiz.parsed_args();
    let env = wiz.parsed_env();
    let server_value = config_writer::build_server_value(&wiz.command, &args, &env);
    let clients = wiz.selected_clients();

    let mut errors = Vec::new();
    let mut success_count = 0;

    for client in &clients {
        match config_writer::add_server(client, &app.cwd, &name, &server_value) {
            Ok(()) => success_count += 1,
            Err(e) => errors.push(format!("{}: {}", client.label(), e)),
        }
    }

    if errors.is_empty() {
        app.set_status(format!(
            "Added \"{}\" to {} client{}",
            name,
            success_count,
            if success_count == 1 { "" } else { "s" }
        ));
    } else {
        app.set_status(format!("Errors: {}", errors.join("; ")));
    }

    app.mode = Mode::Normal;
    app.refresh();
}

fn handle_remove(app: &mut App, key: KeyEvent) {
    let Mode::RemoveConfirm(ref mut rm) = app.mode else {
        return;
    };

    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        _ => match rm.step {
            RemoveStep::SelectClients => match key.code {
                KeyCode::Up | KeyCode::Char('k') => rm.cursor_up(),
                KeyCode::Down | KeyCode::Char('j') => rm.cursor_down(),
                KeyCode::Char(' ') => rm.toggle_client(),
                KeyCode::Enter => {
                    rm.advance();
                }
                _ => {}
            },
            RemoveStep::Confirm => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    execute_remove(app);
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    app.mode = Mode::Normal;
                }
                _ => {}
            },
        },
    }
}

fn execute_remove(app: &mut App) {
    let Mode::RemoveConfirm(ref rm) = app.mode else {
        return;
    };

    let name = rm.server_name.clone();
    let clients = rm.selected_clients();
    let mut errors = Vec::new();
    let mut success_count = 0;

    // For plugin servers, find the source_path
    let plugin_source: Option<String> = app
        .result
        .servers
        .iter()
        .find(|s| s.name == name && s.client == ClientKind::ClaudeCodePlugin)
        .map(|s| s.source_path.clone());

    for client in &clients {
        let res = if *client == ClientKind::ClaudeCodePlugin {
            if let Some(ref src) = plugin_source {
                config_writer::remove_plugin_server(&app.cwd, &name, src)
            } else {
                Err("plugin source path not found".to_string())
            }
        } else {
            config_writer::remove_server(client, &app.cwd, &name)
        };
        match res {
            Ok(()) => success_count += 1,
            Err(e) => errors.push(format!("{}: {}", client.label(), e)),
        }
    }

    if errors.is_empty() {
        app.set_status(format!(
            "Removed \"{}\" from {} client{}",
            name,
            success_count,
            if success_count == 1 { "" } else { "s" }
        ));
    } else {
        app.set_status(format!("Errors: {}", errors.join("; ")));
    }

    app.mode = Mode::Normal;
    app.refresh();
}

fn handle_sync(app: &mut App, key: KeyEvent) {
    let Mode::SyncSelect(ref mut sync) = app.mode else {
        return;
    };

    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Up | KeyCode::Char('k') => sync.cursor_up(),
        KeyCode::Down | KeyCode::Char('j') => sync.cursor_down(),
        KeyCode::Char(' ') => sync.toggle_client(),
        KeyCode::Enter => {
            let selected = sync.selected_clients();
            if selected.is_empty() {
                return;
            }
            execute_sync(app);
        }
        _ => {}
    }
}

fn execute_sync(app: &mut App) {
    let Mode::SyncSelect(ref sync) = app.mode else {
        return;
    };

    let name = sync.server_name.clone();
    let value = sync.server_value.clone();
    let clients = sync.selected_clients();
    let mut errors = Vec::new();
    let mut success_count = 0;

    for client in &clients {
        match config_writer::add_server(client, &app.cwd, &name, &value) {
            Ok(()) => success_count += 1,
            Err(e) => errors.push(format!("{}: {}", client.label(), e)),
        }
    }

    if errors.is_empty() {
        app.set_status(format!(
            "Synced \"{}\" to {} client{}",
            name,
            success_count,
            if success_count == 1 { "" } else { "s" }
        ));
    } else {
        app.set_status(format!("Errors: {}", errors.join("; ")));
    }

    app.mode = Mode::Normal;
    app.refresh();
}
