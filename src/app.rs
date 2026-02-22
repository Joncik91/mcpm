use std::path::PathBuf;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};

use crate::discovery::discover;
use crate::types::{DiscoveryResult, McpServer};

pub struct App {
    pub result: DiscoveryResult,
    pub selected: usize,
    pub scroll_offset: usize,
    pub show_errors: bool,
    pub cwd: PathBuf,
}

impl App {
    pub fn new(cwd: PathBuf) -> Self {
        let result = discover(&cwd);
        App {
            result,
            selected: 0,
            scroll_offset: 0,
            show_errors: false,
            cwd,
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
}

/// Poll for input. Returns true if the app should exit.
pub fn handle_event(app: &mut App) -> std::io::Result<bool> {
    if event::poll(std::time::Duration::from_millis(200))? {
        if let Event::Key(key) = event::read()? {
            if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
                return Ok(true);
            }
            match key.code {
                KeyCode::Char('q') => return Ok(true),
                KeyCode::Char('r') => app.refresh(),
                KeyCode::Char('e') => app.show_errors = !app.show_errors,
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
