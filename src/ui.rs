use std::collections::{HashMap, HashSet};
use std::time::Instant;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, TableState, Wrap,
    },
};

use crate::app::{App, HealthEntry, StatusKind};
use crate::shell;
use crate::types::{ClientKind, HealthStatus, McpServer, Transport};
use crate::wizard::*;

const ACCENT: Color = Color::Cyan;
const MUTED: Color = Color::DarkGray;

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let unique = unique_names(&app.result.servers).len();
    let matrix_height = if app.result.active_clients.is_empty() {
        3
    } else {
        // Leave the main panels at least ~10 rows on short terminals.
        let cap = area.height.saturating_sub(12).clamp(4, 14);
        (unique as u16 + 3).min(cap)
    };

    let [header, main, matrix, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(6),
        Constraint::Length(matrix_height),
        Constraint::Length(1),
    ])
    .areas(area);

    render_header(f, header, app);
    render_main_panels(f, main, app);
    render_matrix(f, matrix, app);
    render_status_bar(f, status, app);

    match &app.mode {
        Mode::AddWizard(wiz) => render_add_wizard(f, area, wiz),
        Mode::RemoveConfirm(rm) => render_remove_confirm(f, area, rm),
        Mode::SyncSelect(sync) => render_sync_select(f, area, sync),
        Mode::Help => render_help(f, area),
        Mode::Errors { scroll } => render_errors(f, area, app, *scroll),
        Mode::Normal => {}
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let n = app.result.servers.len();
    let mut spans = vec![
        Span::styled(
            " mcpm",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" v{}", env!("CARGO_PKG_VERSION")),
            Style::default().fg(MUTED),
        ),
        Span::raw(format!(" — {} server{}", n, if n == 1 { "" } else { "s" })),
    ];
    let errs = app.result.errors.len();
    if errs > 0 {
        spans.push(Span::styled(
            format!(
                "  [{} config error{} — press !]",
                errs,
                if errs == 1 { "" } else { "s" }
            ),
            Style::default().fg(Color::Red),
        ));
    }
    let checking = app.checking_count();
    if checking > 0 {
        spans.push(Span::styled(
            format!("  [checking {}…]", checking),
            Style::default().fg(Color::Yellow),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ---------------------------------------------------------------------------
// Status bar
// ---------------------------------------------------------------------------

fn render_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let line = if let Some(st) = app.status() {
        let color = match st.kind {
            StatusKind::Info => Color::White,
            StatusKind::Success => Color::Green,
            StatusKind::Error => Color::Red,
        };
        Line::from(Span::styled(
            format!(" {}", st.text),
            Style::default().fg(color),
        ))
    } else {
        let keys = match &app.mode {
            Mode::Normal => {
                " a:add  d:remove  s:sync  e:edit  u/U:undo/redo  h/c:check  r:refresh  ?:help  q:quit"
            }
            Mode::AddWizard(wiz) => match wiz.step {
                AddStep::TransportType => " j/k:select  enter:next  shift-tab:back  esc:cancel",
                AddStep::Clients => {
                    " space:toggle  a:all  j/k:move  enter:next  shift-tab:back  esc:cancel"
                }
                AddStep::Confirm => " y:confirm  n:cancel  shift-tab:back",
                AddStep::Headers | AddStep::EnvVars => {
                    " enter:add line (empty = next)  bksp on empty:edit last  shift-tab:back  esc:cancel"
                }
                _ => " enter:next  ctrl-u:clear  shift-tab:back  esc:cancel",
            },
            Mode::RemoveConfirm(rm) => match rm.step {
                RemoveStep::SelectClients => {
                    " space:toggle  a:all  j/k:move  enter:next  esc:cancel"
                }
                RemoveStep::Confirm => " y:confirm  n:cancel  shift-tab:back",
            },
            Mode::SyncSelect(_) => " space:toggle  a:all  j/k:move  enter:sync  esc:cancel",
            Mode::Help => " any key: close",
            Mode::Errors { .. } => " j/k:scroll  any other key: close",
        };
        Line::from(Span::styled(keys, Style::default().fg(MUTED)))
    };
    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(Color::Rgb(30, 30, 30))),
        area,
    );
}

// ---------------------------------------------------------------------------
// Main panels
// ---------------------------------------------------------------------------

fn render_main_panels(f: &mut Frame, area: Rect, app: &mut App) {
    let [list, detail] =
        Layout::horizontal([Constraint::Percentage(38), Constraint::Percentage(62)]).areas(area);
    render_server_list(f, list, app);
    render_detail(f, detail, app);
}

