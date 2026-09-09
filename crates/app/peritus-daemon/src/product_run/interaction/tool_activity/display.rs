//! Human-readable command/argv projections; these strings are display-only, never shell input.
use serde_json::Value;

pub(super) fn summary(name: &str, arguments: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(arguments) else {
        return format!("{name} (invalid arguments)");
    };
    let rendered = if matches!(name, "run_command" | "command_start") {
        let mut parts = vec![quote(value["program"].as_str().unwrap_or("<missing program>"))];
        let mut hide_next = false;
        for argument in value["args"].as_array().into_iter().flatten() {
            let argument = argument.as_str().unwrap_or("<invalid argument>");
            parts.push(quote(if hide_next { "[redacted]" } else { argument }));
            hide_next = sensitive_key(argument.trim_start_matches('-'));
        }
        parts.join(" ")
    } else {
        let mut parts = vec![name.to_owned()];
        for field in ["path", "paths", "pattern", "handle", "signal", "start_line", "end_line"] {
            if let Some(value) = value.get(field) {
                parts.push(format!(
                    "{field}={}",
                    quote(value.as_str().map_or_else(|| value.to_string(), str::to_owned).as_str())
                ));
            }
        }
        parts.join(" ")
    };
    let rendered = value["cwd"]
        .as_str()
        .map_or_else(|| rendered.clone(), |cwd| format!("{rendered} (cwd {})", quote(cwd)));
    // File replacement bodies, stdin, and memory payloads are deliberately not command labels.
    super::super::bounded(&safe(&rendered).replace('\n', " "))
}

pub(super) fn result(name: &str, output: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(output) else {
        return "Tool returned an invalid result; no output preview available.".to_owned();
    };
    if name == "run_command" || name.starts_with("command_") {
        let mut metadata = Vec::new();
        if let Some(code) = value["exit_code"].as_i64() {
            metadata.push(format!("Exit code: {code}"));
        }
        if value["timed_out"].as_bool() == Some(true) {
            metadata.push("Timed out".to_owned());
        }
        for field in ["state", "handle"] {
            if let Some(text) = value[field].as_str().filter(|text| !text.is_empty()) {
                metadata.push(format!("{field}: {text}"));
            }
        }
        let mut parts = if metadata.is_empty() { Vec::new() } else { vec![metadata.join(" · ")] };
        for field in ["error", "stdout", "stderr"] {
            if let Some(text) = value[field].as_str().filter(|text| !text.is_empty()) {
                parts.push(format!("{field}:\n{text}"));
            }
        }
        if !parts.is_empty() {
            return safe(&parts.join("\n"));
        }
    }
    safe(&serde_json::to_string_pretty(&value).unwrap_or_else(|_| "Result unavailable".to_owned()))
}

fn quote(value: &str) -> String {
    let value = value
        .chars()
        .map(|character| {
            if character.is_control() {
                character.escape_default().to_string()
            } else {
                character.to_string()
            }
        })
        .collect::<String>();
    if !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"/_-.:@+".contains(&byte))
    {
        value
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn sensitive_key(key: &str) -> bool {
    matches!(
        key.trim_start_matches('-').to_ascii_lowercase().replace('-', "_").as_str(),
        "password"
            | "passwd"
            | "token"
            | "access_token"
            | "api_key"
            | "apikey"
            | "secret"
            | "client_secret"
            | "authorization"
            | "cookie"
    )
}

// Accidental-disclosure defense, not a complete credential detector. Never reads secret stores.
// Raw authorized tool bytes remain in the existing protected trace; the public preview is bounded.
fn safe(text: &str) -> String {
    let mut private_key = false;
    let mut lines = Vec::new();
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        let begin = lower.contains("-----begin ") && lower.contains("private key-----");
        let end = lower.contains("-----end ") && lower.contains("private key-----");
        let sensitive = begin
            || private_key
            || ["authorization:", "bearer ", "ghp_", "github_pat_", "sk-proj-"]
                .iter()
                .any(|marker| lower.contains(marker))
            || lower.split(|c: char| !(c.is_alphanumeric() || "_-=:\"".contains(c))).any(|word| {
                word.split_once(['=', ':'])
                    .is_some_and(|(key, _)| sensitive_key(key.trim_matches('"')))
            });
        lines.push(if sensitive { "[credential-like content redacted]" } else { line });
        private_key = (private_key || begin) && !end;
    }
    bounded_preview(&lines.join("\n"))
}

fn bounded_preview(text: &str) -> String {
    const MAXIMUM: usize = peritus_app_protocol::MAX_PRODUCT_ACTIVITY_BYTES;
    const MARKER: &str = "\n… preview truncated …\n";
    if text.len() <= MAXIMUM {
        return text.to_owned();
    }
    let half = (MAXIMUM - MARKER.len()) / 2;
    let mut head = half;
    let mut tail = text.len() - half;
    while !text.is_char_boundary(head) {
        head -= 1;
    }
    while !text.is_char_boundary(tail) {
        tail += 1;
    }
    format!("{}{MARKER}{}", &text[..head], &text[tail..])
}
