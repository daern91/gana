use std::fs::File;
use std::process::Command;

use crate::session::tmux::TmuxError;

/// Factory trait for creating PTY handles for tmux sessions.
pub trait PtyFactory: Send + Sync {
    /// Start a command with a new PTY and return the master file descriptor.
    fn start(&self, cmd: &mut Command) -> Result<File, TmuxError>;

    /// Close any resources held by the factory.
    fn close(&self);
}

/// System PTY factory using nix::pty for Unix systems.
pub struct SystemPtyFactory;

#[cfg(unix)]
impl PtyFactory for SystemPtyFactory {
    fn start(&self, cmd: &mut Command) -> Result<File, TmuxError> {
        use nix::pty::openpty;
        use std::os::fd::IntoRawFd;
        use std::os::unix::io::FromRawFd;
        use std::os::unix::process::CommandExt;
        use std::process::Stdio;

        let pty = openpty(None, None).map_err(|e: nix::Error| TmuxError::PtyError(e.to_string()))?;

        let slave_fd = pty.slave.into_raw_fd();

        // Set the child process to use the slave side of the PTY
        let slave_in =
            unsafe { Stdio::from_raw_fd(slave_fd) };
        let slave_out =
            unsafe { Stdio::from_raw_fd(nix::libc::dup(slave_fd)) };
        let slave_err =
            unsafe { Stdio::from_raw_fd(nix::libc::dup(slave_fd)) };

        cmd.stdin(slave_in)
            .stdout(slave_out)
            .stderr(slave_err);

        // Create a new session for the child process
        unsafe {
            cmd.pre_exec(|| {
                nix::libc::setsid();
                Ok(())
            });
        }

        cmd.spawn()
            .map_err(|e| TmuxError::PtyError(e.to_string()))?;

        // Return the master side as a File
        let master_file = File::from(pty.master);
        Ok(master_file)
    }

    fn close(&self) {
        // No persistent resources to clean up
    }
}
