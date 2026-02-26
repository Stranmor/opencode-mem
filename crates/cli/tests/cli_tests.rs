use predicates::prelude::*;

#[test]
fn test_cli_help() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("opencode-mem");
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Persistent memory system for OpenCode"));
}

#[test]
fn test_cli_serve_help() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("opencode-mem");
    cmd.arg("serve").arg("--help").assert().success().stdout(predicate::str::contains("port"));
}
