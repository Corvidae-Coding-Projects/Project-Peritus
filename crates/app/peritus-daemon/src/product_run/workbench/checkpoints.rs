//! Bounded workspace checkpoints and exact-preimage rewind transactions.

use super::{ControlStore, Error, ProductRunService, error_response};
use peritus_app_protocol::{
    AppErrorCode, AppProtocolError, AppResponsePayload, ControlOperationId,
    WorkbenchCheckpointFileMode, WorkbenchCheckpointName, WorkbenchCheckpointPath,
    WorkbenchCheckpointReceipt, WorkbenchCheckpointReferences, WorkbenchCheckpointVersion,
    WorkbenchCommand, WorkbenchIntent, WorkbenchRestoreReceipt, WorkbenchRestoreStatus,
    WorkbenchRewindDisposition, WorkbenchRewindPath, WorkbenchRewindPreview,
    WorkbenchRewindRequest,
};
use peritus_patch::{
    FileMode, FinalFile, LineEndingPolicy, PatchOperation, PatchSet, Preimage, WorkspacePath,
};
use peritus_product_runner::control::{
    CheckpointFileMode, CheckpointFileVersion, CheckpointId, CheckpointPath, CheckpointRange,
    CheckpointReferences, ControlError, ControlIntent, ControlOperation, ConversationId,
    ConversationRecord, OperationId, RestoreId, RestoreOperation, RestoreStatus, UserCheckpoint,
};
use peritus_types::{ActorId, Generation, RevisionNumber, Sha256Digest};
use peritus_workspace::{FolderIdentity, FolderInspection};
use std::{fs, io, path::Path};

mod capture;
mod logical;
mod lookup;
mod projection;
mod ranges;
mod recovery;
mod rewind;
use capture::{check_protected, observe_version};
use projection::{
    app_error, checkpoint_references, derived_id, digest_id, external_effects, noop_manifest,
    patch_input, patch_mode, patch_preimage, public_checkpoint, public_restore, public_version,
};
#[cfg(test)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "crate-level tests inject exact crash boundaries"
)]
pub(crate) use rewind::{RewindFaultPoint, inject_rewind_fault, obstruct_folder_patch};

const HISTORY_EFFECT: &str =
    "Conversation history and cumulative goal and accounting state are preserved.";
const EXTERNAL_EFFECT: &str =
    "Credentials, approvals, process state, and external side effects are excluded.";

struct CapturedPath {
    path: String,
    version: CheckpointFileVersion,
    body: Option<tempfile::TempPath>,
    ranges: Vec<CheckpointRange>,
}

impl CapturedPath {
    fn manifest(&self) -> Result<CheckpointPath, ControlError> {
        if self.ranges.is_empty() {
            CheckpointPath::new(self.path.clone(), self.version)
        } else {
            CheckpointPath::selected_ranges(self.path.clone(), self.version, self.ranges.clone())
        }
    }
}

struct CapturedCoverage {
    paths: Vec<CapturedPath>,
    exclusions: Vec<String>,
}

impl ProductRunService {
    pub(super) fn validate_fork_coverage(
        &self,
        source: &ConversationRecord,
        request: &peritus_app_protocol::WorkbenchForkRequest,
        checkpoint: &UserCheckpoint,
    ) -> Result<(), Error> {
        if request.mode() == peritus_app_protocol::WorkbenchForkMode::ReadOnlyCurrentWorkspace {
            return Ok(());
        }
        if checkpoint.paths().is_empty() {
            return Err(ControlError::InvalidInput.into());
        }
        let observed = self.capture_checkpoint_paths(source, request.child(), checkpoint)?;
        for (index, (current, expected)) in observed.iter().zip(checkpoint.paths()).enumerate() {
            if matches!(
                expected.coverage(),
                peritus_product_runner::control::CheckpointCoverage::SelectedRanges(_)
            ) {
                self.with_controls(false, |store| {
                    rewind::selected_coverage_matches(
                        store,
                        checkpoint.id(),
                        index,
                        expected,
                        current,
                    )
                })?;
            } else if current.version != expected.checkpoint() {
                return Err(Error::StalePreimage);
            }
        }
        Ok(())
    }

    pub(crate) fn inspect_workbench_checkpoint(
        &self,
        actor: ActorId,
        request: WorkbenchRewindRequest,
    ) -> AppResponsePayload {
        let result = self.control_workspace(request.query()).and_then(|()| {
            let conversation = ConversationId::new(request.query().conversation().into_bytes())?;
            self.with_controls(false, |store| {
                let record = store.load(conversation)?.ok_or(ControlError::NotFound)?;
                check_record(&record, actor, request.query(), Some(request.revision()))?;
                let checkpoint = store
                    .load_checkpoint(
                        conversation,
                        CheckpointId::new(request.checkpoint().into_bytes())?,
                    )?
                    .ok_or(ControlError::NotFound)?;
                let operation = store
                    .operation(conversation, OperationId::new(request.checkpoint().into_bytes())?)?
                    .ok_or(ControlError::NotFound)?;
                let receipt = store.resolve(&operation)?.ok_or(ControlError::NotFound)?;
                public_checkpoint(request.query(), receipt.accepted_revision(), &checkpoint)
            })
        });
        result.map_or_else(error_response, AppResponsePayload::WorkbenchCheckpoint)
    }

