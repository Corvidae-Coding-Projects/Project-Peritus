//! Candidate-checkpoint classification for accepted developer tool effects.

use std::{fs, sync::Arc};

use peritus_agent::DeveloperLoopError;
use peritus_types::Sha256Digest;
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use super::WorkspaceDeveloperTools;
use crate::developer_tools::{
    executor::literal_patch::inspect_literal_patch,
    path::{checked, tool},
    removal,
    wire::{required_string, string},
};
use crate::{
    WorkspaceMutationKind,
    control::{CheckpointFileMode, CheckpointFileVersion, WorkspaceMutationBaseline},
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
    /// One command-mutated path with its durable pre-command baseline and exact postimage.
    BaselineMutation {
        /// Canonical workspace-relative path.
        path: String,
        /// Exact supported target kind.
        kind: WorkspaceMutationKind,
        /// Durable streamed state retained before command execution.
        baseline: WorkspaceMutationBaseline,
        /// Exact state produced by the completed command receipt.
        owned_postchange: CheckpointFileVersion,
    },
    /// A declared verification command completed successfully.
    Verification,
    /// A caller-authorized external effect completed successfully.
    ExternalEffect,
}

pub(super) type ToolCheckpointObserver =
    Arc<dyn Fn(ToolCheckpointBoundary) -> Result<(), String> + Send + Sync>;

mod postimage;
mod preflight;
use postimage::exact_file_receipt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PreparedMutation {
    path: String,
    kind: WorkspaceMutationKind,
    owned_postchange: CheckpointFileVersion,
    baseline: Option<WorkspaceMutationBaseline>,
}

