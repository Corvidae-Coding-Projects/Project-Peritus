//! Request-derived restrictions for inputs exposed only through a named public interface.

use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

use serde_json::Value;

use super::executor::WorkspaceDeveloperTools;
use super::command_runtime::CommandExecutionMode;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct WorkspaceAccessPolicy {
    protected_paths: BTreeSet<PathBuf>,
    hard_constraint_paths: BTreeSet<PathBuf>,
}

impl WorkspaceDeveloperTools {
    #[must_use]
    pub(crate) fn with_task_contract(self, _transcript: &str) -> Self {
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
        self.restore_request_source_evidence();
        self
    }

    pub(super) fn refresh_hard_constraints(&mut self) {
        if let Some(view) = &self.protection_view {
            self.access_policy.set_hard_constraints(&self.root, &view.protected_paths());
        }
    }

    pub(super) fn observe_request_authority(&mut self, text: &str) {
        self.references.extend_from_task(&self.root, text);
    }

    pub(super) fn command_confinement(&self, mode: CommandExecutionMode) -> Vec<PathBuf> {
        if mode.is_mutation() {
            self.access_policy.hard_constraint_paths.iter().cloned().collect()
        } else {
            Vec::new()
        }
    }

    pub(super) fn authorize_process_control_confinement(
        &self,
        tool: &str,
        arguments: &Value,
        process_mode: Option<CommandExecutionMode>,
    ) -> Result<(), String> {
        if process_mode != Some(CommandExecutionMode::Mutation)
            || !matches!(tool, "command_stdin" | "command_resize" | "command_signal")
        {
            return Ok(());
        }
        let handle = arguments
            .get("handle")
            .and_then(Value::as_str)
            .ok_or_else(|| "process control is missing its exact command handle".to_owned())?;
        let admitted = self
            .command_runtime
            .as_ref()
            .ok_or_else(|| "process controls have no command runtime".to_owned())?
            .control_confinement(handle)
            .map_err(|error| error.to_string())?;
        let Some(admitted) = admitted else { return Ok(()) };
        self.access_policy.authorize_control_confinement(handle, &admitted)
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

    fn authorize_control_confinement(
        &self,
        handle: &str,
        admitted: &[PathBuf],
    ) -> Result<(), String> {
        let missing = self.hard_constraint_paths.iter().find(|required| {
            !admitted.iter().any(|protected| required.starts_with(protected))
        });
        if let Some(missing) = missing {
            return Err(format!(
                "process control refused for handle {handle}: its retained sandbox predates the current leave-alone constraint for {}; cancel that exact handle and start a new confined command",
                missing.display(),
            ));
        }
        Ok(())
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
    pub(super) fn authorize(&self, tool: &str, arguments: &Value) -> Result<(), String> {
        let inferred_mode = match arguments.get("purpose").and_then(Value::as_str) {
            Some("verification") => Some(CommandExecutionMode::Observational),
            Some("external_effect") => Some(CommandExecutionMode::Mutation),
            _ => None,
        };
        self.authorize_with_process_mode(tool, arguments, inferred_mode)
    }

    pub(super) fn authorize_with_process_mode(
        &self,
        tool: &str,
        arguments: &Value,
        _process_mode: Option<CommandExecutionMode>,
    ) -> Result<(), String> {
        match tool {
            "workspace_list" | "workspace_search" | "workspace_read" => {
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
            "run_command" | "command_start" | "command_stdin" | "command_resize"
            | "command_signal" => {}
            _ => {}
        }
        Ok(())
    }

    pub(super) fn permits_search_result(&self, relative: &Path) -> bool {
        !self.protected_paths.iter().any(|path| relative.starts_with(path))
    }

    fn authorize_path(&self, raw: &str) -> Result<(), String> {
        let relative = normalized_relative(raw);
        if self.protected_paths.iter().any(|path| relative.starts_with(path)) {
            return Err(
                "Peritus private state is not an ordinary workspace file-tool target".to_owned()
            );
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

}

fn normalized_relative(raw: &str) -> PathBuf {
    Path::new(raw.strip_prefix("./").unwrap_or(raw)).to_path_buf()
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_repository_request_keeps_normal_access() {
        let policy = WorkspaceAccessPolicy::default();

        assert!(policy.authorize("workspace_read", &value(r#"{"path":"src/lib.rs"}"#)).is_ok());
        assert!(
            policy
                .authorize("run_command", &value(r#"{"args":["test"],"program":"cargo"}"#),)
                .is_ok()
        );
    }

    #[test]
    fn hard_constraint_allows_read_and_requires_process_confinement() {
        let mut policy = WorkspaceAccessPolicy::default();
        policy.set_hard_constraints(Path::new("/work"), &[PathBuf::from("src/locked.rs")]);

        assert!(
            policy.authorize("workspace_read", &value(r#"{"path":"src/locked.rs"}"#)).is_ok(),
            "explanation must retain read-only source access"
        );
        assert!(
            policy
                .authorize("workspace_patch", &value(r#"{"path":"src/locked.rs"}"#))
                .expect_err("hard path write")
                .contains("leave-alone")
        );
        assert!(policy.authorize("run_command", &value(r#"{"args":[],"program":"true"}"#)).is_ok());
        assert!(policy
            .authorize_control_confinement("owned", &[PathBuf::from("src/locked.rs")])
            .is_ok());
        assert!(policy.authorize_control_confinement("old", &[]).is_err());
        assert!(policy.authorize("workspace_patch", &value(r#"{"path":"src/other.rs"}"#)).is_ok());
    }

    #[test]
    fn existing_private_read_exclusions_do_not_change_process_authority() {
        let mut policy = WorkspaceAccessPolicy::default();
        policy.protect(Path::new("/work"), &[PathBuf::from("/work/.peritus")]);
        assert!(
            policy
                .authorize("run_command", &value(r#"{"args":["test"],"program":"cargo"}"#))
                .is_ok()
        );
    }

    fn value(json: &str) -> Value {
        serde_json::from_str(json).expect("test JSON")
    }
}