fn render_server_list(f: &mut Frame, area: Rect, app: &mut App) {
    // Width available for the name: borders (2) + highlight symbol (1) + client column.
    let name_w = (area.width as usize)
        .saturating_sub(2 + 1 + 1 + 11 + 2)
        .clamp(6, 40);
    let items: Vec<ListItem> = app
        .result
        .servers
        .iter()
        .map(|s| {
            let style = if s.disabled {
                Style::default().fg(MUTED)
            } else {
                Style::default()
            };
            let mut spans = vec![Span::styled(
                format!(
                    " {:<w$} {:<10}",
                    truncate(&s.name, name_w),
                    s.client.label(),
                    w = name_w
                ),
                style,
            )];
            if let Some(h) = app.health_of(s) {
                spans.push(Span::styled(
                    format!(" {}", h.status.symbol()),
                    Style::default().fg(health_color(&h.status)),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(panel(" Servers "))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸");
    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn render_detail(f: &mut Frame, area: Rect, app: &mut App) {
    let lines = match app.selected_server() {
        None => vec![
            Line::from(""),
            Line::from("  No MCP servers found."),
            Line::from(Span::styled(
                "  Press [a] to add one.",
                Style::default().fg(MUTED),
            )),
        ],
        Some(s) => build_detail_lines(s, app.health_of(s)),
    };
    let inner = Block::default().borders(Borders::ALL).inner(area);
    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    // Clamp scroll to the wrapped content so PgDn can't scroll into blank space.
    let max_scroll = (para.line_count(inner.width) as u16).saturating_sub(inner.height);
    app.detail_scroll = app.detail_scroll.min(max_scroll);
    let title = if max_scroll > 0 {
        " Detail (PgUp/PgDn) "
    } else {
        " Detail "
    };
    f.render_widget(
        para.block(panel(title)).scroll((app.detail_scroll, 0)),
        area,
    );
}

fn build_detail_lines(s: &McpServer, health: Option<&HealthEntry>) -> Vec<Line<'static>> {
    let mut lines = vec![
        kv_line("Name", &s.name),
        kv_line("Client", &s.origin_label()),
        kv_line("Source", &s.source_path.display().to_string()),
        kv_line("Transport", s.transport.kind_label()),
    ];
    if s.disabled {
        lines.push(Line::from(vec![
            Span::styled(format!("  {:<12}", "Status"), key_style()),
            Span::styled("disabled", Style::default().fg(Color::Yellow)),
        ]));
    }

    match &s.transport {
        Transport::Http { url, headers } | Transport::Sse { url, headers } => {
            lines.push(kv_line("URL", url));
            if !headers.is_empty() {
                lines.push(section_line("Headers"));
                // Header values are usually credentials: mask like env.
                lines.extend(headers.keys().map(|k| indent_kv(k, "***")));
            }
        }
        Transport::Stdio { command, args } => {
            lines.push(kv_line("Command", command));
            if !args.is_empty() {
                lines.push(kv_line("Args", &shell::join(args)));
            }
        }
        Transport::Unknown => {}
    }

    if !s.env.is_empty() {
        lines.push(section_line("Environment"));
        lines.extend(s.env.keys().map(|k| indent_kv(k, "***")));
    }
    let extra = s.extra_fields();
    if !extra.is_empty() {
        lines.push(kv_line("Other keys", &extra.join(", ")));
    }

    lines.push(Line::from(""));
    match health {
        Some(h) => {
            lines.push(Line::from(vec![
                Span::styled(format!("  {:<12}", "Health"), key_style()),
                Span::styled(
                    format!("{} {}", h.status.symbol(), h.status.label()),
                    Style::default().fg(health_color(&h.status)),
                ),
            ]));
            if let HealthStatus::Healthy {
                protocol_version: Some(p),
                ..
            } = &h.status
            {
                lines.push(kv_line("Protocol", p));
            }
            let checked = h.checked_at.map_or("never".to_string(), format_elapsed);
            lines.push(kv_line("Checked", &checked));
        }
        None if s.transport.is_stdio() => {
            lines.push(kv_line("Health", "unchecked"));
            lines.push(Line::from(Span::styled(
                "  Press [h] to health check this server",
                Style::default().fg(MUTED),
            )));
        }
        None => lines.push(Line::from(Span::styled(
            "  Health checks only available for stdio servers",
            Style::default().fg(MUTED),
        ))),
    }
    lines
}

// ---------------------------------------------------------------------------
// Modal: Add Wizard
// ---------------------------------------------------------------------------

fn render_add_wizard(f: &mut Frame, area: Rect, wiz: &AddWizard) {
    let mut lines: Vec<Line> = vec![Line::from("")];

    match wiz.step {
        AddStep::Name | AddStep::Command | AddStep::Args | AddStep::Url => {
            let label = match wiz.step {
                AddStep::Name => "Server name",
                AddStep::Command => "Command (e.g. npx, node, uvx — arguments may follow)",
                AddStep::Args => "Arguments (quote values with spaces; empty for none)",
                _ => "URL (e.g. https://example.com/mcp)",
            };
            lines.push(prompt_line(label));
            lines.push(input_line(wiz.input()));
        }
        AddStep::TransportType => {
            lines.push(prompt_line("Select transport type:"));
            lines.push(Line::from(""));
            for t in TransportKind::ALL {
                lines.push(choice_line(t == wiz.transport, None, t.describe()));
            }
        }
        AddStep::Headers | AddStep::EnvVars => {
            let (label, map, sep) = if wiz.step == AddStep::Headers {
                (
                    "HTTP headers (Header: value, empty line to continue):",
                    &wiz.headers,
                    ": ",
                )
            } else {
                (
                    "Environment variables (KEY=VALUE, empty line to continue):",
                    &wiz.env,
                    "=",
                )
            };
            lines.push(prompt_line(label));
            lines.push(Line::from(""));
            for (k, v) in map {
                lines.push(Line::from(Span::styled(
                    format!("  {}{}{}", k, sep, mask(v)),
                    Style::default().fg(Color::Green),
                )));
            }
            lines.push(input_line(wiz.input()));
        }
        AddStep::Clients => {
            lines.push(prompt_line("Install to:"));
            lines.push(Line::from(""));
            for (i, (client, on)) in wiz.clients.items.iter().enumerate() {
                lines.push(choice_line(
                    i == wiz.clients.cursor,
                    Some(*on),
                    client.label(),
                ));
            }
        }
        AddStep::Confirm => {
            let name = wiz.server_name();
            lines.push(Line::from(Span::styled(
                format!("  Add \"{}\" to:", name),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )));
            for client in wiz.clients.selected() {
                lines.push(bullet(client.label(), Color::Green));
            }
            lines.push(Line::from(""));
            match wiz.transport_value() {
                Transport::Stdio { command, args } => {
                    lines.push(Line::from(format!("  Command: {}", command)));
                    if !args.is_empty() {
                        lines.push(Line::from(format!("  Args:    {}", shell::join(&args))));
                    }
                    if !wiz.env.is_empty() {
                        lines.push(Line::from(format!(
                            "  Env:     {}",
                            wiz.env.keys().cloned().collect::<Vec<_>>().join(", ")
                        )));
                    }
                }
                t @ (Transport::Http { .. } | Transport::Sse { .. }) => {
                    lines.push(Line::from(format!("  Type:    {}", t.kind_label())));
                    lines.push(Line::from(format!("  URL:     {}", wiz.url)));
                    if !wiz.headers.is_empty() {
                        lines.push(Line::from(format!(
                            "  Headers: {}",
                            wiz.headers.keys().cloned().collect::<Vec<_>>().join(", ")
                        )));
                    }
                }
                Transport::Unknown => {}
            }
            if !wiz.overwrite_clients.is_empty() {
                let targets: Vec<&str> = wiz
                    .overwrite_clients
                    .iter()
                    .map(ClientKind::label)
                    .collect();
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!(
                        "  ⚠ Overwrites existing \"{}\" in: {}",
                        name,
                        targets.join(", ")
                    ),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  A .bak copy is kept; [u] undoes this.",
                Style::default().fg(MUTED),
            )));
        }
    }

    if let Some(err) = &wiz.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  ⚠ {}", err),
            Style::default().fg(Color::Red),
        )));
    }

    modal(
        f,
        area,
        format!(" Add Server — {} ", wiz.step_label()),
        Color::Yellow,
        lines,
    );
}

