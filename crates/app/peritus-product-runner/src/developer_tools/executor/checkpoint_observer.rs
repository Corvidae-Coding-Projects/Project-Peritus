//! Candidate-checkpoint classification for accepted developer tool effects.

use std::{fs, sync::Arc};

use peritus_agent::DeveloperLoopError;
use peritus_types::Sha256Digest;
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use super::WorkspaceDeveloperTools;
use crate::developer_tools::{
    path::{checked, tool},
    wire::{required_string, string},
};
use crate::{
    WorkspaceMutationKind,
    control::{CheckpointFileMode, CheckpointFileVersion},
};

/// Material tool boundary that requires an exact candidate checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolCheckpointBoundary {
    /// One exact target passed mutation-specific preflight but has not changed yet.
    BeforeMutation {
        /// Canonical workspace-relative path.
        path: String,
        /// Exact supported target kind.
        kind: WorkspaceMutationKind,
    },
    /// One owned workspace mutation completed with its exact postimage.
    Mutation {
        /// Canonical workspace-relative path.
        path: String,
        /// Exact supported target kind.
        kind: WorkspaceMutationKind,
        /// Exact state produced by the admitted effect or completed command receipt.
        owned_postchange: CheckpointFileVersion,
    },
    /// A declared verification command completed successfully.
    Verification,
    /// A caller-authorized external effect completed successfully.
    ExternalEffect,
}

pub(super) type ToolCheckpointObserver =
    Arc<dyn Fn(ToolCheckpointBoundary) -> Result<(), String> + Send + Sync>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PreparedMutation {
    path: String,
    kind: WorkspaceMutationKind,
    owned_postchange: CheckpointFileVersion,
}

impl WorkspaceDeveloperTools {
    pub(super) fn prepare_effect_checkpoint(
        &mut self,
        name: &str,
        arguments: &Value,
    ) -> Result<(), DeveloperLoopError> {
        self.prepared_mutations.clear();
        match name {
            "workspace_write" => self.prepare_write_checkpoint(arguments),
            "workspace_patch" => self.prepare_patch_checkpoint(arguments),
            "workspace_remove" => self.prepare_remove_checkpoint(arguments),
            "run_command" | "command_start" => self.prepare_command_checkpoints(arguments),
            _ => Ok(()),
        }
    }

    fn prepare_write_checkpoint(&mut self, arguments: &Value) -> Result<(), DeveloperLoopError> {
        let relative = required_string(arguments, "path")?;
        let content = required_string(arguments, "content")?;
        if content.len() > super::MAX_FILE_BYTES {
            return Err(tool("write exceeds the per-file byte bound"));
        }
        let path = checked(&self.root, relative, true)?;
        let existed_before = path.exists();
        self.grounding.ensure_mutation_allowed(relative, existed_before).map_err(tool)?;
        if path.is_file()
            && fs::read(path).map_err(|error| tool(error.to_string()))? == content.as_bytes()
        {
            return Ok(());
        }
        self.checkpoint_before_mutation(relative, WorkspaceMutationKind::File)?;
        self.prepared_mutations.push(prepared_file(relative, content.as_bytes()));
        Ok(())
    }

    fn prepare_patch_checkpoint(&mut self, arguments: &Value) -> Result<(), DeveloperLoopError> {
        let relative = required_string(arguments, "path")?;
        let old = required_string(arguments, "old")?;
        let new = required_string(arguments, "new")?;
        let replace_all = arguments.get("replace_all").and_then(Value::as_bool).unwrap_or(false);
        if old.is_empty() {
            return Err(tool("patch old text is empty"));
        }
        let path = checked(&self.root, relative, false)?;
        self.grounding.ensure_mutation_allowed(relative, true).map_err(tool)?;
        let content = fs::read_to_string(path).map_err(|error| tool(error.to_string()))?;
        let occurrences = content.matches(old).count();
        if occurrences == 0 || (!replace_all && occurrences != 1) {
            return Err(tool(format!("patch expected one match but found {occurrences}")));
        }
        let replaced =
            if replace_all { content.replace(old, new) } else { content.replacen(old, new, 1) };
        if replaced.len() > super::MAX_FILE_BYTES {
            return Err(tool("patched file exceeds the per-file byte bound"));
        }
        self.checkpoint_before_mutation(relative, WorkspaceMutationKind::File)?;
        self.prepared_mutations.push(prepared_file(relative, replaced.as_bytes()));
        Ok(())
    }

