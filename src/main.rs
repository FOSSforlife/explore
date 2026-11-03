mod app;
mod command;
mod keymap;
mod provider;
mod theme;
mod ui;

use anyhow::Result;
use clap::Parser;
use ratatui::{
    DefaultTerminal,
    crossterm::event::{self, Event, KeyEventKind},
};

use crate::{app::App, keymap::Keymap};

/// Browse nested data in ranger-style columns.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// What to browse: a file path, or `-` for stdin.
    #[arg(required_unless_present = "list_providers")]
    source: Option<String>,

    /// Use this provider instead of auto-detecting one.
    #[arg(short, long)]
    provider: Option<String>,

    /// Run commands on startup, e.g. -c 'enter; down 3'. Repeatable.
    #[arg(short, long = "command", value_name = "COMMANDS")]
    commands: Vec<String>,

    /// List available providers and exit.
    #[arg(long)]
    list_providers: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.list_providers {
        for f in provider::registry() {
            println!("{:<10} {}", f.name(), f.description());
        }
        return Ok(());
    }

    // Load before touching the terminal so errors print normally.
    let source = cli.source.expect("clap enforces source");
    let provider = provider::open(&source, cli.provider.as_deref())?;
    let mut app = App::new(provider, Keymap::defaults())?;
    for line in &cli.commands {
        app.run_line(line);
    }

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    while !app.should_quit() {
        terminal.draw(|f| ui::draw(f, app))?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key);
        }
    }
    Ok(())
}
