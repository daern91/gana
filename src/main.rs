#[allow(dead_code)]
mod app;
mod cmd;
mod config;
mod daemon;
#[allow(dead_code)]
mod keys;
mod log;
mod session;
#[allow(dead_code)]
mod ui;

use clap::{Parser, Subcommand};
use session::storage::InstanceStorage;

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
    /// Start the background daemon
    Daemon {
        /// Config directory override
        #[arg(long)]
        config_dir: Option<String>,
    },
    /// Stop the background daemon
    StopDaemon,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    log::initialize(true);
    let config_dir = config::get_config_dir()?;
    let config = config::Config::load(&config_dir).unwrap_or_default();

    match cli.command {
        Some(Commands::Reset) => {
            println!("Resetting all sessions...");
            let cmd = cmd::SystemCmdExec;
            let _ = session::tmux::TmuxSession::cleanup_sessions(&cmd);
            let config_dir_str = config_dir.to_string_lossy();
            session::git::cleanup_worktrees(&config_dir_str, &cmd)?;
            // Delete stored instances
            let storage = session::storage::FileStorage::new(&config_dir);
            storage.save_instances(&[])?;
            println!("All sessions reset.");
            Ok(())
        }
        Some(Commands::Debug) => {
            println!("Debug information:");
            println!("  Config directory: {}", config_dir.display());
            println!("  Default program: {}", config.default_program);
            println!("  Auto-yes: {}", config.auto_yes);
            println!("  Poll interval: {}ms", config.daemon_poll_interval);
            println!("  Branch prefix: {}", config.branch_prefix);
            println!(
                "  Daemon running: {}",
                daemon::is_daemon_running(&config_dir)
            );
            Ok(())
        }
        Some(Commands::Daemon { config_dir: dir_override }) => {
            let dir = dir_override
                .map(std::path::PathBuf::from)
                .unwrap_or(config_dir);
            daemon::run_daemon(&dir, &config)
        }
        Some(Commands::StopDaemon) => daemon::stop_daemon(&config_dir),
        None => {
            // Launch TUI
            app::run(config, config_dir)
        }
    }
}
