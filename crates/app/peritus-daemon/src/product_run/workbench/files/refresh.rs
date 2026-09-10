//! Request-boundary reobservation for explicit refresh-mode references.

use super::{ProductRunService, WorkbenchFileRequest, WorkbenchQuery};
use crate::product_run::ProductRunServiceError;
use peritus_app_protocol::{ProductModelChoice, WorkbenchFileMode, WorkbenchFileRange};
use peritus_product_runner::control::{
    ControlIntent, ControlOperation, FileMode, FileObservation, FileRange, FileVersion, OperationId,
};
use peritus_types::{ActorId, ArtifactId};

impl ProductRunService {
    /// Revalidates every selected refresh source before admission. Changed bytes append an
    /// immutable version and force request reconstruction; unchanged bytes reuse the version
    /// but receive a new invocation/source receipt when this request is sealed.
    pub(in crate::product_run) fn refresh_request_files(
        &self,
        start: &ControlOperation,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<bool, ProductRunServiceError> {
        let record = self.with_controls(false, |store| store.execution_record(start))?;
        let included =
            record.inputs().capture().map_err(crate::product_control::ControlStoreError::from)?;
        let references: Vec<_> = record
            .files()
            .eligible(included.included())
            .into_iter()
            .filter(|entry| entry.mode() == FileMode::RefreshOnRequest)
            .cloned()
            .collect();
        let actor =
            ActorId::new(*start.actor_bytes()).map_err(|_| ProductRunServiceError::Unavailable)?;
        let query = WorkbenchQuery::new(
            peritus_app_protocol::ConversationId::new(*start.conversation().as_bytes())
                .map_err(|_| ProductRunServiceError::Unavailable)?,
            peritus_types::WorkspaceId::new(*start.workspace_bytes())
                .map_err(|_| ProductRunServiceError::Unavailable)?,
        );
        let mut revision = record.revision();
        let mut changed = false;
        for entry in references {
            let source = entry.file().source();
            let range = match source.range() {
                FileRange::All => WorkbenchFileRange::All,
                FileRange::Bytes { start, end } => WorkbenchFileRange::Bytes { start, end },
                FileRange::Lines { first, last } => WorkbenchFileRange::Lines { first, last },
            };
            let selection = WorkbenchFileRequest::new(
                query,
                revision,
                source.path().ok_or(ProductRunServiceError::Unavailable)?.to_owned(),
                range,
                WorkbenchFileMode::RefreshOnRequest,
                request.profile_id(),
                ProductModelChoice::new(request.model().as_str().to_owned(), true)
                    .map_err(|_| ProductRunServiceError::Unavailable)?,
            )
            .map_err(|_| ProductRunServiceError::Unavailable)?;
            let (preview, text) = self
                .prepare_file(actor, &selection)
                .map_err(|_| ProductRunServiceError::Unavailable)?;
            if source.folder() != Some(preview.folder()) {
                return Err(ProductRunServiceError::Unavailable);
            }
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
            let consent =
                preview.canonical_bytes().map_err(|_| ProductRunServiceError::Unavailable)?;
            let mut identity = b"peritus-file-refresh-v1\0".to_vec();
            identity.extend_from_slice(start.conversation().as_bytes());
            identity.extend_from_slice(entry.current().operation().as_bytes());
            identity.extend_from_slice(request.request_id().expose_for_wire().as_bytes());
            identity.extend_from_slice(&consent);
            let digest = peritus_codec::sha256(&identity);
            let mut bytes = [0; 16];
            bytes.copy_from_slice(&digest.as_bytes()[..16]);
            bytes[0] |= 1;
            let id =
                OperationId::new(bytes).map_err(crate::product_control::ControlStoreError::from)?;
            let version = FileVersion::new(
                id,
                ArtifactId::new(bytes).map_err(|_| ProductRunServiceError::Unavailable)?,
                observation,
                peritus_codec::sha256(&consent),
            )
            .map_err(crate::product_control::ControlStoreError::from)?;
            let operation = ControlOperation::new(
                id,
                start.conversation(),
                actor,
                query.workspace(),
                revision,
                ControlIntent::RefreshFile {
                    attachment: entry.file().operation(),
                    previous: entry.current().operation(),
                    version,
                },
            );
            let receipt =
                self.with_controls(false, |store| store.accept_file(&operation, &text, consent))?;
            revision = receipt.accepted_revision();
            changed = true;
        }
        Ok(changed)
    }
}
