pub mod platform;

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::config::Config;
use crate::session::InstanceStatus;
use crate::session::storage::{FileStorage, InstanceStorage};

const PID_FILE: &str = "daemon.pid";

/// Global shutdown flag, set by signal handlers.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Run the daemon loop: poll sessions, auto-respond to prompts.
pub fn run_daemon(config_dir: &Path, config: &Config) -> anyhow::Result<()> {
    let storage = FileStorage::new(config_dir);
    let poll_interval = std::time::Duration::from_millis(config.daemon_poll_interval);

    // Write PID file
    let pid = std::process::id();
    let pid_path = config_dir.join(PID_FILE);
    fs::create_dir_all(config_dir)?;
    fs::write(&pid_path, pid.to_string())?;

    // Install signal handlers for graceful shutdown
    install_signal_handlers();

    tracing::info!("Daemon started with PID {}", pid);

    while !SHUTDOWN.load(Ordering::SeqCst) {
        if let Ok(mut instances) = storage.load_instances() {
            for instance in instances.iter_mut() {
                if instance.status == InstanceStatus::Running
                    && instance.auto_yes
                    && instance.has_updated()
                {
                    instance.send_keys("y\n");
                }
            }
        }

        std::thread::sleep(poll_interval);
    }

    // Cleanup PID file
    let _ = fs::remove_file(&pid_path);
    tracing::info!("Daemon stopped");
    Ok(())
}

#[cfg(unix)]
extern "C" fn handle_shutdown(_: std::ffi::c_int) {
    SHUTDOWN.store(true, Ordering::SeqCst);
}

#[cfg(unix)]
fn install_signal_handlers() {
    use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};
    let handler = SigHandler::Handler(handle_shutdown);
    let action = SigAction::new(handler, SaFlags::empty(), SigSet::empty());
    unsafe {
        let _ = sigaction(Signal::SIGINT, &action);
        let _ = sigaction(Signal::SIGTERM, &action);
    }
}

#[cfg(not(unix))]
fn install_signal_handlers() {
    // On non-Unix platforms, signal handling is not yet implemented.
}

/// Launch the daemon as a background process.
#[allow(dead_code)]
pub fn launch_daemon(config_dir: &Path) -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;

    let child = std::process::Command::new(exe)
        .arg("daemon")
        .arg("--config-dir")
        .arg(config_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    println!("Daemon launched with PID {}", child.id());
    Ok(())
}

/// Stop a running daemon.
pub fn stop_daemon(config_dir: &Path) -> anyhow::Result<()> {
    let pid_path = config_dir.join(PID_FILE);
    if !pid_path.exists() {
        println!("No daemon running");
        return Ok(());
    }

    let pid_str = fs::read_to_string(&pid_path)?;
    let pid: i32 = pid_str.trim().parse()?;

    // Send SIGTERM
    platform::kill_process(pid)?;

    // Remove PID file
    let _ = fs::remove_file(&pid_path);
    println!("Daemon stopped (PID {})", pid);
    Ok(())
}

/// Check if daemon is running.
pub fn is_daemon_running(config_dir: &Path) -> bool {
    let pid_path = config_dir.join(PID_FILE);
    if !pid_path.exists() {
        return false;
    }
    if let Ok(pid_str) = fs::read_to_string(&pid_path) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            return platform::is_process_running(pid);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_is_daemon_running_no_pid_file() {
        let tmp = TempDir::new().unwrap();
        assert!(!is_daemon_running(tmp.path()));
    }

    #[test]
    fn test_stop_daemon_no_pid_file() {
        let tmp = TempDir::new().unwrap();
        // Should succeed and print "No daemon running"
        stop_daemon(tmp.path()).unwrap();
    }

    #[test]
    fn test_is_daemon_running_stale_pid() {
        let tmp = TempDir::new().unwrap();
        // Write a PID file with a PID that almost certainly doesn't exist
        fs::write(tmp.path().join(PID_FILE), "999999999").unwrap();
        assert!(!is_daemon_running(tmp.path()));
    }

    #[test]
    fn test_is_daemon_running_invalid_pid() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(PID_FILE), "not-a-number").unwrap();
        assert!(!is_daemon_running(tmp.path()));
    }
}
