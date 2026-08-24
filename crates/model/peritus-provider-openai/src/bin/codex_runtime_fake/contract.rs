//! Hardened invocation assertions for the fake executable.

use std::path::PathBuf;

pub(super) fn valid(arguments: &[String], stdin: &str) -> bool {
    let required = [
        "exec",
        "--json",
        "--ephemeral",
        "--ignore-user-config",
        "--ignore-rules",
        "--skip-git-repo-check",
        "--sandbox",
        "read-only",
        "--color",
        "never",
        "--model",
        "--output-schema",
        "-",
    ];
    let required_present = required.iter().all(|value| arguments.iter().any(|item| item == value));
    let schema = argument_value(arguments, "--output-schema").map(PathBuf::from);
    let cwd = std::env::current_dir().ok();
    let isolated_schema = schema.as_ref().is_some_and(|path| {
        path.is_file() && cwd.as_ref().is_some_and(|directory| path.parent() == Some(directory))
    });
    required_present
        && environment_absent()
        && isolated_schema
        && arguments.iter().filter(|value| value.as_str() == "--disable").count() >= 16
        && stdin.starts_with("Peritus is the sole host agent")
}

pub(super) fn environment_absent() -> bool {
    [
        "OPENAI_API_KEY",
        "CODEX_API_KEY",
        "CODEX_ACCESS_TOKEN",
        "OPENAI_BASE_URL",
        "OPENAI_API_BASE",
        "OPENAI_ORG_ID",
        "OPENAI_ORGANIZATION",
        "OPENAI_PROJECT_ID",
    ]
    .iter()
    .all(|name| std::env::var_os(name).is_none())
}

pub(super) fn argument_value<'a>(arguments: &'a [String], name: &str) -> Option<&'a str> {
    arguments.windows(2).find(|pair| pair[0] == name).map(|pair| pair[1].as_str())
}
