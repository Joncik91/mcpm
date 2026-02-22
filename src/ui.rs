use std::collections::BTreeSet;
use std::time::Instant;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table},
    Frame,
};

use crate::app::App;
use crate::types::{HealthStatus, Transport};
use crate::wizard::*;

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let unique_names: BTreeSet<&str> =
        app.result.servers.iter().map(|s| s.name.as_str()).collect();
    let matrix_height = if app.result.active_clients.is_empty() {
        3
    } else {
        (unique_names.len() + 3).min(14) as u16
    };

    let vertical = Layout::vertical([
        Constraint::Length(1),               // header
        Constraint::Min(8),                  // main panels
        Constraint::Length(matrix_height),   // matrix
        Constraint::Length(1),               // status bar
    ])
    .split(area);

    render_header(f, vertical[0], app);
    render_main_panels(f, vertical[1], app);
    render_matrix(f, vertical[2], app);
    render_status_bar(f, vertical[3], app);

    // Error overlay
    if app.show_errors && !app.result.errors.is_empty() {
        render_error_overlay(f, area, app);
    }

    // Modal overlays
    match &app.mode {
        Mode::AddWizard(wiz) => render_add_wizard(f, area, wiz),
        Mode::RemoveConfirm(rm) => render_remove_confirm(f, area, rm),
        Mode::SyncSelect(sync) => render_sync_select(f, area, sync),
        Mode::Normal => {}
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let server_count = app.result.servers.len();
    let err_count = app.result.errors.len();
    let err_indicator = if err_count > 0 {
        format!(" [{} errors]", err_count)
    } else {
        String::new()
    };
    let checking = if app.checking_count > 0 {
        format!(" [checking {}...]", app.checking_count)
    } else {
        String::new()
    };

    let line = Line::from(vec![
        Span::styled(
            " mcpm",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " v1.2.0",
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(format!(
            " — {} server{}{}{}",
            server_count,
            if server_count == 1 { "" } else { "s" },
            err_indicator,
            checking,
        )),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

// ---------------------------------------------------------------------------
// Status bar
// ---------------------------------------------------------------------------

fn render_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let line = if let Some(msg) = &app.status_message {
        Line::from(Span::styled(
            format!(" {}", msg),
            Style::default().fg(Color::Green),
        ))
    } else {
        let keys = match &app.mode {
            Mode::Normal => {
                " a:add  d:remove  s:sync  e:edit  h:check  H:all  !:errors  r:refresh  q:quit"
            }
            Mode::AddWizard(wiz) => match wiz.step {
                AddStep::Clients => " space:toggle  j/k:move  enter:next  esc:cancel",
                AddStep::Confirm => " y:confirm  n:cancel  esc:cancel",
                _ => " enter:next  esc:cancel",
            },
            Mode::RemoveConfirm(rm) => match rm.step {
                RemoveStep::SelectClients => " space:toggle  j/k:move  enter:next  esc:cancel",
                RemoveStep::Confirm => " y:confirm  n:cancel  esc:cancel",
            },
            Mode::SyncSelect(_) => " space:toggle  j/k:move  enter:sync  esc:cancel",
        };
        Line::from(Span::styled(keys, Style::default().fg(Color::DarkGray)))
    };
    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(Color::Rgb(30, 30, 30))),
        area,
    );
}

// ---------------------------------------------------------------------------
// Main panels (unchanged from v1.1.0 except env masking)
// ---------------------------------------------------------------------------

fn render_main_panels(f: &mut Frame, area: Rect, app: &mut App) {
    let horizontal =
        Layout::horizontal([Constraint::Percentage(35), Constraint::Percentage(65)]).split(area);
    render_server_list(f, horizontal[0], app);
    render_detail(f, horizontal[1], app);
}

fn render_server_list(f: &mut Frame, area: Rect, app: &mut App) {
    let items: Vec<ListItem> = app
        .result
        .servers
        .iter()
        .map(|s| {
            let health_sym = s.health.symbol();
            let health_color = health_color(&s.health);
            let mut spans = vec![Span::raw(format!(
                " {:<18} {:<10}",
                truncate(&s.name, 18),
                s.client.label()
            ))];
            if !health_sym.is_empty() {
                spans.push(Span::styled(
                    format!(" {}", health_sym),
                    Style::default().fg(health_color),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Servers ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸");

    let mut state = ListState::default();
    state.select(Some(app.selected));
    f.render_stateful_widget(list, area, &mut state);
}

fn render_detail(f: &mut Frame, area: Rect, app: &App) {
    let lines = match app.selected_server() {
        None => vec![Line::from("  No servers found. Press [a] to add one.")],
        Some(s) => build_detail_lines(s),
    };
    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Detail ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .scroll((app.scroll_offset as u16, 0));
    f.render_widget(para, area);
}

fn build_detail_lines(s: &crate::types::McpServer) -> Vec<Line<'static>> {
    let mut lines = vec![
        kv_line("Name", &s.name),
        kv_line("Client", s.client.label()),
        kv_line("Source", &s.source_path),
        kv_line("Transport", s.transport.kind_label()),
    ];

    match &s.transport {
        Transport::Http { url, headers } => {
            lines.push(kv_line("URL", url));
            if let Some(h) = headers {
                lines.push(section_line("Headers"));
                for (k, v) in h {
                    lines.push(indent_kv(k, v));
                }
            }
        }
        Transport::Sse { url } => {
            lines.push(kv_line("URL", url));
        }
        Transport::Stdio { command, args } => {
            lines.push(kv_line("Command", command));
            if !args.is_empty() {
                lines.push(kv_line("Args", &args.join(" ")));
            }
        }
        Transport::Unknown => {}
    }

    if let Some(env) = &s.env {
        lines.push(section_line("Environment"));
        for (k, _) in env {
            lines.push(indent_kv(k, "***"));
        }
    }

    lines.push(Line::from(""));
    let color = health_color(&s.health);
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {:<12}", "Health"),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} {}", s.health.symbol(), s.health.label()),
            Style::default().fg(color),
        ),
    ]));

    let checked_text = match s.last_checked {
        Some(t) => format_elapsed(t),
        None => "never".to_string(),
    };
    lines.push(kv_line("Checked", &checked_text));

    if let HealthStatus::Healthy {
        server_name,
        server_version,
    } = &s.health
    {
        lines.push(kv_line("Server", &format!("{} v{}", server_name, server_version)));
    }

    if s.transport.is_stdio() && matches!(s.health, HealthStatus::Unchecked) {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Press [h] to health check this server",
            Style::default().fg(Color::DarkGray),
        )));
    } else if !s.transport.is_stdio() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Health checks only available for stdio servers",
            Style::default().fg(Color::DarkGray),
        )));
    }

    lines
}