    pub(crate) async fn create_workbench_checkpoint(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
        checkpoint_coverage: bool,
    ) -> AppResponsePayload {
        let service = self.clone();
        let command = command.clone();
        match tokio::task::spawn_blocking(move || {
            service.create_checkpoint(actor, &command, checkpoint_coverage)
        })
        .await
        {
            Ok(result) => {
                result.map_or_else(error_response, AppResponsePayload::WorkbenchCheckpoint)
            }
            Err(_) => AppResponsePayload::Error(app_error(AppErrorCode::Internal)),
        }
    }

    fn create_checkpoint(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
        checkpoint_coverage: bool,
    ) -> Result<WorkbenchCheckpointReceipt, Error> {
        let WorkbenchIntent::CreateCheckpoint(name) = command.intent() else {
            return Err(ControlError::InvalidInput.into());
        };
        self.control_workspace(command.query())?;
        let conversation = ConversationId::new(command.query().conversation().into_bytes())?;
        let checkpoint = CheckpointId::new(command.operation().into_bytes())?;
        let record = self
            .with_controls(false, |store| store.load(conversation))?
            .ok_or(ControlError::NotFound)?;
        check_record(&record, actor, command.query(), None)?;

        if let Some(existing) = record.checkpoints().iter().find(|value| value.id() == checkpoint) {
            require_checkpoint_schema(existing.paths(), checkpoint_coverage)?;
            if existing.name() != name.as_str() {
                return Err(ControlError::IdempotencyConflict.into());
            }
            return self.resolve_workbench_checkpoint(actor, command);
        }
        check_record(&record, actor, command.query(), Some(command.expected_revision()))?;

        let coverage = self.capture_selected_coverage(&record, command.query())?;
        let references = checkpoint_references(&record);
        let paths =
            coverage.paths.iter().map(CapturedPath::manifest).collect::<Result<Vec<_>, _>>()?;
        // Check the actual captured manifest before durable publication, not a racy
        // separate preview or an already-committed response that the peer cannot decode.
        require_checkpoint_schema(&paths, checkpoint_coverage)?;
        let checkpoint_value = UserCheckpoint::new(
            checkpoint,
            name.as_str().to_owned(),
            references,
            paths,
            coverage.exclusions,
            external_effects(),
        )?;
        let operation = ControlOperation::new(
            OperationId::new(command.operation().into_bytes())?,
            conversation,
            actor,
            command.query().workspace(),
            command.expected_revision(),
            ControlIntent::CreateCheckpoint(checkpoint_value.clone()),
        );
        let bodies = coverage.paths.into_iter().map(|path| path.body).collect::<Vec<_>>();
        let receipt = self
            .with_controls(false, |store| store.accept_checkpoint_snapshots(&operation, &bodies))?;
        public_checkpoint(command.query(), receipt.accepted_revision(), &checkpoint_value)
    }

    pub(crate) fn resolve_workbench_checkpoint(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> Result<WorkbenchCheckpointReceipt, Error> {
        let WorkbenchIntent::CreateCheckpoint(name) = command.intent() else {
            return Err(ControlError::InvalidInput.into());
        };
        self.control_workspace(command.query())?;
        let conversation = ConversationId::new(command.query().conversation().into_bytes())?;
        self.with_controls(false, |store| {
            let record = store.load(conversation)?.ok_or(ControlError::NotFound)?;
            check_record(&record, actor, command.query(), None)?;
            let checkpoint_id = CheckpointId::new(command.operation().into_bytes())?;
            let checkpoint = record
                .checkpoints()
                .iter()
                .find(|checkpoint| checkpoint.id() == checkpoint_id)
                .filter(|checkpoint| checkpoint.name() == name.as_str())
                .cloned()
                .ok_or(ControlError::NotFound)?;
            let operation = ControlOperation::new(
                OperationId::new(command.operation().into_bytes())?,
                conversation,
                actor,
                command.query().workspace(),
                command.expected_revision(),
                ControlIntent::CreateCheckpoint(checkpoint.capture_manifest()),
            );
            let receipt = store.resolve(&operation)?.ok_or(ControlError::NotFound)?;
            public_checkpoint(
                command.query(),
                receipt.accepted_revision(),
                &checkpoint.capture_manifest(),
            )
        })
    }
}

fn require_checkpoint_schema(paths: &[CheckpointPath], supported: bool) -> Result<(), Error> {
    use peritus_product_runner::control::CheckpointCoverage;
    if !supported
        && paths.iter().any(|path| {
            matches!(
                path.coverage(),
                CheckpointCoverage::SelectedRanges(_) | CheckpointCoverage::EmptyDirectory
            )
        })
    {
        return Err(ControlError::UnsupportedSchema.into());
    }
    Ok(())
}

fn check_record(
    record: &ConversationRecord,
    actor: ActorId,
    query: peritus_app_protocol::WorkbenchQuery,
    revision: Option<u64>,
) -> Result<(), Error> {
    if record.owner_bytes() != actor.as_bytes()
        || record.workspace_bytes() != query.workspace().as_bytes()
    {
        return Err(ControlError::ScopeMismatch.into());
    }
    if revision.is_some_and(|expected| record.revision() != expected) {
        return Err(ControlError::StaleRevision.into());
    }
    Ok(())
}
