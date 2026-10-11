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
        "--output-last-message",
        "-",
    ];
    let required_present = required.iter().all(|value| arguments.iter().any(|item| item == value));
    let schema = argument_value(arguments, "--output-schema").map(PathBuf::from);
    let isolated_schema = schema.as_deref().is_some_and(is_file_in_working_directory);
    required_present
        && environment_absent()
        && isolated_schema
        && native_tools_disabled(arguments)
        && !arguments.iter().any(|value| value == "code_mode_host")
        && stdin.starts_with("Peritus is the sole host agent")
}

fn native_tools_disabled(arguments: &[String]) -> bool {
    let features = [
        "shell_tool",
        "unified_exec",
        "apps",
        "plugins",
        "multi_agent",
        "browser_use",
        "computer_use",
        "image_generation",
        "view_image",
        "hooks",
        "skill_search",
        "skill_mcp_dependency_install",
        "tool_call_mcp_elicitation",
        "request_permissions_tool",
        "code_mode",
        "sleep_tool",
        "tool_suggest",
    ];
    features.iter().all(|feature| arguments.windows(2).any(|pair| pair == ["--disable", feature]))
        && [
            "tools.update_plan.enabled=false",
            "tools.experimental_request_user_input.enabled=false",
            "web_search=\"disabled\"",
        ]
        .iter()
        .all(|setting| arguments.windows(2).any(|pair| pair == ["--config", setting]))
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
