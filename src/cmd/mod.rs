use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CmdError {
    #[error("command failed: {0}")]
    Failed(String),
    #[error("command not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg_attr(test, mockall::automock)]
pub trait CmdExec: Send + Sync {
    fn run(&self, name: &str, args: &[String]) -> Result<(), CmdError>;
    fn output(&self, name: &str, args: &[String]) -> Result<String, CmdError>;
}

pub struct SystemCmdExec;

impl CmdExec for SystemCmdExec {
    fn run(&self, name: &str, args: &[String]) -> Result<(), CmdError> {
        let status = Command::new(name).args(args).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(CmdError::Failed(format!(
                "{} {} exited with {}",
                name,
                args.join(" "),
                status
            )))
        }
    }

    fn output(&self, name: &str, args: &[String]) -> Result<String, CmdError> {
        let output = Command::new(name).args(args).output()?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(CmdError::Failed(format!(
                "{} {} failed: {}",
                name,
                args.join(" "),
                stderr.trim()
            )))
        }
    }
}

/// Helper to create args slice from string literals.
pub fn args(strs: &[&str]) -> Vec<String> {
    strs.iter().map(|s| s.to_string()).collect()
}

/// Convert a Command to a string representation for debugging/testing.
pub fn command_to_string(cmd: &Command) -> String {
    let prog = cmd.get_program().to_string_lossy();
    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    if args.is_empty() {
        prog.to_string()
    } else {
        format!("{} {}", prog, args.join(" "))
    }
}
