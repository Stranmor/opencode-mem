use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_cli_help() {
    let mut cmd = Command::cargo_bin("opencode-mem").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Persistent memory system for OpenCode"));
}

#[test]
fn test_cli_serve_help() {
    let mut cmd = Command::cargo_bin("opencode-mem").unwrap();
    cmd.arg("serve").arg("--help").assert().success().stdout(predicate::str::contains("port"));
}