// ---------------------------------------------------------------------------
// Modal: Remove Confirm
// ---------------------------------------------------------------------------

fn render_remove_confirm(f: &mut Frame, area: Rect, rm: &RemoveConfirm) {
    let mut lines: Vec<Line> = vec![Line::from("")];
    match rm.step {
        RemoveStep::SelectClients => {
            lines.push(prompt_line("Remove from:"));
            lines.push(Line::from(""));
            for (i, (s, on)) in rm.targets.items.iter().enumerate() {
                lines.push(choice_line(
                    i == rm.targets.cursor,
                    Some(*on),
                    &s.origin_label(),
                ));
            }
        }
        RemoveStep::Confirm => {
            lines.push(Line::from(Span::styled(
                format!("  Remove \"{}\" from:", rm.server_name),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )));
            for s in rm.targets.selected() {
                lines.push(bullet(
                    &format!("{}  {}", s.origin_label(), s.source_path.display()),
                    Color::Red,
                ));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  A .bak copy is kept; [u] undoes this.",
                Style::default().fg(MUTED),
            )));
        }
    }
    modal(
        f,
        area,
        format!(" Remove \"{}\" ", rm.server_name),
        Color::Red,
        lines,
    );
}

// ---------------------------------------------------------------------------
// Modal: Sync Select
// ---------------------------------------------------------------------------

