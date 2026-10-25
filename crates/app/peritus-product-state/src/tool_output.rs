//! Human-readable tool observations shared by terminal and daemon presentation.
//!
//! This projection never changes the tool result supplied to the model or the private trace.

use serde_json::Value;
use std::fmt::Write as _;

/// Whether all returned sources are explicitly identified Markdown documents.
#[must_use]
pub fn is_markdown(value: &Value) -> bool {
    markdown_source(value, 0)
}

fn markdown_source(value: &Value, depth: u8) -> bool {
    if ["path", "section"].iter().any(|field| {
        value[field].as_str().is_some_and(|path| path.to_ascii_lowercase().ends_with(".md"))
    }) {
        return true;
    }
    depth < 4
        && value["sources"].as_array().is_some_and(|sources| {
            !sources.is_empty()
                && sources.iter().all(|source| {
                    source["data"].as_str().map_or_else(
                        || markdown_source(&source["data"], depth + 1),
                        |data| {
                            serde_json::from_str::<Value>(data)
                                .is_ok_and(|value| markdown_source(&value, depth + 1))
                        },
                    )
                })
        })
}

/// Formats useful returned data without hiding bodies, search hits, or directory entries.
#[must_use]
pub fn render(value: &Value) -> String {
    render_value(value, 0, false)
}

/// Projects saved Markdown sources for display, removing only native file-read line prefixes.
/// Skill bytes and genuine document text (including hash-like lines) remain intact.
#[must_use]
pub fn render_markdown(value: &Value) -> String {
    render_value(value, 0, true)
}

fn render_value(value: &Value, depth: u8, markdown: bool) -> String {
    let mut text = String::new();
    if depth < 4
        && let Some(sources) = value["sources"].as_array()
    {
        for source in sources {
            let handle = source["handle"]
                .as_str()
                .or_else(|| source["source"].as_str())
                .unwrap_or("retained source");
            let _ = writeln!(text, "{handle}");
            let rendered = source["data"].as_str().map_or_else(
                || render_value(&source["data"], depth + 1, markdown),
                |data| {
                    serde_json::from_str::<Value>(data).map_or_else(
                        |_| data.to_owned(),
                        |decoded| render_value(&decoded, depth + 1, markdown),
                    )
                },
            );
            text.push_str(&rendered);
            text.push('\n');
        }
    } else if let Some(content) = value["content"].as_str().or_else(|| value["bytes"].as_str()) {
        if markdown
            && value["content"].is_string()
            && value["path"].as_str().is_some_and(|path| path.to_ascii_lowercase().ends_with(".md"))
        {
            for (index, line) in content.lines().enumerate() {
                if index > 0 {
                    text.push('\n');
                }
                let body = line
                    .split_once(": ")
                    .filter(|(number, _)| {
                        !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit())
                    })
                    .map_or(line, |(_, body)| body);
                text.push_str(body);
            }
        } else {
            text.push_str(content);
        }
    } else if let Some(matches) = value["matches"].as_array() {
        if matches.is_empty() {
            text.push_str("No matches.");
        }
        for hit in matches {
            let _ = writeln!(
                text,
                "{}:{}: {}",
                hit["path"].as_str().unwrap_or("?"),
                hit["line"].as_u64().unwrap_or(0),
                hit["text"].as_str().unwrap_or("")
            );
        }
    } else if let Some(fragment) = value["entry_json_fragment"]["data"].as_str() {
        text.push_str("Saved context entry (JSON fragment)\n");
        text.push_str(fragment);
    } else if let Some(entries) = value["entries"].as_array() {
        if entries.is_empty() {
            text.push_str(if value["authority"] == "none" && value["base_revision"].is_u64() {
                "No saved context entries."
            } else {
                "Empty directory."
            });
        }
        for entry in entries {
            render_entry(&mut text, entry);
        }
    } else {
        return serde_json::to_string_pretty(value)
            .unwrap_or_else(|_| "Result unavailable".to_owned());
    }
    for field in ["error", "source_sha256", "sha256"] {
        if let Some(value) = value[field].as_str() {
            let _ = write!(text, "\n{field}: {value}");
        }
    }
    if value["truncated"].as_bool() == Some(true)
        || !value["next_line"].is_null()
        || !value["next_cursor"].is_null()
        || !value["cursor"].is_null()
    {
        let _ = write!(text, "\nTool returned a partial page");
        if let Some(next) = value["next_line"].as_u64() {
            let _ = write!(text, "; next line {next}");
        }
        text.push('.');
    }
    if let Some(omissions) = value["omissions"].as_array().filter(|items| !items.is_empty()) {
        let _ = write!(text, "\nOmissions: {}", Value::Array(omissions.clone()));
    }
    text
}

fn render_entry(text: &mut String, entry: &Value) {
    if let (Some(id), Some(kind), Some(status), Some(body)) = (
        entry["id"].as_str(),
        entry["kind"].as_str(),
        entry["status"].as_str(),
        entry["text"].as_str(),
    ) {
        let _ = writeln!(text, "{kind} · {status} [{id}]\n{body}");
        if let Some(next) = entry["next_text_offset"].as_u64() {
            let _ =
                writeln!(text, "Text continues at byte {next} of {}.", entry["text_total_bytes"]);
        }
    } else if let Some(path) = entry["path"].as_str().or_else(|| entry.as_str()) {
        let _ = writeln!(
            text,
            "{path}{}",
            if entry["kind"].as_str() == Some("directory") { "/" } else { "" }
        );
    } else {
        // `entries` is shared by multiple tools; preserve an unfamiliar record instead of
        // inventing a missing directory path or discarding its actual content.
        let _ = writeln!(
            text,
            "{}",
            serde_json::to_string_pretty(entry).unwrap_or_else(|_| "Result unavailable".to_owned())
        );
    }
}
