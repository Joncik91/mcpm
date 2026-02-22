use std::io;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

mod app;
mod discovery;
mod types;
mod ui;

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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    match cli.command {
        Some(Commands::List) => cmd_list(&cwd),
        None => run_tui(cwd)?,
    }

    Ok(())
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
            Ok(true) => break Ok(()),
            Ok(false) => {}
            Err(e) => break Err(e),
        }
    };

    // Always restore terminal, even on error
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}