    fn prepare_remove_checkpoint(&mut self, arguments: &Value) -> Result<(), DeveloperLoopError> {
        let relative = required_string(arguments, "path")?;
        if relative.is_empty() || relative == "." {
            return Err(tool("workspace_remove cannot remove the workspace root"));
        }
        let path = checked(&self.root, relative, false)?;
        let metadata = fs::metadata(&path).map_err(|error| tool(error.to_string()))?;
        if metadata.is_dir() {
            self.grounding.ensure_empty_directory_removal_allowed(relative).map_err(tool)?;
            if fs::read_dir(path)
                .map_err(|error| tool(error.to_string()))?
                .next()
                .transpose()
                .map_err(|error| tool(error.to_string()))?
                .is_some()
            {
                return Err(tool("workspace_remove only removes an empty directory"));
            }
            self.checkpoint_before_mutation(relative, WorkspaceMutationKind::EmptyDirectory)?;
            self.prepared_mutations.push(PreparedMutation {
                path: relative.to_owned(),
                kind: WorkspaceMutationKind::EmptyDirectory,
                owned_postchange: CheckpointFileVersion::Absent,
            });
            return Ok(());
        }
        if !metadata.is_file() {
            return Err(tool("workspace_remove requires one regular file or empty directory"));
        }
        self.grounding.ensure_mutation_allowed(relative, true).map_err(tool)?;
        self.ownership.ensure_removable(&path)?;
        self.checkpoint_before_mutation(relative, WorkspaceMutationKind::File)?;
        self.prepared_mutations.push(PreparedMutation {
            path: relative.to_owned(),
            kind: WorkspaceMutationKind::File,
            owned_postchange: CheckpointFileVersion::Absent,
        });
        Ok(())
    }

    fn prepare_command_checkpoints(&self, arguments: &Value) -> Result<(), DeveloperLoopError> {
        if !self.command_has_effect(arguments)? {
            return Ok(());
        }
        let Some(scope) = &self.in_place_scope else { return Ok(()) };
        let paths = scope.paths().map_err(|error| tool(error.to_string()))?;
        for path in &paths {
            let relative = path.to_str().ok_or_else(|| tool("scope path must be UTF-8"))?;
            let target = checked(&self.root, relative, true)?;
            if target.is_dir() {
                return Err(tool("command scope paths must be regular files or absent"));
            }
            self.checkpoint_before_mutation(relative, WorkspaceMutationKind::File)?;
        }
        if scope.paths().map_err(|error| tool(error.to_string()))? != paths {
            return Err(tool("in-place command scope changed during checkpoint capture"));
        }
        Ok(())
    }

    pub(super) fn checkpoint_before_mutation(
        &self,
        path: &str,
        kind: WorkspaceMutationKind,
    ) -> Result<(), DeveloperLoopError> {
        if let Some(observer) = &self.checkpoint_observer {
            observer(ToolCheckpointBoundary::BeforeMutation { path: path.to_owned(), kind })
                .map_err(tool)?;
        }
        Ok(())
    }

