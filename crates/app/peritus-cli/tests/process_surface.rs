//! Black-box G1 process surface and stable exit-contract tests.

use std::process::Command;

fn peritus() -> Command {
    Command::new(env!("CARGO_BIN_EXE_peritus"))
}

#[test]
fn help_version_and_completions_are_transport_free() {
    let help = peritus().arg("--help").output().expect("run help");
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).expect("UTF-8 help");
    assert!(help.contains("USAGE:"));
    assert!(help.contains("update"));
    assert!(help.contains("terminal attach"));

    let version = peritus().arg("--version").output().expect("run version");
    assert!(version.status.success());
    assert!(String::from_utf8(version.stdout).unwrap().starts_with("peritus "));

    let completion =
        peritus().args(["completions", "bash"]).output().expect("run completion generation");
    assert!(completion.status.success());
    let completion = String::from_utf8(completion.stdout).unwrap();
    assert!(completion.contains("_peritus"));
    assert!(completion.contains("artifact"));
    assert!(completion.contains("update"));
}

#[test]
fn usage_failures_have_stable_human_and_json_exit_contracts() {
    let human = peritus().arg("unknown").output().expect("run invalid command");
    assert_eq!(human.status.code(), Some(2));
    assert!(String::from_utf8(human.stderr).unwrap().contains("peritus: usage:"));

    let json = peritus().args(["--json", "unknown"]).output().expect("run JSON failure");
    assert_eq!(json.status.code(), Some(2));
    let value: serde_json::Value = serde_json::from_slice(&json.stderr).expect("JSON error");
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["category"], "usage");
}

#[test]
fn daemon_commands_require_an_explicit_endpoint() {
    let output = peritus().arg("status").output().expect("run status");
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8(output.stderr).unwrap().contains("--endpoint"));
}