fn render_sync_select(f: &mut Frame, area: Rect, sync: &SyncSelect) {
    let mut lines: Vec<Line> = vec![
        Line::from(""),
        prompt_line(&format!("Copy the {} entry to:", sync.source_label)),
        Line::from(""),
    ];
    for (i, (client, on)) in sync.targets.items.iter().enumerate() {
        lines.push(choice_line(
            i == sync.targets.cursor,
            Some(*on),
            client.label(),
        ));
    }
    modal(
        f,
        area,
        format!(" Sync \"{}\" ", sync.server_name),
        Color::Magenta,
        lines,
    );
}

// ---------------------------------------------------------------------------
// Modal: Help & Errors
// ---------------------------------------------------------------------------

fn render_help(f: &mut Frame, area: Rect) {
    const KEYS: &[(&str, &str)] = &[
        ("j/k ↑/↓", "move selection"),
        ("g/G", "first / last server"),
        ("PgUp/PgDn", "scroll detail panel"),
        ("a", "add a server (wizard)"),
        ("d", "remove server from configs"),
        ("s", "copy server to other clients"),
        ("e", "open config in $VISUAL/$EDITOR"),
        ("u / U", "undo / redo (this session)"),
        ("h / c", "health check selected / all stdio"),
        ("!", "show config errors"),
        ("r", "rescan configs"),
        ("q, Ctrl-C", "quit"),
    ];
    let mut lines = vec![Line::from("")];
    lines.extend(KEYS.iter().map(|(k, d)| {
        Line::from(vec![
            Span::styled(format!("  {:<12}", k), key_style()),
            Span::raw(*d),
        ])
    }));
    modal(f, area, " Keys ".to_string(), ACCENT, lines);
}

fn render_errors(f: &mut Frame, area: Rect, app: &App, scroll: u16) {
    let mut lines = vec![Line::from("")];
    for e in &app.result.errors {
        lines.push(Line::from(Span::styled(
            format!("  {}", e),
            Style::default().fg(Color::Red),
        )));
        lines.push(Line::from(""));
    }
    let popup = centered_rect(80, 60, area);
    f.render_widget(Clear, popup);
    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(" Config Errors ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red)),
        )
        .scroll((scroll, 0));
    f.render_widget(para, popup);
}

// ---------------------------------------------------------------------------
// Matrix
// ---------------------------------------------------------------------------

fn render_matrix(f: &mut Frame, area: Rect, app: &App) {
    let clients = &app.result.active_clients;
    if clients.is_empty() {
        let para = Paragraph::new("  No servers discovered across any client.")
            .block(panel(" Client Matrix "))
            .style(Style::default().fg(MUTED));
        f.render_widget(para, area);
        return;
    }

    let names = unique_names(&app.result.servers);
    let mut present: HashMap<&str, HashSet<ClientKind>> = HashMap::new();
    for s in &app.result.servers {
        present.entry(&s.name).or_default().insert(s.client);
    }

    let header = Row::new(
        std::iter::once(Cell::from("")).chain(clients.iter().map(|c| {
            Cell::from(c.label()).style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
        })),
    );
    let rows: Vec<Row> = names
        .iter()
        .map(|name| {
            let have = present.get(name);
            Row::new(
                std::iter::once(Cell::from(truncate(name, 20))).chain(clients.iter().map(|c| {
                    if have.is_some_and(|h| h.contains(c)) {
                        Cell::from(" ✓").style(Style::default().fg(Color::Green))
                    } else {
                        Cell::from(" ·").style(Style::default().fg(MUTED))
                    }
                })),
            )
        })
        .collect();
    let widths = std::iter::once(Constraint::Length(22))
        .chain(std::iter::repeat_n(Constraint::Length(11), clients.len()));

    // Follow the selected server so its row is always visible.
    let mut state = TableState::default();
    let sel = app.selected_server().map(|s| s.name.as_str());
    state.select(sel.and_then(|n| names.iter().position(|x| *x == n)));

    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(ACCENT))
        .block(panel(" Client Matrix "));
    f.render_stateful_widget(table, area, &mut state);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Server names in first-seen order.
