use super::{
    Operation, PLATFORM_PACKAGE, PLATFORM_TERMINAL_CANCEL, PLATFORM_TERMINAL_INTERACTIVE,
    PLATFORM_TERMINAL_SIGNAL, SHARD_NAMES, cargo_command, shard_for_layer, shard_for_package,
};
use std::path::Path;

#[test]
fn operation_parser_is_closed() {
    assert_eq!(Operation::parse("test"), Some(Operation::Test));
    assert_eq!(
        Operation::parse("test-platform-terminal-interactive"),
        Some(Operation::TestPlatformTerminalInteractive)
    );
    assert_eq!(Operation::parse("verus-build-strict"), Some(Operation::VerusBuildStrict));
    assert_eq!(Operation::parse("bench"), None);
}

#[test]
fn every_architecture_layer_has_one_stable_shard() {
    for layer in [
        "foundation",
        "state",
        "runtime",
        "tools",
        "model",
        "orchestration",
        "app",
        "testing",
        "analysis",
        "observe",
        "extensions",
        "engineering",
    ] {
        assert!(SHARD_NAMES.contains(&shard_for_layer(layer).expect("known layer")));
    }
    assert_eq!(shard_for_layer("unknown"), None);
}

#[test]
fn product_runner_has_an_independent_bounded_app_shard() {
    assert_eq!(shard_for_package("peritus-product-runner", "app"), Some("app-runner"));
    assert_eq!(shard_for_package("peritus-daemon", "app"), Some("app-shell"));
}

#[test]
fn long_running_testing_packages_have_independent_bounded_shards() {
    assert_eq!(
        shard_for_package("peritus-platform-qualification", "testing"),
        Some("testing-platform")
    );
    assert_eq!(
        shard_for_package("peritus-external-benchmarks", "testing"),
        Some("testing-external")
    );
    assert_eq!(shard_for_package("peritus-performance-qualification", "testing"), Some("testing"));
}

#[test]
fn strict_verus_shards_always_request_no_cheating() {
    let command = cargo_command(Path::new("."), Operation::VerusBuildStrict, &["peritus-types"]);
    let arguments =
        command.get_args().map(|value| value.to_string_lossy().into_owned()).collect::<Vec<_>>();
    assert!(arguments.iter().any(|argument| argument == "--no-cheating"));
    assert!(arguments.windows(2).any(|pair| pair == ["--rlimit", "20"]));
}

#[test]
fn platform_test_operations_partition_the_terminal_case_exactly_once() {
    let regular = cargo_command(Path::new("."), Operation::Test, &[PLATFORM_PACKAGE]);
    let regular =
        regular.get_args().map(|value| value.to_string_lossy().into_owned()).collect::<Vec<_>>();
    for test in [PLATFORM_TERMINAL_INTERACTIVE, PLATFORM_TERMINAL_SIGNAL, PLATFORM_TERMINAL_CANCEL]
    {
        assert!(regular.windows(2).any(|pair| pair == ["--skip", test]));
    }

    for (operation, test) in [
        (Operation::TestPlatformTerminalInteractive, PLATFORM_TERMINAL_INTERACTIVE),
        (Operation::TestPlatformTerminalSignal, PLATFORM_TERMINAL_SIGNAL),
        (Operation::TestPlatformTerminalCancel, PLATFORM_TERMINAL_CANCEL),
    ] {
        let terminal = cargo_command(Path::new("."), operation, &[PLATFORM_PACKAGE]);
        let terminal = terminal
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(terminal.windows(2).any(|pair| pair == ["--test", "general_capability"]));
        assert!(terminal.iter().any(|argument| argument == test));
        assert!(terminal.iter().any(|argument| argument == "--exact"));
        assert!(!terminal.iter().any(|argument| argument == "--skip"));
    }
}
