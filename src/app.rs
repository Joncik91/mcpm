use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::widgets::ListState;

use crate::config_writer::{self, Change};
use crate::discovery::{discover, same_file};
use crate::health;
use crate::types::{
    ClientKind, Ctx, DiscoveryResult, HealthResult, HealthStatus, McpServer, ServerId,
};
use crate::wizard::*;

const UNDO_LIMIT: usize = 50;
/// Undo entries hold whole files (~/.claude.json can be many MB); cap memory too.
const UNDO_BYTES_LIMIT: usize = 64 << 20;

/// What the event loop must do after a key press.
#[derive(Debug, PartialEq)]
pub enum Action {
    None,
    Quit,
    /// Suspend the TUI and open this file in the user's editor.
    Edit(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StatusKind {
    Info,
    Success,
    Error,
}

pub struct Status {
    pub text: String,
    pub kind: StatusKind,
    expires: Instant,
}

pub struct HealthEntry {
    pub status: HealthStatus,
    pub checked_at: Option<Instant>,
}

/// One user operation's file changes, undoable as a unit.
struct Transaction {
    label: String,
    changes: Vec<Change>,
}

pub struct App {
    pub ctx: Ctx,
    pub result: DiscoveryResult,
    pub list_state: ListState,
    pub detail_scroll: u16,
    /// Health keyed by server identity, so results stay attached to the right
    /// server across rescans and list reordering.
    pub health: HashMap<ServerId, HealthEntry>,
    health_tx: mpsc::Sender<HealthResult>,
    health_rx: mpsc::Receiver<HealthResult>,
    pub mode: Mode,
    status: Option<Status>,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    pub timeout: Duration,
}

impl App {
    pub fn new(ctx: Ctx, timeout: Duration) -> Self {
        let result = discover(&ctx);
        let (health_tx, health_rx) = mpsc::channel();
        let mut list_state = ListState::default();
        if !result.servers.is_empty() {
            list_state.select(Some(0));
        }
        App {
            ctx,
            result,
            list_state,
            detail_scroll: 0,
            health: HashMap::new(),
            health_tx,
            health_rx,
            mode: Mode::Normal,
            status: None,
            undo: Vec::new(),
            redo: Vec::new(),
            timeout,
        }
    }

    /// Rescan configs, keeping the selection on the same server when it still exists.
    pub fn refresh(&mut self) {
        let keep = self.selected_server().map(McpServer::id);
        self.result = discover(&self.ctx);
        let ids: HashSet<ServerId> = self.result.servers.iter().map(McpServer::id).collect();
        self.health
            .retain(|id, h| ids.contains(id) || h.status == HealthStatus::Checking);
        let idx = keep
            .and_then(|id| self.result.servers.iter().position(|s| s.id() == id))
            .or(self.list_state.selected());
        self.select(idx);
    }

    fn select(&mut self, idx: Option<usize>) {
        let len = self.result.servers.len();
        let idx = if len == 0 {
            None
        } else {
            Some(idx.unwrap_or(0).min(len - 1))
        };
        if idx != self.list_state.selected() {
            self.detail_scroll = 0;
        }
        self.list_state.select(idx);
    }

    fn select_by(&mut self, pred: impl Fn(&McpServer) -> bool) {
        if let Some(i) = self.result.servers.iter().position(pred) {
            self.select(Some(i));
        }
    }

    pub fn selected_server(&self) -> Option<&McpServer> {
        self.list_state
            .selected()
            .and_then(|i| self.result.servers.get(i))
    }

    pub fn health_of(&self, s: &McpServer) -> Option<&HealthEntry> {
        self.health.get(&s.id())
    }

    pub fn checking_count(&self) -> usize {
        self.health
            .values()
            .filter(|h| h.status == HealthStatus::Checking)
            .count()
    }

    fn move_by(&mut self, delta: isize) {
        let cur = self.list_state.selected().unwrap_or(0);
        self.select(Some(cur.saturating_add_signed(delta)));
    }

    // -----------------------------------------------------------------------
    // Health
    // -----------------------------------------------------------------------

    /// Start a check unless one is already running. Returns true if started.
    fn start_check(&mut self, server: &McpServer) -> bool {
        let id = server.id();
        if self
            .health
            .get(&id)
            .is_some_and(|h| h.status == HealthStatus::Checking)
        {
            return false;
        }
        let prev = self.health.get(&id).and_then(|h| h.checked_at);
        self.health.insert(
            id,
            HealthEntry {
                status: HealthStatus::Checking,
                checked_at: prev,
            },
        );
        health::spawn_check(server, &self.ctx, self.timeout, self.health_tx.clone());
        true
    }

    fn check_selected(&mut self) {
        let Some(server) = self.selected_server().cloned() else {
            return;
        };
        if !server.transport.is_stdio() {
            self.set_status(
                StatusKind::Info,
                "Health checks only available for stdio servers",
            );
        } else if !self.start_check(&server) {
            self.set_status(StatusKind::Info, "Check already running");
        }
    }

    fn check_all(&mut self) {
        let stdio: Vec<McpServer> = self
            .result
            .servers
            .iter()
            .filter(|s| s.transport.is_stdio())
            .cloned()
            .collect();
        if stdio.is_empty() {
            self.set_status(StatusKind::Info, "No stdio servers to check");
            return;
        }
        let started = stdio.iter().filter(|s| self.start_check(s)).count();
        self.set_status(
            StatusKind::Info,
            format!("Checking {} server{}…", started, plural(started)),
        );
    }

    /// Drain finished health checks. Call every loop iteration.
    pub fn poll_health(&mut self) {
        while let Ok(r) = self.health_rx.try_recv() {
            self.health.insert(
                r.id,
                HealthEntry {
                    status: r.status,
                    checked_at: Some(r.checked_at),
                },
            );
        }
    }

    // -----------------------------------------------------------------------
    // Status line
    // -----------------------------------------------------------------------

    pub fn set_status(&mut self, kind: StatusKind, text: impl Into<String>) {
        let ttl = if kind == StatusKind::Error { 8 } else { 4 };
        self.status = Some(Status {
            text: self.shorten_paths(text.into()),
            kind,
            expires: Instant::now() + Duration::from_secs(ttl),
        });
    }

    /// The status bar is one line: show `./` and `~/` instead of long absolute paths.
    fn shorten_paths(&self, text: String) -> String {
        let sep = std::path::MAIN_SEPARATOR;
        let mut text = text.replace(
            &format!("{}{}", self.ctx.cwd.display(), sep),
            &format!(".{}", sep),
        );
        if let Some(home) = &self.ctx.home {
            text = text.replace(&format!("{}{}", home.display(), sep), &format!("~{}", sep));
        }
        text
    }

    /// The current status message, if it hasn't expired.
    pub fn status(&self) -> Option<&Status> {
        self.status.as_ref().filter(|s| Instant::now() < s.expires)
    }

    // -----------------------------------------------------------------------
    // Targets
    // -----------------------------------------------------------------------

    /// Writable clients that can accept a server right now (deduplicated when
    /// a project config is the same file as a global one, e.g. cwd = $HOME).
    pub fn available_targets(&self) -> Vec<ClientKind> {
        let mut seen: Vec<PathBuf> = Vec::new();
        let mut out = Vec::new();
        for &c in ClientKind::writable() {
            let Some(path) = c.config_path(&self.ctx) else {
                continue;
            };
            if c.is_project_scoped() && seen.iter().any(|p| same_file(p, &path)) {
                continue;
            }
            seen.push(path);
            if config_writer::can_add_to(c, &self.ctx) {
                out.push(c);
            }
        }
        out
    }

    fn instances_of(&self, name: &str) -> Vec<McpServer> {
        self.result
            .servers
            .iter()
            .filter(|s| s.name == name)
            .cloned()
            .collect()
    }

    // -----------------------------------------------------------------------
    // Undo / redo
    // -----------------------------------------------------------------------

    fn record(&mut self, label: String, changes: Vec<Change>) {
        if changes.is_empty() {
            return;
        }
        self.redo.clear();
        self.undo.push(Transaction { label, changes });
        let size = |t: &Transaction| -> usize {
            t.changes
                .iter()
                .map(|c| {
                    c.before.as_ref().map_or(0, String::len)
                        + c.after.as_ref().map_or(0, String::len)
                })
                .sum()
        };
        while self.undo.len() > 1
            && (self.undo.len() > UNDO_LIMIT
                || self.undo.iter().map(size).sum::<usize>() > UNDO_BYTES_LIMIT)
        {
            self.undo.remove(0);
        }
    }

    fn undo_redo(&mut self, is_undo: bool) {
        let (from, verb) = if is_undo {
            (&mut self.undo, "Undid")
        } else {
            (&mut self.redo, "Redid")
        };
        let Some(tx) = from.pop() else {
            let what = if is_undo { "undo" } else { "redo" };
            self.set_status(
                StatusKind::Info,
                format!("Nothing to {} in this session", what),
            );
            return;
        };
        match config_writer::revert(&tx.changes) {
            Ok(inverse) => {
                let to = if is_undo {
                    &mut self.redo
                } else {
                    &mut self.undo
                };
                to.push(Transaction {
                    label: tx.label.clone(),
                    changes: inverse,
                });
                self.refresh();
                self.set_status(StatusKind::Success, format!("{}: {}", verb, tx.label));
            }
            Err(e) => {
                self.refresh();
                self.set_status(
                    StatusKind::Error,
                    format!(
                        "Can't revert \"{}\": {} (dropped from history)",
                        tx.label, e
                    ),
                );
            }
        }
    }

    /// Called after the external editor returns.
    pub fn after_edit(&mut self, outcome: Result<(), String>) {
        self.refresh();
        match outcome {
            Ok(()) => self.set_status(StatusKind::Info, "Reloaded after edit"),
            Err(e) => self.set_status(StatusKind::Error, e),
        }
    }

    // -----------------------------------------------------------------------
    // Input
    // -----------------------------------------------------------------------

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        if key.kind != KeyEventKind::Press {
            return Action::None;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Action::Quit;
        }
        match self.mode {
            Mode::Normal => return self.handle_normal(key),
            Mode::AddWizard(_) => self.handle_add_wizard(key),
            Mode::RemoveConfirm(_) => self.handle_remove(key),
            Mode::SyncSelect(_) => self.handle_sync(key),
            Mode::Help => self.mode = Mode::Normal,
            Mode::Errors { ref mut scroll } => match key.code {
                KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => *scroll = scroll.saturating_add(1),
                _ => self.mode = Mode::Normal,
            },
        }
        Action::None
    }

    /// Bracketed paste: only meaningful in wizard text fields.
    pub fn handle_paste(&mut self, text: &str) {
        if let Mode::AddWizard(wiz) = &mut self.mode
            && wiz.is_text_step()
        {
            wiz.insert_str(text);
        }
    }

    fn handle_normal(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char('q') => return Action::Quit,
            KeyCode::Char('r') => {
                self.refresh();
                self.set_status(StatusKind::Info, "Rescanned configs");
            }
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Char('!') => {
                if self.result.errors.is_empty() {
                    self.set_status(StatusKind::Info, "No config errors");
                } else {
                    self.mode = Mode::Errors { scroll: 0 };
                }
            }
            KeyCode::Char('h') => self.check_selected(),
            KeyCode::Char('c') => self.check_all(),
            KeyCode::Up | KeyCode::Char('k') => self.move_by(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_by(1),
            KeyCode::Home | KeyCode::Char('g') => self.select(Some(0)),
            KeyCode::End | KeyCode::Char('G') => self.select(Some(usize::MAX)),
            KeyCode::PageUp => self.detail_scroll = self.detail_scroll.saturating_sub(5),
            KeyCode::PageDown => self.detail_scroll = self.detail_scroll.saturating_add(5),
            KeyCode::Char('a') => {
                let targets = self.available_targets();
                if targets.is_empty() {
                    self.set_status(StatusKind::Error, "No writable client configs available");
                } else {
                    self.mode = Mode::AddWizard(AddWizard::new(&targets));
                }
            }
            KeyCode::Char('d') => {
                if let Some(server) = self.selected_server().cloned() {
                    let instances = self.instances_of(&server.name);
                    self.mode = Mode::RemoveConfirm(RemoveConfirm::new(
                        server.name.clone(),
                        instances,
                        &server,
                    ));
                }
            }
            KeyCode::Char('s') => {
                if let Some(server) = self.selected_server().cloned() {
                    if !server.raw.is_object()
                        || server.raw.as_object().is_some_and(|m| m.is_empty())
                    {
                        self.set_status(StatusKind::Error, "Cannot sync an empty server entry");
                    } else {
                        let have: HashSet<ClientKind> = self
                            .instances_of(&server.name)
                            .iter()
                            .map(|s| s.client)
                            .collect();
                        let missing: Vec<ClientKind> = self
                            .available_targets()
                            .into_iter()
                            .filter(|c| !have.contains(c))
                            .collect();
                        if missing.is_empty() {
                            self.set_status(
                                StatusKind::Info,
                                "Server already in every available client",
                            );
                        } else {
                            self.mode = Mode::SyncSelect(SyncSelect::new(&server, missing));
                        }
                    }
                }
            }
            KeyCode::Char('u') => self.undo_redo(true),
            KeyCode::Char('U') => self.undo_redo(false),
            KeyCode::Char('e') => {
                if let Some(server) = self.selected_server() {
                    if server.client == ClientKind::ClaudeCodePlugin {
                        self.set_status(
                            StatusKind::Info,
                            "Plugin configs are managed by Claude Code — not opening",
                        );
                    } else if !server.source_path.is_file() {
                        let msg = format!(
                            "Config file doesn't exist: {}",
                            server.source_path.display()
                        );
                        self.set_status(StatusKind::Error, msg);
                    } else {
                        return Action::Edit(server.source_path.clone());
                    }
                }
            }
            _ => {}
        }
        Action::None
    }

    fn handle_add_wizard(&mut self, key: KeyEvent) {
        let Mode::AddWizard(wiz) = &mut self.mode else {
            return;
        };
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                return;
            }
            KeyCode::BackTab => {
                wiz.back();
                return;
            }
            _ => {}
        }

