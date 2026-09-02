//! Request-derived restrictions for inputs exposed only through a named public interface.

use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

use serde_json::Value;

use super::executor::WorkspaceDeveloperTools;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct WorkspaceAccessPolicy {
    opaque_paths: BTreeSet<PathBuf>,
    hidden_identifiers: BTreeSet<String>,
}

impl WorkspaceDeveloperTools {
    #[must_use]
    pub(crate) fn with_task_contract(mut self, transcript: &str) -> Self {
        self.access_policy = WorkspaceAccessPolicy::from_transcript(&self.root, transcript);
        self
    }
}

impl WorkspaceAccessPolicy {
    pub(super) fn from_transcript(root: &Path, transcript: &str) -> Self {
        let lower = transcript.to_ascii_lowercase();
        let restricted_knowledge = ["do not know", "don't know", "black-box", "black box"]
            .iter()
            .any(|marker| lower.contains(marker));
        if !restricted_knowledge {
            return Self::default();
        }

        let mut policy = Self::default();
        for line in transcript.lines() {
            let lower_line = line.to_ascii_lowercase();
            if lower_line.contains("query")
                && lower_line.contains("import")
                && lower_line.contains("call")
            {
                for span in inline_code_spans(line) {
                    if let Some(path) = opaque_path(root, span) {
                        policy.opaque_paths.insert(path);
                    }
                }
            }
        }
        for marker in ["do not know", "don't know"] {
            for offset in match_offsets(&lower, marker) {
                let start = offset.saturating_add(marker.len());
                let clause = transcript[start..].split(['.', ';', '\n']).next().unwrap_or_default();
                policy.hidden_identifiers.extend(
                    clause
                        .split(|character: char| {
                            !character.is_ascii_alphanumeric() && character != '_'
                        })
                        .filter(|token| hidden_identifier(token))
                        .map(str::to_owned),
                );
            }
        }
        policy
    }

    pub(super) fn authorize(&self, tool: &str, arguments: &Value) -> Result<(), String> {
        match tool {
            "workspace_read" | "workspace_write" | "workspace_patch" | "workspace_remove" => {
                if let Some(path) = arguments.get("path").and_then(Value::as_str) {
                    self.authorize_path(path)?;
                }
            }
            "run_command" => self.authorize_command(arguments)?,
            _ => {}
        }
        Ok(())
    }

    pub(super) fn permits_search_result(&self, relative: &Path) -> bool {
        !self.opaque_paths.contains(relative)
    }

    fn authorize_path(&self, raw: &str) -> Result<(), String> {
        let relative = normalized_relative(raw);
        if self.opaque_paths.contains(&relative) {
            return Err(format!(
                "the task declares {} as an opaque query interface; inspect behavior only through its named public interface",
                relative.display(),
            ));
        }
        Ok(())
    }

    fn authorize_command(&self, arguments: &Value) -> Result<(), String> {
        let values = arguments.get("program").and_then(Value::as_str).into_iter().chain(
            arguments
                .get("args")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        );
        for value in values {
            if let Some(path) = self
                .opaque_paths
                .iter()
                .find(|path| value.contains(path.to_string_lossy().as_ref()))
            {
                return Err(format!(
                    "the task declares {} as an opaque query interface; commands may invoke its public interface but may not inspect the implementation path",
                    path.display(),
                ));
            }
            if let Some(identifier) = self
                .hidden_identifiers
                .iter()
                .find(|identifier| contains_identifier(value, identifier))
            {
                return Err(format!(
                    "the task declares implementation detail {identifier} unknown; validate through the named public interface instead of inspecting hidden state",
                ));
            }
        }
        Ok(())
    }
}

fn inline_code_spans(line: &str) -> Vec<&str> {
    line.split('`')
        .enumerate()
        .filter_map(|(index, span)| (index % 2 == 1 && !span.is_empty()).then_some(span))
        .collect()
}

fn opaque_path(root: &Path, raw: &str) -> Option<PathBuf> {
    if raw.contains('(') || raw.contains(')') {
        return None;
    }
    let path = Path::new(raw);
    path.extension()?;
    let relative = if path.is_absolute() {
        path.strip_prefix(root).ok()?.to_path_buf()
    } else {
        path.to_path_buf()
    };
    (!relative.as_os_str().is_empty()
        && !relative.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        }))
    .then_some(relative)
}

fn normalized_relative(raw: &str) -> PathBuf {
    Path::new(raw.strip_prefix("./").unwrap_or(raw)).to_path_buf()
}

fn match_offsets(text: &str, marker: &str) -> Vec<usize> {
    text.match_indices(marker).map(|(offset, _)| offset).collect()
}

fn hidden_identifier(token: &str) -> bool {
    !token.is_empty()
        && token.chars().all(|character| character.is_ascii_alphanumeric() || character == '_')
        && token
            .chars()
            .any(|character| character.is_ascii_uppercase() || character.is_ascii_digit())
}

fn contains_identifier(text: &str, identifier: &str) -> bool {
    text.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| token == identifier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_query_contract_blocks_implementation_reads_and_hidden_state() {
        let policy = WorkspaceAccessPolicy::from_transcript(
            Path::new("/app"),
            "Query the system by importing `forward.py` and calling forward(x). You do not know the shape of A1.",
        );

        assert!(policy.authorize("workspace_read", &value(r#"{"path":"forward.py"}"#)).is_err());
        assert!(policy.authorize("workspace_read", &value(r#"{"path":"steal.py"}"#)).is_ok());
        assert!(
            policy
                .authorize(
                    "run_command",
                    &value(
                        r#"{"args":["-c","import forward; print(forward.A1)"],"program":"python3"}"#,
                    ),
                )
                .is_err()
        );
        assert!(policy
            .authorize(
                "run_command",
                &value(
                    r#"{"args":["-c","from forward import forward; print(callable(forward))"],"program":"python3"}"#,
                ),
            )
            .is_ok());
        assert!(!policy.permits_search_result(Path::new("forward.py")));
        assert!(policy.permits_search_result(Path::new("steal.py")));
    }

    #[test]
    fn ordinary_repository_request_keeps_normal_access() {
        let policy = WorkspaceAccessPolicy::from_transcript(
            Path::new("/work"),
            "Inspect src/lib.rs, fix the parser, and run its tests.",
        );

        assert!(policy.authorize("workspace_read", &value(r#"{"path":"src/lib.rs"}"#)).is_ok());
        assert!(
            policy
                .authorize("run_command", &value(r#"{"args":["test"],"program":"cargo"}"#),)
                .is_ok()
        );
    }

    fn value(json: &str) -> Value {
        serde_json::from_str(json).expect("test JSON")
    }
}
