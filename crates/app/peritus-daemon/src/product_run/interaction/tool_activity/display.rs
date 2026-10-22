//! Human-readable command/argv projections; these strings are display-only, never shell input.
use serde_json::Value;

/// Only bounded proposed text is retained; completion requires a matching host result.
#[derive(Clone)]
pub(super) struct ChangePreview {
    name: String,
    path: String,
    preview: String,
}

impl ChangePreview {
    pub(super) fn capture(name: &str, arguments: &str) -> Option<Self> {
        if !matches!(name, "workspace_patch" | "workspace_write" | "workspace_remove") {
            return None;
        }
        let value: Value = serde_json::from_str(arguments).ok()?;
        let path = value["path"].as_str()?.to_owned();
        let preview = if name == "workspace_patch" {
            format!(
                "@@ replaced fragment @@\n{}\n{}",
                fragment(value["old"].as_str()?, "-"),
                fragment(value["new"].as_str()?, "+")
            )
        } else if name == "workspace_write" {
            format!("Contents written (preview):\n{}", fragment(value["content"].as_str()?, " "))
        } else {
            String::new()
        };
        Some(Self { name: name.to_owned(), path, preview: safe(&preview) })
    }

    pub(super) fn finished(&self, result: &Value) -> Option<(String, String)> {
        if result["path"].as_str()? != self.path {
            return None;
        }
        if self.name == "workspace_remove" {
            let kind = result["kind"].as_str()?;
            return Some((
                safe(&format!("Removed {}", self.path)),
                safe(&format!("Removed {kind}: {}", self.path)),
            ));
        }
        if self.name == "workspace_write" && result["changed"].as_bool() == Some(false) {
            return Some((
                safe(&format!("Unchanged {}", self.path)),
                "Requested contents already match; no file change.".to_owned(),
            ));
        }
        result["sha256"].as_str()?;
        let bytes = result["bytes"].as_u64()?;
        let text = if self.name == "workspace_patch" {
            let count = result["replacements"].as_u64()?;
            let location =
                result["start_line"].as_u64().map_or_else(String::new, |line| format!(":{line}"));
            format!("Updated {}{location} · {count} replacement(s)", self.path)
        } else {
            if result["changed"].as_bool() != Some(true) {
                return None;
            }
            format!("Wrote {} · {bytes} bytes", self.path)
        };
        Some((safe(&text), safe(&format!("{}\nResulting file: {bytes} bytes", self.preview))))
    }
}

fn fragment(text: &str, prefix: &str) -> String {
    let mut lines = text.lines().map(|line| format!("{prefix}{line}")).collect::<Vec<_>>();
    if text.is_empty() {
        lines.push(format!("{prefix}[empty]"));
    }
    lines.join("\n")
}

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
        for field in [
            "path",
            "paths",
            "query",
            "pattern",
            "skill",
            "handle",
            "signal",
            "start_line",
            "end_line",
        ] {
            if let Some(value) = value.get(field) {
                if value.is_null() {
                    continue;
                }
                let display = if field == "query" && value.is_string() {
                    value.to_string()
                } else {
                    quote(value.as_str().map_or_else(|| value.to_string(), str::to_owned).as_str())
                };
                parts.push(format!("{field}={}", display));
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
    if name == "context_read" && peritus_product_runner::tool_output::is_markdown(&value) {
        safe(&peritus_product_runner::tool_output::render_markdown(&value))
    } else {
        safe(&peritus_product_runner::tool_output::render(&value))
    }
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
    lines.join("\n")
}
