use std::path::PathBuf;
use std::sync::mpsc;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};

use crate::discovery::discover;
use crate::health;
use crate::types::{DiscoveryResult, HealthResult, HealthStatus, McpServer};

pub struct App {
    pub result: DiscoveryResult,
    pub selected: usize,
    pub scroll_offset: usize,
    pub show_errors: bool,
    pub cwd: PathBuf,
    pub health_tx: mpsc::Sender<HealthResult>,
    pub health_rx: mpsc::Receiver<HealthResult>,
    pub checking_count: usize,
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

    /// Start health check for the selected server
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

    /// Start health checks for all stdio servers
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

    /// Drain completed health check results from the channel
    pub fn poll_health(&mut self) {
        while let Ok(result) = self.health_rx.try_recv() {
            if let Some(server) = self.result.servers.get_mut(result.server_index) {
                server.health = result.status;
                server.last_checked = Some(result.checked_at);
            }
            self.checking_count = self.checking_count.saturating_sub(1);
        }
    }
}

/// Poll for input. Returns true if the app should exit.
pub fn handle_event(app: &mut App) -> std::io::Result<bool> {
    // Always drain health results
    app.poll_health();

    if event::poll(std::time::Duration::from_millis(200))? {
        if let Event::Key(key) = event::read()? {
            if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
                return Ok(true);
            }
            match key.code {
                KeyCode::Char('q') => return Ok(true),
                KeyCode::Char('r') => app.refresh(),
                KeyCode::Char('e') => app.show_errors = !app.show_errors,
                KeyCode::Char('h') => app.check_selected(),
                KeyCode::Char('H') => app.check_all(),
                KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                KeyCode::PageUp => app.scroll_detail_up(),
                KeyCode::PageDown => app.scroll_detail_down(),
                _ => {}
            }
        }
    }
    Ok(false)
}