fn unique_names(servers: &[McpServer]) -> Vec<&str> {
    let mut seen = HashSet::new();
    servers
        .iter()
        .map(|s| s.name.as_str())
        .filter(|n| seen.insert(*n))
        .collect()
}

fn panel(title: &str) -> Block<'_> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
}

fn modal(f: &mut Frame, area: Rect, title: String, color: Color, lines: Vec<Line>) {
    let popup = centered_rect(64, 64, area);
    f.render_widget(Clear, popup);
    let para = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color)),
    );
    f.render_widget(para, popup);
}

fn key_style() -> Style {
    Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}

fn prompt_line(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {}", text),
        Style::default().fg(Color::Yellow),
    ))
}

fn input_line(text: &str) -> Line<'static> {
    Line::from(vec![
        Span::raw("  > "),
        Span::styled(text.to_string(), Style::default().fg(Color::White)),
        Span::styled("█", Style::default().fg(ACCENT)),
    ])
}

fn choice_line(focused: bool, checked: Option<bool>, label: &str) -> Line<'static> {
    let cursor = if focused { "▸" } else { " " };
    let check = match checked {
        Some(true) => "[x] ",
        Some(false) => "[ ] ",
        None => "",
    };
    let style = if focused {
        Style::default().fg(ACCENT)
    } else {
        Style::default()
    };
    Line::from(Span::styled(
        format!("  {} {}{}", cursor, check, label),
        style,
    ))
}

fn bullet(text: &str, color: Color) -> Line<'static> {
    Line::from(Span::styled(
        format!("    • {}", text),
        Style::default().fg(color),
    ))
}

fn mask(v: &str) -> String {
    if v.is_empty() {
        String::new()
    } else {
        "***".to_string()
    }
}

fn health_color(status: &HealthStatus) -> Color {
    match status {
        HealthStatus::Checking | HealthStatus::Timeout(_) => Color::Yellow,
        HealthStatus::Healthy { .. } => Color::Green,
        HealthStatus::Error(_) => Color::Red,
    }
}

fn format_elapsed(since: Instant) -> String {
    let secs = since.elapsed().as_secs();
    match secs {
        0..60 => format!("{}s ago", secs),
        60..3600 => format!("{}m ago", secs / 60),
        _ => format!("{}h ago", secs / 3600),
    }
}

fn kv_line(key: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {:<12}", key), key_style()),
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
    let [_, v, _] = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .areas(r);
    let [_, h, _] = Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .areas(v);
    h
}

/// Char-boundary-safe truncation with an ellipsis.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::DEFAULT_TIMEOUT;
    use crate::testutil::TempDir;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn truncate_is_utf8_safe() {
        assert_eq!(truncate("日本語サーバー", 4), "日本語…");
        assert_eq!(truncate("ab", 0), "…");
        assert_eq!(truncate("ab", 2), "ab");
    }

    #[test]
    fn renders_without_panicking_at_any_size_and_mode() {
        let t = TempDir::new();
        let ctx = t.ctx();
        std::fs::write(
            ctx.cwd.join(".mcp.json"),
            r#"{"mcpServers":{"日本語-server-with-a-really-long-name":{"type":"http","url":"https://x","headers":{"Authorization":"Bearer SECRET"}},"s":{"command":"x","env":{"K":"SECRET"}}}}"#,
        )
        .unwrap();
        let mut app = App::new(ctx, DEFAULT_TIMEOUT);
        app.result.errors.push("bad file".into());
        let modes: Vec<fn(&App) -> Mode> = vec![
            |_| Mode::Normal,
            |_| Mode::Help,
            |_| Mode::Errors { scroll: 3 },
            |a| Mode::AddWizard(AddWizard::new(&a.available_targets())),
            |a| {
                let s = a.result.servers[0].clone();
                Mode::RemoveConfirm(RemoveConfirm::new(s.name.clone(), vec![s.clone()], &s))
            },
            |a| {
                Mode::SyncSelect(SyncSelect::new(
                    &a.result.servers[0],
                    vec![ClientKind::CursorGlobal],
                ))
            },
        ];
        for (w, h) in [(1, 1), (20, 8), (80, 24), (200, 60)] {
            let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
            for m in &modes {
                app.mode = m(&app);
                app.detail_scroll = u16::MAX;
                term.draw(|f| render(f, &mut app)).unwrap();
                let buf = format!("{:?}", term.backend().buffer());
                assert!(!buf.contains("SECRET"), "secret leaked at {}x{}", w, h);
            }
        }
    }
}
