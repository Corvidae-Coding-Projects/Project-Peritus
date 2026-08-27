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
    let isolated_schema = schema.as_deref().is_some_and(is_file_in_working_directory);
    required_present
        && environment_absent()
        && isolated_schema
        && arguments.iter().filter(|value| value.as_str() == "--disable").count() == 15
        && !arguments.iter().any(|value| value == "code_mode_host")
        && stdin.starts_with("Peritus is the sole host agent")
}

fn is_file_in_working_directory(path: &std::path::Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    path.is_file()
        && std::fs::canonicalize(parent).ok()
            == std::env::current_dir().and_then(std::fs::canonicalize).ok()
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
