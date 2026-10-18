use std::io::{self, IsTerminal};
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
use crossterm::{
    cursor::Show,
    event::{self, DisableBracketedPaste, EnableBracketedPaste, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

mod app;
mod config_writer;
mod discovery;
mod health;
mod jsonc;
mod scope;
mod shell;
#[cfg(test)]
mod testutil;
mod types;
mod ui;
mod wizard;

use types::{Ctx, HealthStatus};

#[derive(Parser)]
#[command(
    name = "mcpm",
    version,
    about = "MCP Server Manager — see all your MCP servers across all clients"
)]
struct Cli {
    /// Health check timeout in seconds (TUI and `check`)
    #[arg(long, global = true, default_value_t = health::DEFAULT_TIMEOUT.as_secs_f64(), value_parser = parse_timeout)]
    timeout: f64,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List all discovered MCP servers as plain text (no TUI)
    List,
    /// Health check all stdio servers in parallel; exit code 1 if any fails
    Check,
}

fn parse_timeout(s: &str) -> Result<f64, String> {
    match s.parse::<f64>() {
        Ok(v) if v.is_finite() && v > 0.0 && v <= 600.0 => Ok(v),
        _ => Err("expected a number of seconds between 0 and 600".to_string()),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let ctx = Ctx::from_env();
    let timeout = Duration::from_secs_f64(cli.timeout);

    match cli.command {
        Some(Commands::List) => cmd_list(&ctx),
        Some(Commands::Check) => cmd_check(&ctx, timeout),
        None => match run_tui(ctx, timeout) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("Error: {}", e);
                ExitCode::FAILURE
            }
        },
    }
}

// ---------------------------------------------------------------------------
// CLI commands
// ---------------------------------------------------------------------------

fn print_errors(errors: &[String]) {
    if !errors.is_empty() {
        eprintln!("\nConfig errors:");
        for e in errors {
            eprintln!("  {}", e);
        }
    }
}

fn cmd_list(ctx: &Ctx) -> ExitCode {
    let result = discovery::discover(ctx);
    if result.servers.is_empty() {
        println!("No MCP servers found.");
    } else {
        let w = result
            .servers
            .iter()
            .map(|s| s.name.chars().count())
            .max()
            .unwrap_or(6)
            .max(6);
        println!(
            "{:<w$}  {:<10}  {:<7}  SOURCE",
            "SERVER",
            "CLIENT",
            "TYPE",
            w = w
        );
        for s in &result.servers {
            let kind = if s.disabled {
                "off"
            } else {
                s.transport.kind_label()
            };
            println!(
                "{:<w$}  {:<10}  {:<7}  {}",
                s.name,
                s.client.label(),
                kind,
                s.source_path.display(),
                w = w
            );
        }
    }
    print_errors(&result.errors);
    ExitCode::SUCCESS
}

fn cmd_check(ctx: &Ctx, timeout: Duration) -> ExitCode {
    let result = discovery::discover(ctx);
    let servers: Vec<_> = result
        .servers
        .iter()
        .filter(|s| s.transport.is_stdio())
        .collect();
    if servers.is_empty() {
        println!("No stdio servers found to health check.");
        print_errors(&result.errors);
        return ExitCode::SUCCESS;
    }
    println!(
        "Checking {} stdio server{} (timeout {}s)...\n",
        servers.len(),
        if servers.len() == 1 { "" } else { "s" },
        timeout.as_secs_f64()
    );

    // All checks run concurrently; output stays in discovery order.
    let statuses: Vec<HealthStatus> = std::thread::scope(|scope| {
        let handles: Vec<_> = servers
            .iter()
            .map(|s| scope.spawn(move || health::check(s, ctx, timeout)))
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| HealthStatus::Error("check panicked".into()))
            })
            .collect()
    });

    let color = io::stdout().is_terminal();
    let paint = |code: &str, s: &str| {
        if color {
            format!("\x1b[{}m{}\x1b[0m", code, s)
        } else {
            s.to_string()
        }
    };
    let w = servers
        .iter()
        .map(|s| s.name.chars().count())
        .max()
        .unwrap_or(0)
        .max(10);
    let mut failed = 0;
    for (s, status) in servers.iter().zip(&statuses) {
        let (mark, detail) = match status {
            HealthStatus::Healthy {
                server_name,
                server_version,
                ..
            } => (
                paint("32", "✓"),
                format!("{} v{}", server_name, server_version),
            ),
            HealthStatus::Timeout(_) => (paint("33", "⚠"), status.label()),
            other => (paint("31", "✗"), other.label()),
        };
        if !status.is_ok() {
            failed += 1;
        }
        println!(
            "  {} {:<w$} {:<10} {}",
            mark,
            s.name,
            s.client.label(),
            detail,
            w = w
        );
    }
    println!();
    print_errors(&result.errors);
    if failed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

// ---------------------------------------------------------------------------
// TUI
// ---------------------------------------------------------------------------

/// Puts the terminal into TUI mode and restores it on drop — including on
/// early returns via `?`. A panic hook covers unwinding panics too.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let guard = TerminalGuard;
        execute!(io::stdout(), EnterAlternateScreen)?;
        // Not supported by every terminal (e.g. legacy Windows console); optional.
        let _ = execute!(io::stdout(), EnableBracketedPaste);
        Ok(guard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn restore_terminal() {
    let _ = execute!(
        io::stdout(),
        DisableBracketedPaste,
        LeaveAlternateScreen,
        Show
    );
    let _ = disable_raw_mode();
}

fn run_tui(ctx: Ctx, timeout: Duration) -> io::Result<()> {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));

    let mut guard = Some(TerminalGuard::enter()?);
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut app = app::App::new(ctx, timeout);

    loop {
        app.poll_health();
        terminal.draw(|f| ui::render(f, &mut app))?;

        // Short poll keeps spinners/status expiry/health results flowing.
        if !event::poll(Duration::from_millis(150))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) => match app.handle_key(key) {
                app::Action::Quit => break,
                app::Action::Edit(path) => {
                    drop(guard.take());
                    let outcome = open_editor(&path);
                    guard = Some(TerminalGuard::enter()?);
                    terminal.clear()?;
                    app.after_edit(outcome);
                }
                app::Action::None => {}
            },
            Event::Paste(text) => app.handle_paste(&text),
            _ => {}
        }
    }
    Ok(())
}

/// Run `$VISUAL` / `$EDITOR` (which may include arguments, e.g. `code -w`).
fn open_editor(path: &Path) -> Result<(), String> {
    let default = if cfg!(windows) { "notepad" } else { "vi" };
    let spec = ["VISUAL", "EDITOR"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|s| !s.trim().is_empty()))
        .unwrap_or_else(|| default.to_string());
    let words =
        shell::split(&spec).map_err(|e| format!("can't parse editor \"{}\": {}", spec, e))?;
    let (program, args) = words.split_first().ok_or("editor command is empty")?;
    let status = std::process::Command::new(program)
        .args(args)
        .arg(path)
        .status()
        .map_err(|e| format!("failed to launch editor \"{}\": {}", program, e))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("editor \"{}\" exited with {}", program, status))
    }
}
