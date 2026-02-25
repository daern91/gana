#[cfg(unix)]
pub fn kill_process(pid: i32) -> anyhow::Result<()> {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    kill(Pid::from_raw(pid), Signal::SIGTERM)?;
    Ok(())
}

#[cfg(unix)]
pub fn is_process_running(pid: i32) -> bool {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    // Sending signal 0 checks if process exists without actually signaling it
    kill(Pid::from_raw(pid), None).is_ok()
}

#[cfg(windows)]
pub fn kill_process(pid: i32) -> anyhow::Result<()> {
    std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .output()?;
    Ok(())
}

#[cfg(windows)]
pub fn is_process_running(pid: i32) -> bool {
    std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid)])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
        .unwrap_or(false)
}