        if wiz.is_text_step() {
            match key.code {
                KeyCode::Char('u') if ctrl => wiz.clear_input(),
                KeyCode::Char('w') if ctrl => wiz.delete_word(),
                KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
                    wiz.insert_str(&c.to_string());
                }
                KeyCode::Backspace => wiz.backspace(),
                KeyCode::Enter => {
                    wiz.advance();
                }
                _ => {}
            }
            return;
        }

        match wiz.step {
            AddStep::TransportType => match key.code {
                KeyCode::Up | KeyCode::Char('k') => wiz.transport_up(),
                KeyCode::Down | KeyCode::Char('j') => wiz.transport_down(),
                KeyCode::Enter => {
                    wiz.advance();
                }
                _ => {}
            },
            AddStep::Clients => match key.code {
                KeyCode::Up | KeyCode::Char('k') => wiz.clients.up(),
                KeyCode::Down | KeyCode::Char('j') => wiz.clients.down(),
                KeyCode::Char(' ') => wiz.clients.toggle(),
                KeyCode::Char('a') => wiz.clients.toggle_all(),
                // Warn about real clobbers, checked against the exact write scope.
                KeyCode::Enter if wiz.advance() => {
                    let name = wiz.server_name();
                    let ctx = &self.ctx;
                    wiz.overwrite_clients = wiz
                        .clients
                        .selected()
                        .into_iter()
                        .filter(|c| config_writer::server_exists(*c, ctx, &name))
                        .collect();
                }
                _ => {}
            },
            AddStep::Confirm => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => self.execute_add(),
                KeyCode::Char('n') | KeyCode::Char('N') => self.mode = Mode::Normal,
                _ => {}
            },
            _ => {}
        }
    }

    fn execute_add(&mut self) {
        let Mode::AddWizard(wiz) = std::mem::take(&mut self.mode) else {
            return;
        };
        let name = wiz.server_name();
        let value = wiz.server_value();
        let clients = wiz.clients.selected();
        let label = format!("add \"{}\"", name);
        self.write_to_clients(&name, &value, &clients, "Added", label);
    }

    /// Shared by add and sync: write one entry to several clients, record the
    /// changes as one undoable step, and report every outcome.
    fn write_to_clients(
        &mut self,
        name: &str,
        value: &serde_json::Value,
        clients: &[ClientKind],
        verb: &str,
        label: String,
    ) {
        let mut changes = Vec::new();
        let mut overwrote = 0;
        let mut errors = Vec::new();
        for &client in clients {
            match config_writer::add_server(client, &self.ctx, name, value) {
                Ok(added) => {
                    overwrote += usize::from(added.overwrote);
                    changes.push(added.change);
                }
                Err(e) => errors.push(format!("{}: {}", client.label(), e)),
            }
        }
        let ok = changes.len();
        self.record(label, changes);
        self.refresh();
        self.select_by(|s| s.name == name);

        let mut parts = Vec::new();
        if ok > 0 {
            let mut s = format!("{} \"{}\" to {} client{}", verb, name, ok, plural(ok));
            if overwrote > 0 {
                s.push_str(&format!(" (overwrote {})", overwrote));
            }
            parts.push(s);
        }
        if !errors.is_empty() {
            parts.push(format!("failed: {}", errors.join("; ")));
        }
        let kind = if errors.is_empty() {
            StatusKind::Success
        } else {
            StatusKind::Error
        };
        self.set_status(kind, parts.join(" — "));
    }

    fn handle_remove(&mut self, key: KeyEvent) {
        let Mode::RemoveConfirm(rm) = &mut self.mode else {
            return;
        };
        if key.code == KeyCode::Esc {
            self.mode = Mode::Normal;
            return;
        }
        match rm.step {
            RemoveStep::SelectClients => match key.code {
                KeyCode::Up | KeyCode::Char('k') => rm.targets.up(),
                KeyCode::Down | KeyCode::Char('j') => rm.targets.down(),
                KeyCode::Char(' ') => rm.targets.toggle(),
                KeyCode::Char('a') => rm.targets.toggle_all(),
                KeyCode::Enter => {
                    if rm.targets.selected().is_empty() {
                        self.set_status(StatusKind::Error, "Select at least one config");
                    } else {
                        rm.step = RemoveStep::Confirm;
                    }
                }
                _ => {}
            },
            RemoveStep::Confirm => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => self.execute_remove(),
                KeyCode::Char('n') | KeyCode::Char('N') => self.mode = Mode::Normal,
                KeyCode::BackTab => rm.step = RemoveStep::SelectClients,
                _ => {}
            },
        }
    }

    fn execute_remove(&mut self) {
        let Mode::RemoveConfirm(rm) = std::mem::take(&mut self.mode) else {
            return;
        };
        let name = rm.server_name;
        let mut changes = Vec::new();
        let mut not_found = Vec::new();
        let mut errors = Vec::new();
        for server in rm.targets.selected() {
            match config_writer::remove_server(&server, &self.ctx) {
                Ok(Some(change)) => changes.push(change),
                Ok(None) => not_found.push(server.origin_label()),
                Err(e) => errors.push(format!("{}: {}", server.origin_label(), e)),
            }
        }
        let removed = changes.len();
        self.record(format!("remove \"{}\"", name), changes);
        self.refresh();

        let mut parts = Vec::new();
        if removed > 0 {
            parts.push(format!(
                "removed from {} config{}",
                removed,
                plural(removed)
            ));
        }
        if !not_found.is_empty() {
            parts.push(format!("already gone from {}", not_found.join(", ")));
        }
        if !errors.is_empty() {
            parts.push(format!("failed: {}", errors.join("; ")));
        }
        let kind = if !errors.is_empty() {
            StatusKind::Error
        } else if removed > 0 {
            StatusKind::Success
        } else {
            StatusKind::Info
        };
        self.set_status(kind, format!("\"{}\": {}", name, parts.join("; ")));
    }

    fn handle_sync(&mut self, key: KeyEvent) {
        let Mode::SyncSelect(sync) = &mut self.mode else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Up | KeyCode::Char('k') => sync.targets.up(),
            KeyCode::Down | KeyCode::Char('j') => sync.targets.down(),
            KeyCode::Char(' ') => sync.targets.toggle(),
            KeyCode::Char('a') => sync.targets.toggle_all(),
            KeyCode::Enter => {
                if sync.targets.selected().is_empty() {
                    self.set_status(StatusKind::Error, "Select at least one client (space)");
                } else {
                    self.execute_sync();
                }
            }
            _ => {}
        }
    }

    fn execute_sync(&mut self) {
        let Mode::SyncSelect(sync) = std::mem::take(&mut self.mode) else {
            return;
        };
        let clients = sync.targets.selected();
        let label = format!("sync \"{}\"", sync.server_name);
        self.write_to_clients(
            &sync.server_name,
            &sync.server_value,
            &clients,
            "Synced",
            label,
        );
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;
    use crossterm::event::KeyEvent;

    fn press(app: &mut App, code: KeyCode) -> Action {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn type_str(app: &mut App, s: &str) {
        for c in s.chars() {
            press(app, KeyCode::Char(c));
        }
    }

    #[test]
    fn add_sync_remove_undo_redo_end_to_end() {
        let t = TempDir::new();
        let ctx = t.ctx();
        std::fs::create_dir_all(ctx.cwd.join(".cursor")).unwrap();
        std::fs::write(
            ctx.cwd.join(".cursor/mcp.json"),
            r#"{"mcpServers":{"keep":{"command":"k","cwd":"/srv","timeout":30}}}"#,
        )
        .unwrap();
        let mut app = App::new(ctx.clone(), health::DEFAULT_TIMEOUT);
        assert_eq!(app.result.servers.len(), 1);

        // Add via the wizard (defaults to CC-Project).
        press(&mut app, KeyCode::Char('a'));
        type_str(&mut app, "new");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter); // stdio
        type_str(&mut app, "node srv.js");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter); // args prefilled
        press(&mut app, KeyCode::Enter); // no env
        press(&mut app, KeyCode::Enter); // clients
        press(&mut app, KeyCode::Char('y'));
        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(
            app.selected_server().unwrap().name,
            "new",
            "selection follows new server"
        );
        let mcp = ctx.cwd.join(".mcp.json");
        assert!(std::fs::read_to_string(&mcp).unwrap().contains("srv.js"));

        // Sync "keep" from Cursor-Project to CC-Project: unmodelled fields survive.
        app.select_by(|s| s.name == "keep");
        press(&mut app, KeyCode::Char('s'));
        let Mode::SyncSelect(sync) = &mut app.mode else {
            panic!("sync modal")
        };
        let pos = sync
            .targets
            .items
            .iter()
            .position(|(c, _)| *c == ClientKind::ClaudeCodeProject)
            .unwrap();
        sync.targets.cursor = pos;
        press(&mut app, KeyCode::Char(' '));
        press(&mut app, KeyCode::Enter);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&mcp).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["keep"]["timeout"], 30);
        assert_eq!(v["mcpServers"]["keep"]["cwd"], "/srv");

        // Remove pre-selects only the focused copy.
        app.select_by(|s| s.name == "keep" && s.client == ClientKind::ClaudeCodeProject);
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('y'));
        assert!(
            app.result
                .servers
                .iter()
                .any(|s| s.name == "keep" && s.client == ClientKind::CursorProject)
        );
        assert!(
            !app.result
                .servers
                .iter()
                .any(|s| s.name == "keep" && s.client == ClientKind::ClaudeCodeProject)
        );

        // Undo the remove, the sync, then the add (which deletes the created file).
        for _ in 0..3 {
            press(&mut app, KeyCode::Char('u'));
            assert_eq!(
                app.status().unwrap().kind,
                StatusKind::Success,
                "{}",
                app.status().unwrap().text
            );
        }
        assert!(!mcp.exists());
        press(&mut app, KeyCode::Char('U'));
        assert!(mcp.exists(), "redo re-creates the file");
    }

    #[test]
    fn health_survives_refresh_and_reorder() {
        let t = TempDir::new();
        let ctx = t.ctx();
        std::fs::write(
            ctx.cwd.join(".mcp.json"),
            r#"{"mcpServers":{"b":{"command":"mcpm-missing-bin"}}}"#,
        )
        .unwrap();
        let mut app = App::new(ctx.clone(), Duration::from_secs(2));
        press(&mut app, KeyCode::Char('h'));
        assert_eq!(app.checking_count(), 1);
        press(&mut app, KeyCode::Char('h'));
        assert_eq!(app.checking_count(), 1, "no duplicate check");
        // A new server sorts before "b" while the check is in flight.
        std::fs::write(
            ctx.cwd.join(".mcp.json"),
            r#"{"mcpServers":{"a":{"command":"x"},"b":{"command":"mcpm-missing-bin"}}}"#,
        )
        .unwrap();
        app.refresh();
        assert_eq!(app.selected_server().unwrap().name, "b");
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.checking_count() > 0 && Instant::now() < deadline {
            app.poll_health();
            std::thread::sleep(Duration::from_millis(20));
        }
        let a = app.result.servers[0].clone();
        let b = app.result.servers[1].clone();
        assert!(
            app.health_of(&a).is_none(),
            "result must not land on the wrong row"
        );
        assert!(matches!(
            app.health_of(&b).unwrap().status,
            HealthStatus::Error(_)
        ));
    }
}