impl WorkspaceDeveloperTools {
    pub(super) fn recover_checkpoint(
        &mut self,
        name: &str,
        arguments: &Value,
        result: &Value,
    ) -> Result<(), DeveloperLoopError> {
        self.prepared_mutations.clear();
        match name {
            "workspace_write" if result.get("changed").and_then(Value::as_bool) == Some(true) => {
                let path = required_string(arguments, "path")?;
                let content = required_string(arguments, "content")?;
                let actual = exact_file_receipt(&self.root, path)?;
                let content_digest = Sha256Digest::new(Sha256::digest(content.as_bytes()).into());
                if !version_has_content(
                    actual.owned_postchange,
                    content_digest,
                    content.len() as u64,
                ) || !result_matches_version(result, actual.owned_postchange)
                {
                    return Err(tool(
                        "workspace changed after the completed write; checkpoint recovery requires reconciliation",
                    ));
                }
                self.prepared_mutations.push(actual);
            }
            "workspace_patch" => {
                let path = required_string(arguments, "path")?;
                let actual = exact_file_receipt(&self.root, path)?;
                if !result_matches_version(result, actual.owned_postchange) {
                    return Err(tool(
                        "workspace changed after the completed patch; checkpoint recovery requires reconciliation",
                    ));
                }
                self.prepared_mutations.push(actual);
            }
            "workspace_remove" => {
                let path = required_string(arguments, "path")?;
                match fs::symlink_metadata(checked(&self.root, path, true)?) {
                    Ok(_) => {
                        return Err(tool(
                            "removed path reappeared before checkpoint recovery; reconciliation is required",
                        ));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(tool(error.to_string())),
                }
                if result.get("transaction").and_then(Value::as_str).is_some() {
                    let state_root = self
                        .removal_transactions
                        .as_deref()
                        .ok_or_else(|| tool("writable tools have no durable removal state"))?;
                    for (path, kind) in removal::recovery_checkpoints(state_root, result)? {
                        self.prepared_mutations.push(PreparedMutation {
                            path,
                            kind,
                            owned_postchange: CheckpointFileVersion::Absent,
                            baseline: None,
                        });
                    }
                } else {
                    let kind = match result.get("kind").and_then(Value::as_str) {
                        Some("file") => WorkspaceMutationKind::File,
                        Some("directory") => WorkspaceMutationKind::EmptyDirectory,
                        _ => {
                            return Err(tool(
                                "completed removal receipt has no supported path kind",
                            ));
                        }
                    };
                    self.prepared_mutations.push(PreparedMutation {
                        path: path.to_owned(),
                        kind,
                        owned_postchange: CheckpointFileVersion::Absent,
                        baseline: None,
                    });
                }
            }
            _ => {}
        }
        self.record_checkpoint(name, arguments, result)
    }

    pub(super) fn prepare_effect_checkpoint(
        &mut self,
        name: &str,
        arguments: &Value,
    ) -> Result<(), DeveloperLoopError> {
        self.prepare_checkpoint_targets(name, arguments)?;
        let recursive = self.prepared_removal.clone();
        if let Some(observer) = &self.checkpoint_observer {
            for (path, kind) in &self.checkpoint_targets {
                observer(ToolCheckpointBoundary::BeforeMutation {
                    path: path.clone(),
                    kind: *kind,
                })
                .map_err(tool)?;
            }
        }
        if recursive.as_ref().is_some_and(removal::PreparedRemoval::checkpoint_required) {
            let targets = self.checkpoint_targets.clone();
            let mutations = self.prepared_mutations.clone();
            self.prepare_checkpoint_targets(name, arguments)?;
            if self.checkpoint_targets != targets
                || self.prepared_mutations != mutations
                || self.prepared_removal != recursive
            {
                return Err(tool(
                    "recursive removal scope changed during checkpoint preflight; reobserve before mutation",
                ));
            }
        }
        Ok(())
    }

    fn prepare_checkpoint_targets(
        &mut self,
        name: &str,
        arguments: &Value,
    ) -> Result<(), DeveloperLoopError> {
        self.prepared_mutations.clear();
        self.prepared_removal = None;
        self.checkpoint_targets.clear();
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
        let path = checked(&self.root, relative, true)?;
        let existed_before = path.exists();
        self.grounding.ensure_mutation_allowed(relative, existed_before).map_err(tool)?;
        let observed = exact_file_receipt(&self.root, relative)?;
        let mode = observed.owned_postchange.mode().unwrap_or(CheckpointFileMode::Regular);
        let prepared = prepared_file(relative, content.as_bytes(), mode);
        if same_file_content(&observed, &prepared) {
            return Ok(());
        }
        self.checkpoint_before_mutation(relative, WorkspaceMutationKind::File);
        self.prepared_mutations.push(prepared);
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
        let prepared = inspect_literal_patch(&self.root, relative, old, new, replace_all)?;
        self.checkpoint_before_mutation(relative, WorkspaceMutationKind::File);
        self.prepared_mutations.push(PreparedMutation {
            path: relative.to_owned(),
            kind: WorkspaceMutationKind::File,
            owned_postchange: prepared.version(),
            baseline: None,
        });
        Ok(())
    }

    fn prepare_remove_checkpoint(&mut self, arguments: &Value) -> Result<(), DeveloperLoopError> {
        if removal::recursive(arguments)? {
            let transaction = self
                .receipts
                .as_mut()
                .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?
                .pending_effect_identity()?;
            let state_root = self
                .removal_transactions
                .as_deref()
                .ok_or_else(|| tool("writable tools have no recursive removal state"))?;
            let view = self
                .protection_view
                .as_deref()
                .ok_or_else(|| tool("recursive workspace_remove has no live user authority"))?;
            let prepared = removal::prepare_recursive(
                &self.root,
                state_root,
                &transaction,
                &self.grounding,
                &self.access_policy,
                view,
                arguments,
            )?;
            let checkpoints = prepared.checkpoints();
            if let Some(scope) = &self.in_place_scope {
                for (path, _) in &checkpoints {
                    scope
                        .authorize_current_preimage(path)
                        .map_err(|error| tool(error.to_string()))?;
                }
            }
            for (path, kind) in checkpoints {
                if prepared.checkpoint_required() {
                    self.checkpoint_before_mutation(&path, kind);
                }
                self.prepared_mutations.push(PreparedMutation {
                    path,
                    kind,
                    owned_postchange: CheckpointFileVersion::Absent,
                    baseline: None,
                });
            }
            self.prepared_removal = Some(prepared);
            return Ok(());
        }
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
            self.checkpoint_before_mutation(relative, WorkspaceMutationKind::EmptyDirectory);
            self.prepared_mutations.push(PreparedMutation {
                path: relative.to_owned(),
                kind: WorkspaceMutationKind::EmptyDirectory,
                owned_postchange: CheckpointFileVersion::Absent,
                baseline: None,
            });
            return Ok(());
        }
        if !metadata.is_file() {
            return Err(tool("workspace_remove requires one regular file or empty directory"));
        }
        self.grounding.ensure_mutation_allowed(relative, true).map_err(tool)?;
        if removal::explicit_authority(arguments) {
            let transaction = self
                .receipts
                .as_mut()
                .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?
                .pending_effect_identity()?;
            let state_root = self
                .removal_transactions
                .as_deref()
                .ok_or_else(|| tool("writable tools have no authorized removal state"))?;
            let view = self
                .protection_view
                .as_deref()
                .ok_or_else(|| tool("workspace_remove has no live user authority"))?;
            let prepared = removal::prepare_authorized_file(
                &self.root,
                state_root,
                &transaction,
                &self.grounding,
                &self.access_policy,
                view,
                arguments,
            )?;
            let checkpoints = prepared.checkpoints();
            if let Some(scope) = &self.in_place_scope {
                for (path, _) in &checkpoints {
                    scope
                        .authorize_current_preimage(path)
                        .map_err(|error| tool(error.to_string()))?;
                }
            }
            for (path, kind) in checkpoints {
                if prepared.checkpoint_required() {
                    self.checkpoint_before_mutation(&path, kind);
                }
                self.prepared_mutations.push(PreparedMutation {
                    path,
                    kind,
                    owned_postchange: CheckpointFileVersion::Absent,
                    baseline: None,
                });
            }
            self.prepared_removal = Some(prepared);
            return Ok(());
        }
        self.ownership.ensure_removable(&path)?;
        self.checkpoint_before_mutation(relative, WorkspaceMutationKind::File);
        self.prepared_mutations.push(PreparedMutation {
            path: relative.to_owned(),
            kind: WorkspaceMutationKind::File,
            owned_postchange: CheckpointFileVersion::Absent,
            baseline: None,
        });
        Ok(())
    }

