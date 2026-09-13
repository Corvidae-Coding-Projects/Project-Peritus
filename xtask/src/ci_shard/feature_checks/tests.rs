use super::{AGENT, SHARD, command, execute};
use crate::ci_shard::{Operation, SHARD_NAMES, cargo_command};
use std::path::Path;
use std::process::{Command, Stdio};

#[test]
fn agent_feature_commands_are_separate_locked_all_target_invocations() {
    let root = Path::new("workspace");
    for (operation, name, suffix) in [
        (Operation::Build, "build", vec![]),
        (Operation::Test, "test", vec!["--", "--test-threads=1"]),
        (Operation::Clippy, "clippy", vec!["--", "-D", "warnings"]),
    ] {
        let additional = command(root, operation, SHARD, &[AGENT, "peritus-orchestrator"])
            .expect("feature command selection")
            .expect("required agent feature configuration");
        assert_eq!(additional.get_program(), "cargo");
        assert_eq!(additional.get_current_dir(), Some(root));
        let arguments: Vec<_> =
            additional.get_args().map(|value| value.to_str().unwrap()).collect();
        let mut expected =
            vec![name, "--locked", "--all-targets", "--no-default-features", "--package", AGENT];
        expected.extend(suffix);
        assert_eq!(arguments, expected);
        let regular = cargo_command(root, operation, &[AGENT, "peritus-orchestrator"]);
        assert!(regular.get_args().any(|argument| argument == "--all-features"));
        assert!(!regular.get_args().any(|argument| argument == "--no-default-features"));
    }
}

#[test]
fn feature_checks_only_extend_the_owning_build_test_and_clippy_shards() {
    let root = Path::new("workspace");
    for shard in SHARD_NAMES.into_iter().filter(|shard| *shard != SHARD) {
        for operation in [Operation::Build, Operation::Test, Operation::Clippy] {
            assert!(command(root, operation, shard, &[AGENT]).expect("other shard").is_none());
        }
    }
    for operation in [
        Operation::TestDaemon,
        Operation::DocTest,
        Operation::Docs,
        Operation::TestPlatformTerminalInteractive,
        Operation::TestPlatformTerminalSignal,
        Operation::TestPlatformTerminalCancel,
        Operation::TestRunnerRecovery,
        Operation::TestRunnerProduct,
        Operation::VerusVerify,
        Operation::VerusVerifyStrict,
        Operation::VerusBuild,
        Operation::VerusBuildStrict,
    ] {
        assert!(command(root, operation, SHARD, &[AGENT]).expect("other operation").is_none());
    }
}

#[test]
fn removing_the_agent_from_its_shard_cannot_silently_remove_feature_coverage() {
    for operation in [Operation::Build, Operation::Test, Operation::Clippy] {
        assert!(command(Path::new("workspace"), operation, SHARD, &[]).is_err());
    }
}

#[test]
fn failed_feature_process_is_a_failed_shard_check() {
    // The test harness rejects an unknown option before it can execute any child test.
    let mut failing = Command::new(std::env::current_exe().expect("test executable"));
    failing
        .arg("--peritus-intentionally-invalid-test-option")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let error = execute(Path::new("."), Operation::Test, &mut failing)
        .expect_err("nonzero feature process must fail");
    assert!(error.to_string().contains("no-default-features"));
    assert!(error.to_string().contains("failed during Test"));
}

#[test]
fn feature_process_launch_failure_is_not_success() {
    let missing = std::env::temp_dir()
        .join(format!("peritus-missing-feature-command-{}", std::process::id()));
    assert!(!missing.exists(), "missing executable fixture must not exist");
    let error = execute(Path::new("."), Operation::Build, &mut Command::new(missing))
        .expect_err("missing feature executable must fail");
    assert!(error.to_string().contains("execute agent no-default-features check"));
}
