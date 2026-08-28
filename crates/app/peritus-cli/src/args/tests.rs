use std::ffi::OsString;

use super::{Cli, Command, PromptValue};

fn parse(arguments: &[&str]) -> Result<Cli, crate::error::CliError> {
    Cli::parse(arguments.iter().map(OsString::from))
}

#[test]
fn provider_settings_are_a_standalone_product_command() {
    let cli = parse(&["peritus", "providers"]).expect("provider settings");
    assert!(matches!(cli.command, Command::Providers));
    assert!(cli.endpoint.is_none());
}

#[test]
fn global_options_and_status_are_parsed_strictly() {
    let cli = parse(&[
        "peritus",
        "--endpoint",
        "/tmp/peritus.sock",
        "--timeout-seconds",
        "7",
        "--json",
        "status",
    ])
    .expect("status command");
    assert!(matches!(cli.command, Command::Status));
    assert!(cli.json);
    assert_eq!(cli.timeout.as_secs(), 7);
    assert_eq!(cli.endpoint.as_deref(), Some(std::ffi::OsStr::new("/tmp/peritus.sock")));
}

#[test]
fn duplicate_and_unknown_options_are_usage_errors() {
    for arguments in [
        vec!["peritus", "--json", "--json", "status"],
        vec!["peritus", "--mystery", "status"],
        vec!["peritus", "status", "--extra"],
    ] {
        assert!(parse(&arguments).is_err(), "accepted {arguments:?}");
    }
}

#[test]
fn event_topics_are_required_unique_and_canonicalized() {
    assert!(parse(&["peritus", "events", "watch"]).is_err());
    assert!(parse(&["peritus", "events", "watch", "--topic", "run", "--topic", "run"]).is_err());
    let cli = parse(&[
        "peritus", "events", "watch", "--topic", "trace", "--topic", "run", "--count", "3",
    ])
    .expect("event watch");
    let Command::Events(events) = cli.command else {
        panic!("events command expected");
    };
    assert_eq!(events.topics, ["run", "trace"]);
    assert_eq!(events.count, Some(3));
}

#[test]
fn prompt_answers_require_exactly_one_value_kind() {
    let binding = "binding.bin";
    assert!(
        parse(&[
            "peritus",
            "prompt",
            "answer",
            "--binding",
            binding,
            "--text",
            "hello",
            "--confirm",
            "true",
        ])
        .is_err()
    );
    let cli = parse(&["peritus", "prompt", "answer", "--binding", binding, "--text", "hello"])
        .expect("text answer");
    let Command::PromptAnswer(answer) = cli.command else {
        panic!("prompt answer expected");
    };
    assert!(matches!(answer.value, PromptValue::Text(ref text) if text == "hello"));
}

#[test]
fn identifiers_and_positive_limits_fail_before_transport() {
    assert!(parse(&["peritus", "--session", "00", "status"]).is_err());
    assert!(parse(&["peritus", "--timeout-seconds", "0", "status"]).is_err());
    assert!(
        parse(&[
            "peritus",
            "terminal",
            "resize",
            "--attachment",
            "11111111111111111111111111111111",
            "--process",
            "22222222222222222222222222222222",
            "--originating-request",
            "33333333333333333333333333333333",
            "--columns",
            "0",
            "--rows",
            "24",
        ])
        .is_err()
    );
}
