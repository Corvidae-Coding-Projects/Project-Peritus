//! Durable automatic before-images and exact owned-postimage checkpoint settlement.

use super::{
    AUTOMATIC_CHECKPOINT_NAME, ActorId, CheckpointFileMode, CheckpointFileVersion, CheckpointPath,
    ControlError, ControlIntent, ControlOperation, ConversationId, ConversationRecord, Error,
    OperationId, Path, ProductRunService, RunId, UserCheckpoint, WorkspaceId,
    WorkspaceMutationKind, automatic_checkpoint_id, check_automatic_record, check_protected,
    checkpoint_references, empty_directory_exclusion, external_effects, observe_empty_directory,
    observe_path, public_query, validate_automatic_checkpoint, validate_run_binding,
};

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
        self.capture_automatic_checkpoint_for_operation(
            actor,
            start.conversation(),
            workspace,
            run,
            path,
            kind,
            None,
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
        let query = public_query(conversation, workspace)?;
        let checkpoint = automatic_checkpoint_id(run, path, kind)?;
        let record = self
            .with_controls(false, |store| store.load(conversation))?
            .ok_or(ControlError::NotFound)?;
        check_automatic_record(&record, actor, workspace)?;
        if let Some(existing) = record.checkpoints().iter().find(|value| value.id() == checkpoint) {
            validate_automatic_checkpoint(existing, run, path, kind)?;
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
                observe_empty_directory(&identity, path)?;
                (Vec::new(), vec![empty_directory_exclusion(path)], Vec::new())
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
            ControlIntent::CreateCheckpoint(value),
        );
        self.with_controls(false, |store| store.accept_checkpoint(&operation, &bodies))
            .map(|receipt| receipt.accepted_revision())
    }

    pub(crate) fn seal_automatic_checkpoint(
        &self,
        start: &ControlOperation,
        run: RunId,
        relative: &Path,
        kind: WorkspaceMutationKind,
        owned_postchange: CheckpointFileVersion,
    ) -> Result<(), Error> {
        validate_run_binding(start, run)?;
        let path = relative.to_str().ok_or(ControlError::InvalidInput)?;
        let conversation = start.conversation();
        let record = self
            .with_controls(false, |store| store.load(conversation))?
            .ok_or(ControlError::NotFound)?;
        let actor = ActorId::new(*start.actor_bytes()).map_err(|_| ControlError::InvalidInput)?;
        let workspace =
            WorkspaceId::new(*start.workspace_bytes()).map_err(|_| ControlError::InvalidInput)?;
        check_automatic_record(&record, actor, workspace)?;
        let checkpoint_id = automatic_checkpoint_id(run, path, kind)?;
        let checkpoint = record
            .checkpoints()
            .iter()
            .find(|value| value.id() == checkpoint_id)
            .ok_or(ControlError::NotFound)?;
        validate_automatic_checkpoint(checkpoint, run, path, kind)?;
        let versions = match kind {
            WorkspaceMutationKind::File => vec![(path.to_owned(), owned_postchange)],
            WorkspaceMutationKind::EmptyDirectory => Vec::new(),
        };
        self.seal_checkpoint_versions(start, run, &record, checkpoint, versions)
    }

    pub(crate) fn seal_latest_checkpoint(
        &self,
        start: &ControlOperation,
        run: RunId,
    ) -> Result<(), Error> {
        validate_run_binding(start, run)?;
        let record = self
            .with_controls(false, |store| store.load(start.conversation()))?
            .ok_or(ControlError::NotFound)?;
        let Some(checkpoint) = record.checkpoints().iter().rev().find(|checkpoint| {
            checkpoint.sealed_by_run().is_none() && checkpoint.automatic_run().is_none()
        }) else {
            return Ok(());
        };
        self.seal_checkpoint(start, run, &record, checkpoint)
    }

    fn seal_checkpoint(
        &self,
        start: &ControlOperation,
        run: RunId,
        record: &ConversationRecord,
        checkpoint: &UserCheckpoint,
    ) -> Result<(), Error> {
        let conversation = start.conversation();
        let workspace =
            WorkspaceId::new(*start.workspace_bytes()).map_err(|_| ControlError::InvalidInput)?;
        let query = public_query(conversation, workspace)?;
        let captured = self.capture_checkpoint_paths(record, query, checkpoint)?;
        self.seal_checkpoint_versions(
            start,
            run,
            record,
            checkpoint,
            captured.into_iter().map(|path| (path.path, path.version)).collect(),
        )
    }

    fn seal_checkpoint_versions(
        &self,
        start: &ControlOperation,
        run: RunId,
        record: &ConversationRecord,
        checkpoint: &UserCheckpoint,
        versions: Vec<(String, CheckpointFileVersion)>,
    ) -> Result<(), Error> {
        let conversation = start.conversation();
        let actor = ActorId::new(*start.actor_bytes()).map_err(|_| ControlError::InvalidInput)?;
        let workspace =
            WorkspaceId::new(*start.workspace_bytes()).map_err(|_| ControlError::InvalidInput)?;
        let operation = ControlOperation::new(
            OperationId::new(seal_operation_id(checkpoint.id().as_bytes(), run, &versions))?,
            conversation,
            actor,
            workspace,
            record.revision(),
            ControlIntent::SealCheckpoint {
                checkpoint: checkpoint.id(),
                run: run.into_bytes(),
                versions,
            },
        );
        self.with_controls(false, |store| store.accept(&operation)).map(|_| ())
    }
}

fn seal_operation_id(
    checkpoint: &[u8; 16],
    run: RunId,
    versions: &[(String, CheckpointFileVersion)],
) -> [u8; 16] {
    let mut bytes = b"peritus-workbench-checkpoint-seal-v2\0".to_vec();
    bytes.extend_from_slice(checkpoint);
    bytes.extend_from_slice(run.as_bytes());
    for (path, version) in versions {
        bytes.extend_from_slice(&(path.len() as u64).to_le_bytes());
        bytes.extend_from_slice(path.as_bytes());
        match version {
            CheckpointFileVersion::Absent => bytes.push(0),
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
