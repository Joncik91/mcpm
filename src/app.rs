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
    pub detail_content_height: usize, // lines in detail panel (set during render)
    pub detail_visible_height: usize, // visible area of detail panel (set during render)
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
            detail_content_height: 0,
            detail_visible_height: 0,
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
        if self.detail_content_height > self.detail_visible_height {
            let max_offset = self.detail_content_height - self.detail_visible_height;
            if self.scroll_offset < max_offset {
                self.scroll_offset += 1;
            }
        }
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
        let env = server.env.clone().unwrap_or_default();
        match &server.transport {
            Transport::Stdio { command, args } => {
                config_writer::build_server_value(command, args, &env)
            }
            Transport::Http { url, headers } => {
                config_writer::build_http_server_value(url, headers.as_ref(), &env)
            }
            Transport::Sse { url } => {
                config_writer::build_sse_server_value(url, &env)
            }
            Transport::Unknown => serde_json::json!({}),
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
        KeyCode::Char('h') => {
            if let Some(server) = app.selected_server() {
                if !server.transport.is_stdio() {
                    app.set_status("Health checks only available for stdio servers".to_string());
                } else {
                    app.check_selected();
                }
            }
        }
        KeyCode::Char('c') => app.check_all(),
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
                if matches!(server.transport, Transport::Unknown) {
                    app.set_status("Cannot sync server with unknown transport".to_string());
                } else {
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
        }
        KeyCode::Char('u') => {
            // Undo: restore from .bak file for the selected server's client
            if let Some(server) = app.selected_server() {
                let client = server.client.clone();
                if client == ClientKind::ClaudeCodePlugin {
                    app.set_status("Cannot undo plugin config changes".to_string());
                } else {
                    match config_writer::restore_backup(&client, &app.cwd) {
                        Ok(()) => {
                            app.refresh();
                            app.set_status(format!("Restored backup for {}", client.label()));
                        }
                        Err(e) => app.set_status(format!("Undo failed: {}", e)),
                    }
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
                } else {
                    app.set_status("Cannot edit plugin configs — they are read-only".to_string());
                }
            }
        }
        _ => {}
    }
    Ok((false, None))
}

fn handle_add_wizard(app: &mut App, key: KeyEvent) {
    // Clone cwd up front so the overwrite check (which reads config files) needs
    // no borrow of `app` while `wiz` holds a mutable borrow of `app.mode`.
    let cwd = app.cwd.clone();

    let Mode::AddWizard(ref mut wiz) = app.mode else {
        return;
    };

    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        _ => match wiz.step {
            AddStep::Name | AddStep::Command | AddStep::Args | AddStep::Url | AddStep::EnvVars => {
                match key.code {
                    KeyCode::Char(c) => wiz.push_char(c),
                    KeyCode::Backspace => wiz.pop_char(),
                    KeyCode::Enter => {
                        wiz.advance();
                    }
                    _ => {}
                }
            }
            AddStep::TransportType => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    wiz.transport_type = wiz.transport_type.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if wiz.transport_type < 2 {
                        wiz.transport_type += 1;
                    }
                }
                KeyCode::Enter => {
                    wiz.advance();
                }
                _ => {}
            },
            AddStep::Clients => match key.code {
                KeyCode::Up | KeyCode::Char('k') => wiz.cursor_up(),
                KeyCode::Down | KeyCode::Char('j') => wiz.cursor_down(),
                KeyCode::Char(' ') => wiz.toggle_client(),
                KeyCode::Enter => {
                    // advance() validates "at least one client" and moves to
                    // Confirm; on success, compute which targets would actually
                    // be clobbered by checking the real write scope on disk.
                    if wiz.advance() {
                        let name = wiz.name.trim().to_string();
                        wiz.overwrite_clients = wiz
                            .selected_clients()
                            .into_iter()
                            .filter(|c| {
                                config_writer::server_exists_in_scope(c, &cwd, &name)
                            })
                            .collect();
                    }
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
    let server_value = match wiz.transport_type {
        1 => config_writer::build_http_server_value(&wiz.url, None, &env),
        2 => config_writer::build_sse_server_value(&wiz.url, &env),
        _ => config_writer::build_server_value(&wiz.command, &args, &env),
    };
    let clients = wiz.selected_clients();

    let mut errors = Vec::new();
    let mut success_count = 0;
    let mut overwrote_count = 0;

    for client in &clients {
        match config_writer::add_server(client, &app.cwd, &name, &server_value) {
            Ok(overwrote) => {
                success_count += 1;
                if overwrote {
                    overwrote_count += 1;
                }
            }
            Err(e) => errors.push(format!("{}: {}", client.label(), e)),
        }
    }

    if errors.is_empty() {
        // Report overwrites from the authoritative write result, not just the
        // pre-confirm warning — a stale snapshot can't hide a real clobber here.
        let mut msg = format!(
            "Added \"{}\" to {} client{}",
            name,
            success_count,
            if success_count == 1 { "" } else { "s" }
        );
        if overwrote_count > 0 {
            msg.push_str(&format!(
                " (overwrote {} existing)",
                overwrote_count
            ));
        }
        app.set_status(msg);
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
    let mut removed_count = 0;
    let mut not_found = Vec::new();

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
            Ok(true) => removed_count += 1,
            Ok(false) => not_found.push(client.label()),
            Err(e) => errors.push(format!("{}: {}", client.label(), e)),
        }
    }

    if removed_count == 0 && errors.is_empty() {
        // Every target was a no-op — e.g. a CC-Global entry that only lives in a
        // project scope (D002 leaves those untouched). Don't claim a fake success.
        app.set_status(format!(
            "\"{}\" not present in selected config{} — nothing removed",
            name,
            if not_found.len() == 1 { "" } else { "s" }
        ));
    } else {
        // Surface every outcome together so a partial success (some configs
        // mutated) is never masked by an error on another client.
        let mut parts: Vec<String> = Vec::new();
        if removed_count > 0 {
            parts.push(format!(
                "removed from {} client{}",
                removed_count,
                if removed_count == 1 { "" } else { "s" }
            ));
        }
        if !not_found.is_empty() {
            parts.push(format!("not present in {}", not_found.join(", ")));
        }
        if !errors.is_empty() {
            parts.push(format!("errors: {}", errors.join("; ")));
        }
        app.set_status(format!("\"{}\": {}", name, parts.join("; ")));
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
            Ok(_) => success_count += 1,
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
