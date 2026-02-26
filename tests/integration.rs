use assert_cmd::Command;
use predicates::prelude::*;

#[allow(deprecated)]
fn gana() -> Command {
    Command::cargo_bin("gana").unwrap()
}

#[test]
fn test_help_flag() {
    gana()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Orchestrate your AI agent teams"));
}

#[test]
fn test_version_flag() {
    gana()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("gana 0.1.0"));
}

#[test]
fn test_help_subcommand() {
    gana()
        .arg("help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

#[test]
fn test_debug_subcommand() {
    gana()
        .arg("debug")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Config directory")
                .and(predicate::str::contains("Default program")),
        );
}

#[test]
fn test_reset_subcommand() {
    gana()
        .arg("reset")
        .assert()
        .success()
        .stdout(predicate::str::contains("reset"));
}

#[test]
fn test_stop_daemon_no_daemon() {
    gana()
        .arg("stop-daemon")
        .assert()
        .success()
        .stdout(predicate::str::contains("No daemon running"));
}

#[test]
fn test_unknown_subcommand() {
    gana()
        .arg("foobar")
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn test_debug_shows_config_values() {
    gana()
        .arg("debug")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("claude")
                .and(predicate::str::contains("1000"))
                .and(predicate::str::contains("gana/")),
        );
}

#[test]
fn test_reset_is_idempotent() {
    // First reset
    gana().arg("reset").assert().success();
    // Second reset should also succeed
    gana().arg("reset").assert().success();
}

#[test]
fn test_daemon_subcommand_help() {
    gana()
        .args(["daemon", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("config-dir"));
}
