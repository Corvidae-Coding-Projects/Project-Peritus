//! Request-derived restrictions for inputs exposed only through a named public interface.

use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

use serde_json::Value;

use super::executor::WorkspaceDeveloperTools;

/// Reapply the same current read policy before presenting an archived source page.
pub(crate) fn may_reuse_read(
    root: &Path,
    path: &str,
    protected: &[PathBuf],
    view: &dyn crate::ConversationView,
) -> bool {
    if !view.effective_permissions().allows(crate::control::PermissionCapability::Read) {
        return false;
    }
    let mut policy =
        WorkspaceAccessPolicy::from_transcript(root, &view.reference_authority_context());
    policy.protect(root, protected);
    policy.set_hard_constraints(root, &view.protected_paths());
    policy.authorize("workspace_read", &serde_json::json!({"path":path})).is_ok()
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct WorkspaceAccessPolicy {
    protected_paths: BTreeSet<PathBuf>,
    hard_constraint_paths: BTreeSet<PathBuf>,
    opaque_paths: BTreeSet<PathBuf>,
    hidden_identifiers: BTreeSet<String>,
}

impl WorkspaceDeveloperTools {
    #[must_use]
    pub(crate) fn with_task_contract(mut self, transcript: &str) -> Self {
        let protected = std::mem::take(&mut self.access_policy.protected_paths);
        let hard_constraints = std::mem::take(&mut self.access_policy.hard_constraint_paths);
        self.access_policy = WorkspaceAccessPolicy::from_transcript(&self.root, transcript);
        self.access_policy.protected_paths = protected;
        self.access_policy.hard_constraint_paths = hard_constraints;
        self
    }

    pub(crate) fn with_protected_paths(mut self, paths: &[PathBuf]) -> Self {
        self.access_policy.protect(&self.root, paths);
        self
    }

    pub(crate) fn with_protection_view(
        mut self,
        view: std::sync::Arc<dyn crate::ConversationView>,
    ) -> Self {
        self.access_policy.set_hard_constraints(&self.root, &view.protected_paths());
        self.protection_view = Some(view);
        self
    }

    pub(super) fn refresh_hard_constraints(&mut self) {
        if let Some(view) = &self.protection_view {
            self.access_policy.set_hard_constraints(&self.root, &view.protected_paths());
        }
    }
}

impl WorkspaceAccessPolicy {
    pub(super) fn protect(&mut self, root: &Path, paths: &[PathBuf]) {
        for path in paths {
            if let Some(relative) = configured_relative(root, path) {
                self.protected_paths.insert(relative);
            }
        }
    }

    fn set_hard_constraints(&mut self, root: &Path, paths: &[PathBuf]) {
        self.hard_constraint_paths.clear();
        self.hard_constraint_paths
            .extend(paths.iter().filter_map(|path| configured_relative(root, path)));
    }
}

fn configured_relative(root: &Path, path: &Path) -> Option<PathBuf> {
    if path.is_absolute() {
        if let Ok(relative) = path.strip_prefix(root) {
            return Some(relative.to_owned());
        }
        return root.starts_with(path).then(PathBuf::new);
    }
    (!path.components().any(|component| {
        matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
    }))
    .then(|| path.to_owned())
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
            "workspace_list" | "workspace_search" | "workspace_read" | "view_image" => {
                if let Some(path) = arguments.get("path").and_then(Value::as_str) {
                    self.authorize_path(path)?;
                }
            }
            "workspace_write" | "workspace_patch" | "workspace_remove" => {
                if let Some(path) = arguments.get("path").and_then(Value::as_str) {
                    self.authorize_path(path)?;
                    self.authorize_mutation_path(path)?;
                }
            }
            "run_command" | "command_start" | "command_stdin" => {
                self.authorize_command(arguments)?;
            }
            "browser_control" | "desktop_control" => {
                // GUI input is an unconstrained host effect, just like a command. A live
                // leave-alone constraint must not be bypassed through an editor window.
                self.authorize_command(arguments)?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn permits_search_result(&self, relative: &Path) -> bool {
        !self.opaque_paths.contains(relative)
            && !self.protected_paths.iter().any(|path| relative.starts_with(path))
    }

    fn authorize_path(&self, raw: &str) -> Result<(), String> {
        let relative = normalized_relative(raw);
        if self.protected_paths.iter().any(|path| relative.starts_with(path)) {
            return Err(
                "Peritus private state is not an ordinary workspace file-tool target".to_owned()
            );
        }
        if self.opaque_paths.contains(&relative) {
            return Err(format!(
                "the task declares {} as an opaque query interface; inspect behavior only through its named public interface",
                relative.display(),
            ));
        }
        Ok(())
    }

    fn authorize_mutation_path(&self, raw: &str) -> Result<(), String> {
        let relative = normalized_relative(raw);
        if self.hard_constraint_paths.iter().any(|path| relative.starts_with(path)) {
            return Err(format!(
                "{} is protected by an explicit leave-alone review constraint; dismiss or rebind that exact constraint before requesting a write",
                relative.display(),
            ));
        }
        Ok(())
    }

    fn authorize_command(&self, arguments: &Value) -> Result<(), String> {
        if !self.hard_constraint_paths.is_empty() {
            return Err(
                "process admission refused: the configured command backend cannot confine writes away from protected paths; explicitly revise the constraint or use enforceable file tools"
                    .to_owned(),
            );
        }
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
