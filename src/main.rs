use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

mod app;
mod config_writer;
mod discovery;
mod health;
mod types;
mod ui;
mod wizard;

#[derive(Parser)]
#[command(name = "mcpm", version, about = "MCP Server Manager — see all your MCP servers across all clients")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List all discovered MCP servers as plain text (no TUI)
    List,
    /// Run health checks on all stdio servers and print results
    Check,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    match cli.command {
        Some(Commands::List) => {
            cmd_list(&cwd);
            ExitCode::SUCCESS
        }
        Some(Commands::Check) => cmd_check(&cwd),
        None => match run_tui(cwd) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("Error: {}", e);
                ExitCode::FAILURE
            }
        },
    }
}

fn cmd_list(cwd: &PathBuf) {
    let result = discovery::discover(cwd);

    if result.servers.is_empty() {
        println!("No MCP servers found.");
    } else {
        println!(
            "{:<25} {:>12}  {:<8}  {}",
            "SERVER", "CLIENT", "TYPE", "SOURCE"
        );
        println!("{}", "-".repeat(80));
        for s in &result.servers {
            println!(
                "{:<25} {:>12}  {:<8}  {}",
                s.name,
                s.client.label(),
                s.transport.kind_label(),
                s.source_path,
            );
        }
    }

    if !result.errors.is_empty() {
        eprintln!("\nParse errors:");
        for e in &result.errors {
            eprintln!("  {}", e);
        }
    }
}

fn cmd_check(cwd: &PathBuf) -> ExitCode {
    let result = discovery::discover(cwd);

    let stdio_servers: Vec<(usize, &types::McpServer)> = result
        .servers
        .iter()
        .enumerate()
        .filter(|(_, s)| s.transport.is_stdio())
        .collect();

    if stdio_servers.is_empty() {
        println!("No stdio servers found to health check.");
        return ExitCode::SUCCESS;
    }

    println!(
        "Checking {} stdio server{}...\n",
        stdio_servers.len(),
        if stdio_servers.len() == 1 { "" } else { "s" }
    );

    let mut any_failed = false;

    for (i, server) in &stdio_servers {
        let hr = health::check_server(*i, server);
        match &hr.status {
            types::HealthStatus::Healthy {
                server_name,
                server_version,
            } => {
                println!(
                    "  \x1b[32m✓\x1b[0m {:<25} ({} v{})",
                    server.name, server_name, server_version
                );
            }
            types::HealthStatus::Timeout => {
                println!("  \x1b[33m⚠\x1b[0m {:<25} timeout (5s)", server.name);
                any_failed = true;
            }
            types::HealthStatus::Error(e) => {
                println!("  \x1b[31m✗\x1b[0m {:<25} {}", server.name, e);
                any_failed = true;
            }
            _ => {}
        }
    }

    println!();
    if any_failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn run_tui(cwd: PathBuf) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app_state = app::App::new(cwd);

    let result = loop {
        terminal.draw(|f| ui::render(f, &mut app_state))?;
        match app::handle_event(&mut app_state) {
            Ok((true, _)) => break Ok(()),
            Ok((false, Some(editor_path))) => {
                // Exit TUI, run $EDITOR, re-enter TUI
                disable_raw_mode()?;
                execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
                let _ = std::process::Command::new(&editor)
                    .arg(&editor_path)
                    .status();

                enable_raw_mode()?;
                execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                terminal.clear()?;
                app_state.refresh();
            }
            Ok((false, None)) => {}
            Err(e) => break Err(e),
        }
    };

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}
