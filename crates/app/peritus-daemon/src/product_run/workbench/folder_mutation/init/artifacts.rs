//! Authenticated artifact initialization with the existing patch authority and receipt owner.
use super::{
    ActorId, AppResponsePayload, ControlError, ConversationId, Error, Generation,
    ProductRunService, RevisionNumber, WorkbenchCommand, WorkbenchIntent, error_response,
};
use peritus_app_protocol::{InitArtifactDiscovery, InitArtifactPageRequest, InitArtifactProposal};
use std::path::PathBuf;

impl ProductRunService {
    pub(crate) async fn discover_init_artifacts(
        &self,
        actor: ActorId,
        request: InitArtifactDiscovery,
    ) -> AppResponsePayload {
        let service = self.clone();
        match tokio::task::spawn_blocking(move || {
            let scope = service.init_artifact_scope(actor, request.request())?;
            crate::product_control::discover_init_artifacts_checked(
                &scope.root,
                &scope.archive,
                &request,
                &|path| scope.read_allowed(path),
            )
            .map_err(|_| Error::from(ControlError::InvalidInput))
        })
        .await
        {
            Ok(result) => {
                result.map_or_else(error_response, AppResponsePayload::InitArtifactProposal)
            }
            Err(_) => error_response(ControlError::InvalidInput.into()),
        }
    }
    pub(crate) async fn init_artifact_page(
        &self,
        actor: ActorId,
        request: InitArtifactPageRequest,
    ) -> AppResponsePayload {
        let service = self.clone();
        match tokio::task::spawn_blocking(move || {
            let scope = service.init_artifact_scope(actor, request.proposal().request())?;
            crate::product_control::init_artifact_page_checked(&scope.archive, request, &|path| {
                scope.read_allowed(path)
            })
            .map_err(|_| Error::from(ControlError::InvalidInput))
        })
        .await
        {
            Ok(result) => result.map_or_else(error_response, AppResponsePayload::InitArtifactPage),
            Err(_) => error_response(ControlError::InvalidInput.into()),
        }
    }
    pub(super) fn init_artifact_scope(
        &self,
        actor: ActorId,
        request: peritus_app_protocol::InitDiscoveryRequest,
    ) -> Result<InitArtifactScope, Error> {
        self.require_workspace_permissions(
            actor,
            request.query(),
            &[peritus_product_runner::control::PermissionCapability::Read],
        )?;
        let id = ConversationId::new(request.query().conversation().into_bytes())?;
        let record =
            self.with_controls(false, |store| store.load(id))?.ok_or(ControlError::NotFound)?;
        if record.owner_bytes() != actor.as_bytes()
            || record.workspace_bytes() != request.query().workspace().as_bytes()
        {
            return Err(ControlError::ScopeMismatch.into());
        }
        if record.revision() != request.revision() {
            return Err(ControlError::StaleRevision.into());
        }
        let root = self
            .inner
            .workspaces
            .get(&request.query().workspace())
            .ok_or(ControlError::ScopeMismatch)?
            .clone();
        let archive = self
            .inner
            .directory
            .join("workbench-init-artifacts")
            .join(super::super::hex_id(request.query().workspace().as_bytes()))
            .join(super::super::hex_id(request.query().conversation().as_bytes()));
        if let Some(folder) = self.inner.folders.get(&request.query().workspace()) {
            folder.verify().map_err(|_| ControlError::ScopeMismatch)?;
        }
        let protected = self.protected_paths(request.query())?;
        let contract =
            self.with_controls(false, |store| store.user_instruction_context(&record))?;
        Ok(InitArtifactScope { root, archive, contract, protected })
    }
    pub(super) fn prepare_init_artifact(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
        proposal: InitArtifactProposal,
        generation: Generation,
        revision: RevisionNumber,
    ) -> Result<peritus_patch::PatchSet, Error> {
        if proposal.query() != command.query()
            || proposal.request().revision() != command.expected_revision()
        {
            return Err(ControlError::StaleRevision.into());
        }
        self.require_workspace_permissions(
            actor,
            command.query(),
            &[
                peritus_product_runner::control::PermissionCapability::Read,
                peritus_product_runner::control::PermissionCapability::Write,
            ],
        )?;
        let scope = self.init_artifact_scope(actor, proposal.request())?;
        crate::product_control::prepare_init_artifact_patch_checked(
            &scope.root,
            &scope.archive,
            command.query().workspace(),
            generation,
            revision,
            proposal,
            &|path| scope.read_allowed(path),
        )
        .map_err(|_| ControlError::StaleRevision.into())
    }
}

pub(super) fn fingerprint(intent: &WorkbenchIntent) -> Result<peritus_types::Sha256Digest, Error> {
    match intent {
        WorkbenchIntent::ApplyInitDiff(proposal) => {
            proposal.fingerprint().map_err(|_| ControlError::InvalidInput.into())
        }
        WorkbenchIntent::ApplyInitArtifact(proposal) => {
            let mut bytes = b"peritus-init-artifact-approval-v1\0".to_vec();
            bytes.extend_from_slice(proposal.query().conversation().as_bytes());
            bytes.extend_from_slice(proposal.query().workspace().as_bytes());
            bytes.extend_from_slice(&proposal.request().revision().to_be_bytes());
            for content in [proposal.manifest(), proposal.review()] {
                bytes.extend_from_slice(content.digest().as_bytes());
                bytes.extend_from_slice(&content.bytes().to_be_bytes());
            }
            Ok(peritus_codec::sha256(&bytes))
        }
        _ => Err(ControlError::InvalidInput.into()),
    }
}

pub(super) struct InitArtifactScope {
    pub(super) root: PathBuf,
    archive: PathBuf,
    contract: String,
    protected: Vec<PathBuf>,
}
impl InitArtifactScope {
    pub(super) fn read_allowed(&self, path: &str) -> bool {
        peritus_product_runner::checked_protected_file(
            &self.root,
            path,
            &self.contract,
            &self.protected,
        )
        .is_ok()
    }
}
