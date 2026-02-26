use std::fs;
use std::path::Path;
use std::process::Command;

const REPO: &str = "daern91/gana";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Check for updates and auto-install if a newer version is available.
/// Runs silently — never blocks startup or shows errors to the user.
/// Returns Some(new_version) if an update was downloaded and will take
/// effect on next launch.
pub fn auto_update(config_dir: &Path) -> Option<String> {
    // Check if we recently checked (at most once per hour)
    let last_check_file = config_dir.join("last_update_check");
    if let Ok(metadata) = fs::metadata(&last_check_file) {
        if let Ok(modified) = metadata.modified() {
            if modified.elapsed().unwrap_or_default().as_secs() < 3600 {
                // Check for pending update notification
                return check_pending_update(config_dir);
            }
        }
    }

    // Check if a previous update was staged
    let result = check_pending_update(config_dir);

    // Spawn background thread so we never block startup
    let config_dir_owned = config_dir.to_path_buf();
    std::thread::spawn(move || {
        let _ = do_update_check(&config_dir_owned);
    });

    result
}

/// Check if there's a pending "updated to vX.Y.Z" notification.
fn check_pending_update(config_dir: &Path) -> Option<String> {
    let notify_file = config_dir.join("update_installed");
    if let Ok(version) = fs::read_to_string(&notify_file) {
        let _ = fs::remove_file(&notify_file);
        Some(version.trim().to_string())
    } else {
        None
    }
}

/// The actual update check + download (runs in background thread).
fn do_update_check(config_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Mark that we checked
    let _ = fs::create_dir_all(config_dir);
    let _ = fs::write(config_dir.join("last_update_check"), "");

    // Get latest release from GitHub API
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time", "5",
            &format!("https://api.github.com/repos/{}/releases/latest", REPO),
        ])
        .output()?;

    if !output.status.success() {
        return Ok(()); // Silently fail
    }

    let body = String::from_utf8_lossy(&output.stdout);

    // Parse tag_name from JSON (simple extraction, no serde needed)
    let tag = body
        .split("\"tag_name\"")
        .nth(1)
        .and_then(|s| s.split('"').nth(1))
        .unwrap_or("");

    let latest = tag.strip_prefix('v').unwrap_or(tag);

    if latest.is_empty() {
        return Ok(());
    }

    // Compare versions
    let current = semver::Version::parse(CURRENT_VERSION).ok();
    let remote = semver::Version::parse(latest).ok();

    match (current, remote) {
        (Some(cur), Some(rem)) if rem > cur => {
            // Newer version available — download it
            if let Err(_) = download_and_install(tag, config_dir) {
                // Silently fail
            }
        }
        _ => {} // Up to date or parse error
    }

    Ok(())
}

/// Download the new binary and replace the current one.
fn download_and_install(tag: &str, config_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let target = detect_target();
    let url = format!(
        "https://github.com/{}/releases/download/{}/gana-{}.tar.gz",
        REPO, tag, target
    );

    let tmp_dir = config_dir.join("update_tmp");
    let _ = fs::create_dir_all(&tmp_dir);

    let tarball = tmp_dir.join("gana.tar.gz");

    // Download
    let status = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time", "30",
            "-o", &tarball.to_string_lossy(),
            &url,
        ])
        .status()?;

    if !status.success() {
        let _ = fs::remove_dir_all(&tmp_dir);
        return Ok(());
    }

    // Extract
    let status = Command::new("tar")
        .args(["-xzf", &tarball.to_string_lossy(), "-C", &tmp_dir.to_string_lossy()])
        .status()?;

    if !status.success() {
        let _ = fs::remove_dir_all(&tmp_dir);
        return Ok(());
    }

    let new_binary = tmp_dir.join("gana");
    if !new_binary.exists() {
        let _ = fs::remove_dir_all(&tmp_dir);
        return Ok(());
    }

    // Replace the current binary
    if let Ok(current_exe) = std::env::current_exe() {
        let current_exe = current_exe.canonicalize().unwrap_or(current_exe);
        // Move current to .old, move new to current
        let backup = current_exe.with_extension("old");
        let _ = fs::remove_file(&backup);
        if fs::rename(&current_exe, &backup).is_ok() {
            if fs::copy(&new_binary, &current_exe).is_ok() {
                // Make executable
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(
                        &current_exe,
                        fs::Permissions::from_mode(0o755),
                    );
                }
                // Write notification for next launch
                let version = tag.strip_prefix('v').unwrap_or(tag);
                let _ = fs::write(
                    config_dir.join("update_installed"),
                    version,
                );
                let _ = fs::remove_file(&backup);
            } else {
                // Restore backup
                let _ = fs::rename(&backup, &current_exe);
            }
        }
    }

    let _ = fs::remove_dir_all(&tmp_dir);
    Ok(())
}

fn detect_target() -> String {
    let os = if cfg!(target_os = "linux") {
        "unknown-linux-gnu"
    } else if cfg!(target_os = "macos") {
        "apple-darwin"
    } else {
        "unknown"
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    };

    format!("{}-{}", arch, os)
}