    fn prepare_command_checkpoints(&mut self, arguments: &Value) -> Result<(), DeveloperLoopError> {
        if !self.command_has_effect(arguments)? {
            return Ok(());
        }
        let Some(scope) = &self.in_place_scope else { return Ok(()) };
        scope.command_paths().map(|_| ()).map_err(|error| tool(error.to_string()))
    }

    pub(super) fn checkpoint_before_mutation(&mut self, path: &str, kind: WorkspaceMutationKind) {
        self.checkpoint_targets.push((path.to_owned(), kind));
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
            && result.get("state").and_then(Value::as_str) == Some("completed");
        let successful_command =
            completed_command && result.get("success").and_then(Value::as_bool) == Some(true);
        let mutating_command = completed_command
            && string(arguments, "purpose")
                .or_else(|| result.get("purpose").and_then(Value::as_str))
                == Some("external_effect");
        let mutations = if completed_direct_mutation {
            if name == "workspace_remove" {
                std::mem::take(&mut self.prepared_mutations)
            } else {
                self.confirm_direct_file_mutation(arguments, result)?
            }
        } else if mutating_command {
            self.command_mutations()?
        } else {
            self.prepared_mutations.clear();
            Vec::new()
        };
        let observer = self.checkpoint_observer.clone();
        for mutation in mutations {
            if let Some(observer) = &observer {
                let boundary = match mutation.baseline {
                    Some(baseline) => ToolCheckpointBoundary::BaselineMutation {
                        path: mutation.path,
                        kind: mutation.kind,
                        baseline,
                        owned_postchange: mutation.owned_postchange,
                    },
                    None => ToolCheckpointBoundary::Mutation {
                        path: mutation.path,
                        kind: mutation.kind,
                        owned_postchange: mutation.owned_postchange,
                    },
                };
                observer(boundary).map_err(tool)?;
            }
        }
        let boundary = match name {
            "run_command" | "command_poll" | "command_recover" if successful_command => {
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

    fn confirm_direct_file_mutation(
        &mut self,
        arguments: &Value,
        result: &Value,
    ) -> Result<Vec<PreparedMutation>, DeveloperLoopError> {
        let relative = required_string(arguments, "path")?;
        let actual = exact_file_receipt(&self.root, relative)?;
        let expected = std::mem::take(&mut self.prepared_mutations);
        if expected.as_slice() != std::slice::from_ref(&actual)
            || !result_matches_version(result, actual.owned_postchange)
        {
            return Err(tool(
                "workspace target changed after mutation; checkpoint recovery requires reconciliation",
            ));
        }
        Ok(vec![actual])
    }

    pub(super) fn file_content_matches(
        &self,
        path: &str,
        content: &[u8],
    ) -> Result<bool, DeveloperLoopError> {
        let actual = exact_file_receipt(&self.root, path)?;
        let digest = Sha256Digest::new(Sha256::digest(content).into());
        Ok(version_has_content(actual.owned_postchange, digest, content.len() as u64))
    }

    fn command_mutations(&self) -> Result<Vec<PreparedMutation>, DeveloperLoopError> {
        let Some(scope) = &self.in_place_scope else { return Ok(Vec::new()) };
        let paths = scope
            .changed_paths(&self.root)
            .map_err(|error| tool(error.to_string()))?;
        let mut mutations = Vec::with_capacity(paths.len());
        for path in paths {
            let relative = path.to_str().ok_or_else(|| tool("scope path must be UTF-8"))?;
            let (kind, baseline) =
                scope.mutation_baseline(relative).map_err(|error| tool(error.to_string()))?;
            let mut mutation = exact_file_receipt(&self.root, relative)?;
            mutation.kind = kind;
            mutation.baseline = Some(baseline);
            mutations.push(mutation);
        }
        Ok(mutations)
    }
}

fn prepared_file(path: &str, content: &[u8], mode: CheckpointFileMode) -> PreparedMutation {
    PreparedMutation {
        path: path.to_owned(),
        kind: WorkspaceMutationKind::File,
        owned_postchange: CheckpointFileVersion::present(
            Sha256Digest::new(Sha256::digest(content).into()),
            content.len() as u64,
            mode,
        ),
        baseline: None,
    }
}

fn same_file_content(left: &PreparedMutation, right: &PreparedMutation) -> bool {
    left.owned_postchange.digest() == right.owned_postchange.digest()
        && left.owned_postchange.bytes() == right.owned_postchange.bytes()
}

fn version_has_content(version: CheckpointFileVersion, digest: Sha256Digest, bytes: u64) -> bool {
    version.digest() == Some(digest) && version.bytes() == Some(bytes)
}

fn result_matches_version(result: &Value, version: CheckpointFileVersion) -> bool {
    let (Some(digest), Some(bytes)) = (version.digest(), version.bytes()) else {
        return false;
    };
    result.get("bytes").and_then(Value::as_u64) == Some(bytes)
        && result.get("sha256").and_then(Value::as_str) == Some(digest_hex(digest).as_str())
}

fn digest_hex(digest: Sha256Digest) -> String {
    use core::fmt::Write as _;

    let mut value = String::with_capacity(64);
    for byte in digest.as_bytes() {
        let _ = write!(value, "{byte:02x}");
    }
    value
}
