//! Test-only Claude command behavior and invocation assertions.

use std::fs::OpenOptions;
use std::io::{Read as _, Write as _};
use std::path::PathBuf;
use std::time::Duration;

const AUTHENTICATED: &str = r#"{"loggedIn":true}"#;
const UNAUTHENTICATED: &str = r#"{"loggedIn":false}"#;
const SUCCESS: &str = r#"{"is_error":false,"structured_output":{"content":"runtime response","tool_calls":[]},"usage":{"cache_creation_input_tokens":3,"cache_read_input_tokens":4,"input_tokens":12,"output_tokens":7}}"#;
const INCOMPLETE: &str = r#"{"is_error":false,"result":"unstructured output"}"#;

pub fn run() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments == ["auth", "status", "--json"] {
        trace("auth");
        let authenticated = environment_absent() && !executable_name().contains("authentication");
        emit(if authenticated { AUTHENTICATED } else { UNAUTHENTICATED }, 0);
    }
    let mut stdin = String::new();
    if std::io::stdin().read_to_string(&mut stdin).is_err() || !valid_turn(&arguments, &stdin) {
        emit("", 90);
    }
    trace("turn");
    let model = argument_value(&arguments, "--model").unwrap_or_default();
    if model.contains("cancellation") {
        trace("spin");
        loop {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    if model.contains("rate-limit") && attempt(model) == 1 {
        trace("failure-rate-limited-250");
        emit(r#"{"is_error":true,"result":"rate limited"}"#, 0);
    } else if model.contains("transient") && attempt(model) == 1 {
        trace("failure-transient-0");
        emit(r#"{"is_error":true,"result":"temporarily unavailable"}"#, 0);
    }
    let (output, exit) = scenario(model);
    emit(&output, exit);
}

fn valid_turn(arguments: &[String], stdin: &str) -> bool {
    let required = [
        "-p",
        "--output-format",
        "json",
        "--safe-mode",
        "--tools",
        "--disallowedTools",
        "mcp__*",
        "--disable-slash-commands",
        "--no-chrome",
        "--no-session-persistence",
        "--strict-mcp-config",
        "--mcp-config",
        r#"{"mcpServers":{}}"#,
        "--system-prompt-file",
        "--json-schema",
    ];
    let cwd = std::env::current_dir().ok();
    let system = argument_value(arguments, "--system-prompt-file").map(PathBuf::from);
    let isolated_system = system.as_ref().is_some_and(|path| {
        path.is_file() && cwd.as_ref().is_some_and(|directory| path.parent() == Some(directory))
    });
    let system_owned = system
        .as_deref()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .is_some_and(|value| value.contains("sole agent harness"));
    let schema_is_inert = argument_value(arguments, "--json-schema").is_some_and(|schema| {
        schema.contains("tool_calls") && schema.contains("additionalProperties")
    });
    required.iter().all(|value| arguments.iter().any(|argument| argument == value))
        && argument_pair(arguments, "--tools", "")
        && environment_absent()
        && isolated_system
        && system_owned
        && schema_is_inert
        && stdin.starts_with("The following JSON is the complete ordered conversation state")
}

fn environment_absent() -> bool {
    ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "CLAUDE_CODE_OAUTH_TOKEN"]
        .iter()
        .all(|name| std::env::var_os(name).is_none())
}

fn scenario(model: &str) -> (String, i32) {
    if model.contains("malformed") {
        return ("{not-json}".to_owned(), 0);
    }
    if model.contains("incomplete") {
        return (INCOMPLETE.to_owned(), 0);
    }
    if model.contains("interruption") {
        return (r#"{"is_error":false"#.to_owned(), 7);
    }
    if model.contains("ambiguous") {
        return (String::new(), 7);
    }
    if model.contains("fragmented") {
        let payload = "x".repeat(5_000);
        return (
            format!(
                r#"{{"is_error":false,"structured_output":{{"content":"calling lookup","tool_calls":[{{"arguments":{{"id":"{payload}"}},"name":"lookup"}}]}},"usage":{{"input_tokens":12,"output_tokens":7}}}}"#
            ),
            0,
        );
    }
    (SUCCESS.to_owned(), 0)
}

fn argument_value<'a>(arguments: &'a [String], name: &str) -> Option<&'a str> {
    arguments.windows(2).find(|pair| pair[0] == name).map(|pair| pair[1].as_str())
}

fn argument_pair(arguments: &[String], name: &str, value: &str) -> bool {
    arguments.windows(2).any(|pair| pair[0] == name && pair[1] == value)
}

fn emit(output: &str, exit: i32) -> ! {
    let _ = std::io::stdout().write_all(output.as_bytes());
    let _ = std::io::stdout().flush();
    std::process::exit(exit);
}

fn trace(value: &str) {
    let Some(path) = trace_path() else {
        return;
    };
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = file.write_all(value.as_bytes());
    let _ = file.write_all(b"\n");
    let _ = file.flush();
}

fn trace_path() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|path| path.parent().map(|parent| parent.join("trace")))
}

fn attempt(model: &str) -> u64 {
    if !model.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-') {
        return 1;
    }
    let Some(directory) =
        std::env::current_exe().ok().and_then(|path| path.parent().map(PathBuf::from))
    else {
        return 1;
    };
    let path = directory.join(format!("{model}.attempt"));
    let previous = std::fs::read_to_string(&path)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let next = previous.saturating_add(1);
    let _ = std::fs::write(path, next.to_string());
    next
}

fn executable_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.file_name().map(|name| name.to_string_lossy().into_owned()))
        .unwrap_or_default()
}
