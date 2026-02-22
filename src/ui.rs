use std::collections::BTreeSet;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table},
    Frame,
};

use crate::app::App;
use crate::types::Transport;

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // Compute matrix height: unique server names + 2 (border + header), capped at 12
    let unique_names: BTreeSet<&str> = app.result.servers.iter().map(|s| s.name.as_str()).collect();
    let matrix_height = if app.result.active_clients.is_empty() {
        3 // just the empty bordered box
    } else {
        (unique_names.len() + 3).min(14) as u16
    };

    let vertical = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(8),
        Constraint::Length(matrix_height),
    ])
    .split(area);

    render_header(f, vertical[0], app);
    render_main_panels(f, vertical[1], app);
    render_matrix(f, vertical[2], app);

    if app.show_errors && !app.result.errors.is_empty() {
        render_error_overlay(f, area, app);
    }
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let server_count = app.result.servers.len();
    let err_count = app.result.errors.len();
    let err_indicator = if err_count > 0 {
        format!(" [{} errors]", err_count)
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
        Span::raw(format!(
            " — {} server{}{}  ",
            server_count,
            if server_count == 1 { "" } else { "s" },
            err_indicator,
        )),
        Span::styled(
            "[r]efresh [e]rrors [q]uit [j/k]nav",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn render_main_panels(f: &mut Frame, area: Rect, app: &mut App) {
    let horizontal = Layout::horizontal([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    render_server_list(f, horizontal[0], app);
    render_detail(f, horizontal[1], app);
}

fn render_server_list(f: &mut Frame, area: Rect, app: &mut App) {
    let items: Vec<ListItem> = app
        .result
        .servers
        .iter()
        .map(|s| {
            let label = format!(" {:<18} {}", truncate(&s.name, 18), s.client.label());
            ListItem::new(label)
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
        None => vec![Line::from("  No servers found. Press [r] to refresh.")],
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
        for (k, v) in env {
            lines.push(indent_kv(k, v));
        }
    } else {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  No environment variables",
            Style::default().fg(Color::DarkGray),
        )));
    }

    lines
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

    // Deduplicate server names preserving discovery order
    let mut unique_names: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for s in &app.result.servers {
        if seen.insert(s.name.clone()) {
            unique_names.push(s.name.clone());
        }
    }

    // Build lookup: server_name -> set of clients
    let mut server_clients: std::collections::HashMap<&str, std::collections::HashSet<&crate::types::ClientKind>> =
        std::collections::HashMap::new();
    for s in &app.result.servers {
        server_clients
            .entry(&s.name)
            .or_default()
            .insert(&s.client);
    }

    // Header row
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

    // Data rows
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
            .title(" Parse Errors [e to close] ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red)),
    );

    f.render_widget(Clear, popup);
    f.render_widget(para, popup);
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