    pub(super) fn record_checkpoint(
        &mut self,
        name: &str,
        arguments: &Value,
        result: &Value,
    ) -> Result<(), DeveloperLoopError> {
        let completed_direct_mutation = match name {
            "workspace_write" => result.get("changed").and_then(Value::as_bool) == Some(true),
            "workspace_patch" | "workspace_remove" => result.get("error").is_none(),
            _ => false,
        };
        let completed_command = matches!(name, "run_command" | "command_poll" | "command_recover")
            && result.get("success").and_then(Value::as_bool) == Some(true)
            && (name == "run_command"
                || result.get("state").and_then(Value::as_str) == Some("completed"));
        let mutations = if completed_direct_mutation {
            std::mem::take(&mut self.prepared_mutations)
        } else if completed_command {
            self.command_mutations()?
        } else {
            self.prepared_mutations.clear();
            Vec::new()
        };
        let observer = self.checkpoint_observer.clone();
        for mutation in mutations {
            if let Some(observer) = &observer {
                observer(ToolCheckpointBoundary::Mutation {
                    path: mutation.path,
                    kind: mutation.kind,
                    owned_postchange: mutation.owned_postchange,
                })
                .map_err(tool)?;
            }
        }
        let boundary = match name {
            "run_command" | "command_poll" | "command_recover" if completed_command => {
                match string(arguments, "purpose")
                    .or_else(|| result.get("purpose").and_then(Value::as_str))
                {
                    Some("verification") => Some(ToolCheckpointBoundary::Verification),
                    Some("external_effect") => Some(ToolCheckpointBoundary::ExternalEffect),
                    _ => None,
                }
            }
            _ => None,
        };
        if let (Some(observer), Some(boundary)) = (&self.checkpoint_observer, boundary) {
            observer(boundary).map_err(tool)?;
        }
        Ok(())
    }

    fn command_mutations(&self) -> Result<Vec<PreparedMutation>, DeveloperLoopError> {
        let Some(scope) = &self.in_place_scope else { return Ok(Vec::new()) };
        scope
            .changed_paths(&self.root)
            .map_err(|error| tool(error.to_string()))?
            .into_iter()
            .map(|path| {
                let relative = path.to_str().ok_or_else(|| tool("scope path must be UTF-8"))?;
                exact_file_receipt(&self.root, relative)
            })
            .collect()
    }
}

fn prepared_file(path: &str, content: &[u8]) -> PreparedMutation {
    PreparedMutation {
        path: path.to_owned(),
        kind: WorkspaceMutationKind::File,
        owned_postchange: CheckpointFileVersion::present(
            Sha256Digest::new(Sha256::digest(content).into()),
            content.len() as u64,
            CheckpointFileMode::Regular,
        ),
    }
}

fn exact_file_receipt(
    root: &std::path::Path,
    path: &str,
) -> Result<PreparedMutation, DeveloperLoopError> {
    let target = checked(root, path, true)?;
    let before = match fs::symlink_metadata(&target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PreparedMutation {
                path: path.to_owned(),
                kind: WorkspaceMutationKind::File,
                owned_postchange: CheckpointFileVersion::Absent,
            });
        }
        Err(error) => return Err(tool(error.to_string())),
    };
    if !before.is_file() || before.file_type().is_symlink() {
        return Err(tool("completed command scope is not a regular file or absent"));
    }
    let bytes = fs::read(&target).map_err(|error| tool(error.to_string()))?;
    if bytes.len() > super::MAX_FILE_BYTES {
        return Err(tool("completed command scope exceeds the per-file byte bound"));
    }
    let after = fs::symlink_metadata(&target).map_err(|error| tool(error.to_string()))?;
    if !after.is_file()
        || after.file_type().is_symlink()
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
    {
        return Err(tool("completed command scope changed while recording its receipt"));
    }
    Ok(PreparedMutation {
        path: path.to_owned(),
        kind: WorkspaceMutationKind::File,
        owned_postchange: CheckpointFileVersion::present(
            Sha256Digest::new(Sha256::digest(&bytes).into()),
            bytes.len() as u64,
            file_mode(&after),
        ),
    })
}

fn file_mode(metadata: &fs::Metadata) -> CheckpointFileMode {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o111 == 0 {
            CheckpointFileMode::Regular
        } else {
            CheckpointFileMode::Executable
        }
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        CheckpointFileMode::Regular
    }
}
