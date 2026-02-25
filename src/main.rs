mod app;
mod cmd;
mod config;
mod daemon;
mod keys;
mod log;
mod session;
mod ui;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "league",
    about = "Orchestrate multiple AI coding assistants in parallel, isolated workspaces",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Reset all sessions and clean up resources
    Reset,
    /// Show debug information
    Debug,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    log::initialize(true);

    match cli.command {
        Some(Commands::Reset) => {
            println!("Resetting all sessions...");
            // TODO: implement reset
            Ok(())
        }
        Some(Commands::Debug) => {
            println!("Debug information:");
            let config_dir = config::get_config_dir()?;
            println!("  Config directory: {}", config_dir.display());
            let config = config::Config::load(&config_dir)?;
            println!("  Default program: {}", config.default_program);
            println!("  Auto-yes: {}", config.auto_yes);
            println!("  Poll interval: {}ms", config.daemon_poll_interval);
            println!("  Branch prefix: {}", config.branch_prefix);
            Ok(())
        }
        None => {
            // TODO: launch TUI (Phase 6)
            println!("league - AI coding assistant orchestrator");
            println!("TUI not yet implemented. Use --help for available commands.");
            Ok(())
        }
    }
}