// ---------------------------------------------------------------------------
// Modal: Add Wizard
// ---------------------------------------------------------------------------

fn render_add_wizard(f: &mut Frame, area: Rect, wiz: &AddWizard) {
    let popup = centered_rect(60, 60, area);
    f.render_widget(Clear, popup);

    let title = format!(" Add Server — {} ", wiz.step_label());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let mut lines: Vec<Line> = vec![Line::from("")];

    match wiz.step {
        AddStep::Name | AddStep::Command | AddStep::Args => {
            let label = match wiz.step {
                AddStep::Name => "Server name",
                AddStep::Command => "Command (e.g. npx, node, uvx)",
                AddStep::Args => "Arguments (space-separated, or empty)",
                _ => "",
            };
            lines.push(Line::from(Span::styled(
                format!("  {}:", label),
                Style::default().fg(Color::Yellow),
            )));
            lines.push(Line::from(vec![
                Span::raw("  > "),
                Span::styled(
                    wiz.current_input().to_string(),
                    Style::default().fg(Color::White),
                ),
                Span::styled("█", Style::default().fg(Color::Cyan)),
            ]));
        }
        AddStep::EnvVars => {
            lines.push(Line::from(Span::styled(
                "  Environment variables (KEY=VALUE, empty line to skip):",
                Style::default().fg(Color::Yellow),
            )));
            lines.push(Line::from(""));
            for env_line in &wiz.env_lines {
                lines.push(Line::from(Span::styled(
                    format!("  {}", env_line),
                    Style::default().fg(Color::Green),
                )));
            }
            lines.push(Line::from(vec![
                Span::raw("  > "),
                Span::styled(wiz.env_input.clone(), Style::default().fg(Color::White)),
                Span::styled("█", Style::default().fg(Color::Cyan)),
            ]));
        }
        AddStep::Clients => {
            lines.push(Line::from(Span::styled(
                "  Install to:",
                Style::default().fg(Color::Yellow),
            )));
            lines.push(Line::from(""));
            for (i, (client, selected)) in wiz.clients.iter().enumerate() {
                let check = if *selected { "x" } else { " " };
                let cursor = if i == wiz.cursor { "▸" } else { " " };
                let style = if i == wiz.cursor {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default()
                };
                lines.push(Line::from(Span::styled(
                    format!("  {} [{}] {}", cursor, check, client.label()),
                    style,
                )));
            }
        }
        AddStep::Confirm => {
            lines.push(Line::from(Span::styled(
                format!("  Add \"{}\" to:", wiz.name),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )));
            for (client, selected) in &wiz.clients {
                if *selected {
                    lines.push(Line::from(Span::styled(
                        format!("    • {}", client.label()),
                        Style::default().fg(Color::Green),
                    )));
                }
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::raw(format!(
                "  Command: {}",
                wiz.command
            ))));
            if !wiz.args.is_empty() {
                lines.push(Line::from(Span::raw(format!("  Args:    {}", wiz.args))));
            }
            if !wiz.env_lines.is_empty() {
                lines.push(Line::from(Span::raw(format!(
                    "  Env:     {} var{}",
                    wiz.env_lines.len(),
                    if wiz.env_lines.len() == 1 { "" } else { "s" }
                ))));
            }
        }
    }

    // Show error if any
    if let Some(err) = &wiz.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  ⚠ {}", err),
            Style::default().fg(Color::Red),
        )));
    }

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, popup);
}

