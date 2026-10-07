//! Durable automatic before-images and exact owned-postimage checkpoint settlement.

use super::{
    AUTOMATIC_CHECKPOINT_NAME, ActorId, CheckpointFileMode, CheckpointFileVersion, CheckpointPath,
    ControlError, ControlIntent, ControlOperation, ConversationId, ConversationRecord, Error,
    OperationId, Path, ProductRunService, RunId, UserCheckpoint, WorkspaceId,
    WorkspaceMutationKind, automatic_checkpoint_id, check_automatic_record, check_protected,
    checkpoint_references, external_effects, observe_empty_directory, observe_path, public_query,
    validate_automatic_checkpoint, validate_run_binding,
};
use crate::product_run::ProductRunServiceError;
use std::sync::{Arc, atomic::AtomicBool};

#[cfg(test)]
mod tests;

impl ProductRunService {
    pub(crate) fn capture_automatic_checkpoint(
        &self,
        start: &ControlOperation,
        run: RunId,
        relative: &Path,
        kind: WorkspaceMutationKind,
    ) -> Result<(), Error> {
        let path = relative.to_str().ok_or(ControlError::InvalidInput)?;
        validate_run_binding(start, run)?;
        let actor = ActorId::new(*start.actor_bytes()).map_err(|_| ControlError::InvalidInput)?;
        let workspace =
            WorkspaceId::new(*start.workspace_bytes()).map_err(|_| ControlError::InvalidInput)?;
        let cancellation = self
            .inner
            .records
            .read()
            .map_err(|_| Error::Corrupt("product run registry lock poisoned"))?
            .get(&run)
            .ok_or(ControlError::NotFound)?
            .control_cancellation
            .clone();
        self.capture_automatic_checkpoint_for_operation_cancellable(
            actor,
            start.conversation(),
            workspace,
            run,
            path,
            kind,
            None,
            &cancellation,
        )
        .map(|_| ())
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the authenticated checkpoint binding and optional first-admission fence stay explicit"
    )]
    pub(crate) fn capture_automatic_checkpoint_for_operation(
        &self,
        actor: ActorId,
        conversation: ConversationId,
        workspace: WorkspaceId,
        run: RunId,
        path: &str,
        kind: WorkspaceMutationKind,
        expected_revision: Option<u64>,
    ) -> Result<u64, Error> {
        self.capture_automatic_checkpoint_for_operation_cancellable(
            actor,
            conversation,
            workspace,
            run,
            path,
            kind,
            expected_revision,
            &self.inner.control_shutdown,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the authenticated checkpoint binding, cancellation and optional first-admission fence stay explicit"
    )]
    fn capture_automatic_checkpoint_for_operation_cancellable(
        &self,
        actor: ActorId,
        conversation: ConversationId,
        workspace: WorkspaceId,
        run: RunId,
        path: &str,
        kind: WorkspaceMutationKind,
        expected_revision: Option<u64>,
        cancellation: &peritus_journal::JournalCancellation,
    ) -> Result<u64, Error> {
        let query = public_query(conversation, workspace)?;
        let checkpoint = automatic_checkpoint_id(run, path, kind)?;
        let (record, existing) = self.with_control_conversation_cancellable(
            conversation,
            cancellation,
            |store| {
                let record = store.load(conversation)?.ok_or(ControlError::NotFound)?;
                let existing = store.load_checkpoint(conversation, checkpoint)?;
                Ok((record, existing))
            },
        )?;
        check_automatic_record(&record, actor, workspace)?;
        if let Some(existing) = existing {
            validate_automatic_checkpoint(&existing, run, path, kind)?;
            return Ok(record.revision());
        }
        if expected_revision.is_some_and(|expected| expected != record.revision()) {
            return Err(ControlError::StaleRevision.into());
        }
        let root = self.workspace_root(query)?;
        let identity = self.checked_folder_identity(query, root)?;
        let protected = self.protected_paths(query)?;
        let contract = record.inputs().capture()?.conversation().to_owned();
        check_protected(root, path, &contract, &protected)?;
        let (paths, exclusions, bodies) = match kind {
            WorkspaceMutationKind::File => {
                let captured = observe_path(&identity, path)?;
                let checkpoint_path = CheckpointPath::new(path.to_owned(), captured.version)?;
                (vec![checkpoint_path], Vec::new(), vec![captured.body])
            }
            WorkspaceMutationKind::EmptyDirectory => {
                let version = observe_empty_directory(&identity, path)?;
                (vec![CheckpointPath::new(path.to_owned(), version)?], Vec::new(), vec![None])
            }
        };
        let value = UserCheckpoint::automatic(
            checkpoint,
            AUTOMATIC_CHECKPOINT_NAME.to_owned(),
            checkpoint_references(&record),
            paths,
            exclusions,
            external_effects(),
            run.into_bytes(),
        )?;
        let operation = ControlOperation::new(
            OperationId::new(*checkpoint.as_bytes())?,
            conversation,
            actor,
            workspace,
            record.revision(),
            ControlIntent::CreateAutomaticCheckpoint(value),
        );
        let prepared = self
            .inner
            .control_generation
            .prepare_checkpoint_snapshots(&operation, &bodies)?;
        self.with_control_conversation_cancellable(conversation, cancellation, |store| {
            store.accept_prepared_checkpoint_snapshots(prepared)
        })
        .map(|receipt| receipt.accepted_revision())
    }

    pub(crate) fn seal_automatic_checkpoint(
        &self,
        start: &ControlOperation,
        run: RunId,
        attempt: &Arc<AtomicBool>,
        relative: &Path,
        kind: WorkspaceMutationKind,
        owned_postchange: CheckpointFileVersion,
    ) -> Result<(), ProductRunServiceError> {
        validate_run_binding(start, run)?;
        let reconciliation = self.retained_control_reconciliation(run, attempt)?;
        let path = relative
            .to_str()
            .ok_or(ProductRunServiceError::Control(ControlError::InvalidInput))?;
        let conversation = start.conversation();
        let actor = ActorId::new(*start.actor_bytes())
            .map_err(|_| ProductRunServiceError::Control(ControlError::InvalidInput))?;
        let workspace = WorkspaceId::new(*start.workspace_bytes())
            .map_err(|_| ProductRunServiceError::Control(ControlError::InvalidInput))?;
        let checkpoint_id = automatic_checkpoint_id(run, path, kind)?;
        let (record, checkpoint) = self.with_control_reconciliation(&reconciliation, |store| {
            let record = store.load(conversation)?.ok_or(ControlError::NotFound)?;
            let checkpoint = store
                .load_checkpoint(conversation, checkpoint_id)?
                .ok_or(ControlError::NotFound)?;
            Ok((record, checkpoint))
        })?;
        check_automatic_record(&record, actor, workspace)?;
        validate_automatic_checkpoint(&checkpoint, run, path, kind)?;
        let versions = if checkpoint.paths().is_empty() {
            Vec::new()
        } else {
            vec![(path.to_owned(), owned_postchange)]
        };
        self.seal_checkpoint_versions(start, run, &reconciliation, &checkpoint, versions)
    }

    pub(crate) fn seal_latest_checkpoint(
        &self,
        start: &ControlOperation,
        run: RunId,
        attempt: &Arc<AtomicBool>,
    ) -> Result<(), ProductRunServiceError> {
        validate_run_binding(start, run)?;
        let reconciliation = self.retained_control_reconciliation(run, attempt)?;
        let record = self
            .with_control_reconciliation(&reconciliation, |store| {
                store.load(start.conversation())
            })?
            .ok_or(ProductRunServiceError::Control(ControlError::NotFound))?;
        let Some(checkpoint) = record.checkpoints().iter().rev().find(|checkpoint| {
            checkpoint.sealed_by_run().is_none()
                && checkpoint.automatic_run().is_none()
                && checkpoint.paths().iter().all(|path| path.owned_postchange().is_none())
        }) else {
            return Ok(());
        };
        self.seal_checkpoint(start, run, &reconciliation, &record, checkpoint)
    }

    fn seal_checkpoint(
        &self,
        start: &ControlOperation,
        run: RunId,
        reconciliation: &crate::product_control::ControlReconciliation,
        record: &ConversationRecord,
        checkpoint: &UserCheckpoint,
    ) -> Result<(), ProductRunServiceError> {
        let conversation = start.conversation();
        let workspace = WorkspaceId::new(*start.workspace_bytes())
            .map_err(|_| ProductRunServiceError::Control(ControlError::InvalidInput))?;
        let query = public_query(conversation, workspace)?;
        let captured = self.observe_checkpoint_paths(record, query, checkpoint)?;
        self.seal_checkpoint_versions(
            start,
            run,
            reconciliation,
            checkpoint,
            captured.into_iter().map(|path| (path.path, path.version)).collect(),
        )
    }

    fn seal_checkpoint_versions(
        &self,
        start: &ControlOperation,
        run: RunId,
        reconciliation: &crate::product_control::ControlReconciliation,
        checkpoint: &UserCheckpoint,
        versions: Vec<(String, CheckpointFileVersion)>,
    ) -> Result<(), ProductRunServiceError> {
        let conversation = start.conversation();
        let actor = ActorId::new(*start.actor_bytes())
            .map_err(|_| ProductRunServiceError::Control(ControlError::InvalidInput))?;
        let workspace = WorkspaceId::new(*start.workspace_bytes())
            .map_err(|_| ProductRunServiceError::Control(ControlError::InvalidInput))?;
        self.with_control_reconciliation(reconciliation, |store| {
            let current = store.load(conversation)?.ok_or(ControlError::NotFound)?;
            let retained =
                current.checkpoints().iter().find(|value| value.id() == checkpoint.id()).cloned();
            let checkpoint = match retained.as_ref() {
                Some(value) => value.clone(),
                None => store
                    .load_checkpoint(conversation, checkpoint.id())?
                    .ok_or(ControlError::NotFound)?,
            };
            if checkpoint.sealed_by_run() == Some(run.into_bytes())
                && checkpoint.paths().len() == versions.len()
                && checkpoint.paths().iter().zip(&versions).all(|(path, (name, version))| {
                    path.path() == name && path.owned_postchange() == Some(*version)
                })
            {
                return Ok(());
            }
            let intent = if retained.is_some() {
                ControlIntent::SealCheckpoint {
                    checkpoint: checkpoint.id(),
                    run: run.into_bytes(),
                    versions: versions.clone(),
                }
            } else {
                ControlIntent::SealAutomaticCheckpoint {
                    checkpoint: checkpoint.id(),
                    run: run.into_bytes(),
                    versions: versions.clone(),
                }
            };
            let operation = ControlOperation::new(
                OperationId::new(seal_operation_id(
                    checkpoint.id().as_bytes(),
                    run,
                    current.revision(),
                    &versions,
                ))?,
                conversation,
                actor,
                workspace,
                current.revision(),
                intent,
            );
            store.accept(&operation).map(|_| ())
        })
    }
}

fn seal_operation_id(
    checkpoint: &[u8; 16],
    run: RunId,
    revision: u64,
    versions: &[(String, CheckpointFileVersion)],
) -> [u8; 16] {
    let mut bytes = b"peritus-workbench-checkpoint-seal-v3\0".to_vec();
    bytes.extend_from_slice(checkpoint);
    bytes.extend_from_slice(run.as_bytes());
    bytes.extend_from_slice(&revision.to_le_bytes());
    for (path, version) in versions {
        bytes.extend_from_slice(&(path.len() as u64).to_le_bytes());
        bytes.extend_from_slice(path.as_bytes());
        match version {
            CheckpointFileVersion::Absent => bytes.push(0),
            CheckpointFileVersion::EmptyDirectory { permissions } => {
                bytes.push(2);
                bytes.extend_from_slice(&permissions.to_le_bytes());
            }
            CheckpointFileVersion::Present { digest, bytes: size, mode } => {
                bytes.push(1);
                bytes.extend_from_slice(digest);
                bytes.extend_from_slice(&size.to_le_bytes());
                bytes.push(match mode {
                    CheckpointFileMode::Regular => 1,
                    CheckpointFileMode::Executable => 2,
                });
            }
        }
    }
    super::super::digest_id(&bytes)
}
