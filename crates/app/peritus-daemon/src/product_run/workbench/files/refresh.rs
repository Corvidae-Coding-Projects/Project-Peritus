//! Coherent request-construction snapshots; preparation retries reuse exact successful sources.

use super::{ProductRunService, WorkbenchFileRequest, WorkbenchQuery};
use crate::product_run::ProductRunServiceError as Error;
use peritus_app_protocol::{
    ProductModelChoice, WorkbenchFileMode, WorkbenchFilePreview, WorkbenchFileRange,
};
use peritus_product_runner::{
    attachment::ValidatedFileText,
    control::{
        ControlIntent, ControlOperation, FileMode, FileObservation, FileRange, FileVersion,
        OperationId,
    },
};
use peritus_types::{ActorId, ArtifactId, ProviderProfileId};
use std::collections::BTreeMap;

/// Pending exact observations survive a failed sibling read; they grant no execution authority.
pub(in crate::product_run) struct RefreshSnapshot {
    generation: u64,
    provider: ProviderProfileId,
    provider_revision: u64,
    model: ProductModelChoice,
    sources: BTreeMap<OperationId, (WorkbenchFilePreview, ValidatedFileText)>,
    complete: bool,
}

impl ProductRunService {
    /// Captures selected source versions together before constructing a provider request.
    /// All changed files publish in one journal append; a failed source publishes none.
    pub(in crate::product_run) fn refresh_request_files(
        &self,
        start: &ControlOperation,
        provider: ProviderProfileId,
        model: &ProductModelChoice,
    ) -> Result<(), Error> {
        let record = self.with_controls(false, |store| store.execution_record(start))?;
        let included =
            record.inputs().capture().map_err(crate::product_control::ControlStoreError::from)?;
        let generation = included.generation();
        let references: Vec<_> = record
            .files()
            .eligible(included.included())
            .into_iter()
            .filter(|entry| entry.mode() == FileMode::RefreshOnRequest)
            .cloned()
            .collect();
        if references.is_empty() {
            return Ok(());
        }
        let actor = ActorId::new(*start.actor_bytes()).map_err(|_| Error::Unavailable)?;
        let query = WorkbenchQuery::new(
            peritus_app_protocol::ConversationId::new(*start.conversation().as_bytes())
                .map_err(|_| Error::Unavailable)?,
            peritus_types::WorkspaceId::new(*start.workspace_bytes())
                .map_err(|_| Error::Unavailable)?,
        );
        self.require_workspace_permissions(
            actor,
            query,
            &[peritus_product_runner::control::PermissionCapability::Read],
        )?;
        let root =
            self.inner.workspaces.get(&query.workspace()).ok_or(Error::WorkspaceUnavailable)?;
        let folder = peritus_workspace::FolderIdentity::observe(root)
            .map_err(|error| Error::invalid_data("verify selected folder", error))?;
        if references.iter().any(|entry| entry.file().source().folder() != Some(folder.digest())) {
            return Err(Error::WorkspaceUnavailable);
        }
        let provider_revision = self.select_provider(provider, model)?.profile().revision();
        let key = *start.conversation().as_bytes();
        let mut snapshot = self
            .inner
            .file_refreshes
            .lock()
            .map_err(|_| Error::Unavailable)?
            .remove(&key)
            .filter(|snapshot| {
                snapshot.generation == generation
                    && snapshot.provider == provider
                    && snapshot.provider_revision == provider_revision
                    && &snapshot.model == model
            })
            .unwrap_or_else(|| RefreshSnapshot {
                generation,
                provider,
                provider_revision,
                model: model.clone(),
                sources: BTreeMap::new(),
                complete: false,
            });
        if snapshot.complete {
            self.inner.file_refreshes.lock().map_err(|_| Error::Unavailable)?.insert(key, snapshot);
            return Ok(());
        }
        let mut failures = Vec::new();
        for entry in &references {
            if snapshot.sources.contains_key(&entry.current().operation()) {
                continue;
            }
            let source = entry.file().source();
            let path = source.path().ok_or(Error::Unavailable)?;
            let range = match source.range() {
                FileRange::All => WorkbenchFileRange::All,
                FileRange::Bytes { start, end } => WorkbenchFileRange::Bytes { start, end },
                FileRange::Lines { first, last } => WorkbenchFileRange::Lines { first, last },
            };
            let selection = WorkbenchFileRequest::new(
                query,
                record.revision(),
                path.to_owned(),
                range,
                WorkbenchFileMode::RefreshOnRequest,
                provider,
                model.clone(),
            )
            .map_err(|error| Error::invalid_data("prepare file refresh", error))?;
            match self.prepare_file(actor, &selection) {
                Ok((preview, text)) if source.folder() == Some(preview.folder()) => {
                    snapshot.sources.insert(entry.current().operation(), (preview, text));
                }
                Ok(_) => failures.push(format!("{path}: selected folder identity changed")),
                Err(error) => failures.push(format!("{path}: {}", error.actionable_message())),
            }
        }
        if !failures.is_empty() {
            self.inner.file_refreshes.lock().map_err(|_| Error::Unavailable)?.insert(key, snapshot);
            return Err(Error::Context {
                code: peritus_app_protocol::AppErrorCode::NotReady,
                retry: peritus_app_protocol::RetryDisposition::AfterRecovery,
                subsystem: peritus_app_protocol::ResponsibleSubsystem::Workspace,
                operation: "refresh selected sources",
                detail: failures.join("; "),
            });
        }
        let mut changes = Vec::new();
        let mut prepared = Vec::new();
        for entry in &references {
            let (preview, text) =
                snapshot.sources.get(&entry.current().operation()).ok_or(Error::Unavailable)?;
            let file = preview.file();
            let observation = FileObservation::new(
                file.source_digest(),
                file.source_bytes(),
                file.range(),
                file.digest(),
            )
            .map_err(crate::product_control::ControlStoreError::from)?;
            if observation == entry.current().observation() {
                continue;
            }
            let consent = preview
                .canonical_bytes()
                .map_err(|error| Error::invalid_data("retain refresh consent", error))?;
            let mut identity = b"peritus-file-refresh-v2\0".to_vec();
            identity.extend_from_slice(start.conversation().as_bytes());
            identity.extend_from_slice(entry.current().operation().as_bytes());
            identity.extend_from_slice(&consent);
            let digest = peritus_codec::sha256(&identity);
            let mut bytes = [0; 16];
            bytes.copy_from_slice(&digest.as_bytes()[..16]);
            bytes[0] |= 1;
            let id =
                OperationId::new(bytes).map_err(crate::product_control::ControlStoreError::from)?;
            let version = FileVersion::new(
                id,
                ArtifactId::new(bytes).map_err(|_| Error::Unavailable)?,
                observation,
                peritus_codec::sha256(&consent),
            )
            .map_err(crate::product_control::ControlStoreError::from)?;
            changes.push((entry.file().operation(), entry.current().operation(), version));
            prepared.push((text, consent));
        }
        if !changes.is_empty() {
            let mut identity = b"peritus-file-refresh-snapshot-v1\0".to_vec();
            identity.extend_from_slice(start.id().as_bytes());
            for (_, _, version) in &changes {
                identity.extend_from_slice(version.operation().as_bytes());
            }
            let digest = peritus_codec::sha256(&identity);
            let mut bytes = [0; 16];
            bytes.copy_from_slice(&digest.as_bytes()[..16]);
            bytes[0] |= 1;
            let operation = ControlOperation::new(
                OperationId::new(bytes).map_err(crate::product_control::ControlStoreError::from)?,
                start.conversation(),
                actor,
                query.workspace(),
                record.revision(),
                ControlIntent::RefreshFiles { versions: changes },
            );
            let sources = prepared
                .iter()
                .map(|(text, consent)| (*text, consent.as_slice()))
                .collect::<Vec<_>>();
            self.with_controls(false, |store| store.accept_files(&operation, &sources))?;
            snapshot.generation = self
                .with_controls(false, |store| store.execution_record(start))?
                .inputs()
                .generation();
        }
        snapshot.sources.clear();
        snapshot.complete = true;
        self.inner.file_refreshes.lock().map_err(|_| Error::Unavailable)?.insert(key, snapshot);
        Ok(())
    }

    pub(in crate::product_run) fn finish_file_refresh(
        &self,
        start: &ControlOperation,
    ) -> Result<(), Error> {
        self.inner
            .file_refreshes
            .lock()
            .map_err(|_| Error::Unavailable)?
            .remove(start.conversation().as_bytes());
        Ok(())
    }
}