// ---------------------------------------------------------------------------
// Modal: Remove Confirm
// ---------------------------------------------------------------------------

fn render_remove_confirm(f: &mut Frame, area: Rect, rm: &RemoveConfirm) {
    let popup = centered_rect(55, 50, area);
    f.render_widget(Clear, popup);

    let title = format!(" Remove \"{}\" ", rm.server_name);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));

    let mut lines: Vec<Line> = vec![Line::from("")];

    match rm.step {
        RemoveStep::SelectClients => {
            lines.push(Line::from(Span::styled(
                "  Remove from:",
                Style::default().fg(Color::Yellow),
            )));
            lines.push(Line::from(""));
            for (i, (client, selected)) in rm.clients.iter().enumerate() {
                let check = if *selected { "x" } else { " " };
                let cursor = if i == rm.cursor { "▸" } else { " " };
                let style = if i == rm.cursor {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default()
                };
                lines.push(Line::from(Span::styled(
                    format!("  {} [{}] {}", cursor, check, client.label()),
                    style,
                )));
            }
        }
        RemoveStep::Confirm => {
            lines.push(Line::from(Span::styled(
                format!("  Remove \"{}\" from:", rm.server_name),
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            )));
            for (client, selected) in &rm.clients {
                if *selected {
                    lines.push(Line::from(Span::styled(
                        format!("    • {}", client.label()),
                        Style::default().fg(Color::Red),
                    )));
                }
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  This will modify config files. Backups will be created.",
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, popup);
}

// ---------------------------------------------------------------------------
// Modal: Sync Select
// ---------------------------------------------------------------------------

fn render_sync_select(f: &mut Frame, area: Rect, sync: &SyncSelect) {
    let popup = centered_rect(55, 50, area);
    f.render_widget(Clear, popup);

    let title = format!(" Sync \"{}\" ", sync.server_name);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Copy to:",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
    ];

    if sync.targets.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No additional clients available",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (i, (client, selected)) in sync.targets.iter().enumerate() {
            let check = if *selected { "x" } else { " " };
            let cursor = if i == sync.cursor { "▸" } else { " " };
            let style = if i == sync.cursor {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };
            lines.push(Line::from(Span::styled(
                format!("  {} [{}] {}", cursor, check, client.label()),
                style,
            )));
        }
    }

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, popup);
}

// ---------------------------------------------------------------------------
// Matrix (unchanged)
// ---------------------------------------------------------------------------

fn render_matrix(f: &mut Frame, area: Rect, app: &App) {
    let clients = &app.result.active_clients;

    if clients.is_empty() {
        let block = Block::default()
            .title(" Client Matrix ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
        let para = Paragraph::new("  No servers discovered across any client.")
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(para, area);
        return;
    }

    let mut unique_names: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for s in &app.result.servers {
        if seen.insert(s.name.clone()) {
            unique_names.push(s.name.clone());
        }
    }

    let mut server_clients: std::collections::HashMap<
        &str,
        std::collections::HashSet<&crate::types::ClientKind>,
    > = std::collections::HashMap::new();
    for s in &app.result.servers {
        server_clients
            .entry(&s.name)
            .or_default()
            .insert(&s.client);
    }

    let header_cells: Vec<Cell> = std::iter::once(Cell::from(""))
        .chain(clients.iter().map(|c| {
            Cell::from(c.label()).style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        }))
        .collect();
    let header = Row::new(header_cells);

    let rows: Vec<Row> = unique_names
        .iter()
        .map(|name| {
            let client_set = server_clients.get(name.as_str());
            let cells: Vec<Cell> = std::iter::once(
                Cell::from(truncate(name, 20)).style(Style::default().fg(Color::White)),
            )
            .chain(clients.iter().map(|c| {
                if client_set.is_some_and(|cs| cs.contains(c)) {
                    Cell::from(" ✓").style(Style::default().fg(Color::Green))
                } else {
                    Cell::from(" ·").style(Style::default().fg(Color::DarkGray))
                }
            }))
            .collect();
            Row::new(cells)
        })
        .collect();

    let widths: Vec<Constraint> = std::iter::once(Constraint::Length(22))
        .chain(std::iter::repeat_n(
            Constraint::Length(11),
            clients.len(),
        ))
        .collect();

    let table = Table::new(rows, widths).header(header).block(
        Block::default()
            .title(" Client Matrix ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    f.render_widget(table, area);
}

// ---------------------------------------------------------------------------
// Error overlay
// ---------------------------------------------------------------------------

fn render_error_overlay(f: &mut Frame, area: Rect, app: &App) {
    let popup = centered_rect(70, 50, area);

    let lines: Vec<Line> = std::iter::once(Line::from(""))
        .chain(app.result.errors.iter().map(|e| {
            Line::from(Span::styled(
                format!("  {}", e),
                Style::default().fg(Color::Red),
            ))
        }))
        .collect();

    let para = Paragraph::new(lines).block(
        Block::default()
            .title(" Parse Errors [! to close] ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red)),
    );

    f.render_widget(Clear, popup);
    f.render_widget(para, popup);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn health_color(status: &HealthStatus) -> Color {
    match status {
        HealthStatus::Unchecked => Color::DarkGray,
        HealthStatus::Checking => Color::Yellow,
        HealthStatus::Healthy { .. } => Color::Green,
        HealthStatus::Timeout => Color::Yellow,
        HealthStatus::Error(_) => Color::Red,
    }
}

fn format_elapsed(since: Instant) -> String {
    let secs = since.elapsed().as_secs();
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else {
        format!("{}h ago", secs / 3600)
    }
}

fn kv_line(key: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {:<12}", key),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(value.to_string()),
    ])
}

fn section_line(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {}:", title),
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    ))
}

fn indent_kv(key: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("    {}: ", key), Style::default().fg(Color::Gray)),
        Span::raw(value.to_string()),
    ])
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let v = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(v[1])[1]
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}
