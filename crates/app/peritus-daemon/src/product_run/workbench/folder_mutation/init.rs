//! Exact confirmed initialization, C1 effect, and durable original-receipt recovery.

use super::{
    ActorId, ControlError, ControlStore, Error, ProductRunService, SessionId, WorkbenchCommand,
};
use crate::product_run::workbench::{error_response, receipt_projection};
use peritus_app_protocol::{AppResponsePayload, WorkbenchIntent, WorkbenchReceipt};
use peritus_product_runner::control::{
    ControlIntent, ControlOperation, ConversationId, OperationId,
};
use peritus_types::{Generation, RevisionNumber};

impl ProductRunService {
    pub(crate) fn discover_init(
        &self,
        actor: ActorId,
        request: peritus_app_protocol::InitDiscoveryRequest,
    ) -> AppResponsePayload {
        let result = self
            .require_workspace_permissions(
                actor,
                request.query(),
                &[peritus_product_runner::control::PermissionCapability::Read],
            )
            .and_then(|()| {
                let id = ConversationId::new(request.query().conversation().into_bytes())?;
                let record = self
                    .with_controls(false, |store| store.load(id))?
                    .ok_or(ControlError::NotFound)?;
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
                    .ok_or(ControlError::ScopeMismatch)?;
                crate::product_control::discover_init(root, request)
                    .map_err(|_| Error::from(ControlError::InvalidInput))
            });
        result.map_or_else(error_response, AppResponsePayload::InitProposal)
    }

    /// Revalidates an exact inspected initialization proposal and constructs the inert patch that
    /// the authenticated workspace mutation owner must authorize and apply.
    pub(crate) fn prepare_init_patch(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
        generation: Generation,
        revision: RevisionNumber,
    ) -> Result<peritus_patch::PatchSet, Error> {
        let WorkbenchIntent::ApplyInitDiff(proposal) = command.intent() else {
            return Err(ControlError::InvalidInput.into());
        };
        if proposal.query() != command.query() || proposal.revision() != command.expected_revision()
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
        self.control_workspace(command.query())?;
        let id = ConversationId::new(command.query().conversation().into_bytes())?;
        let record =
            self.with_controls(false, |store| store.load(id))?.ok_or(ControlError::NotFound)?;
        if record.owner_bytes() != actor.as_bytes()
            || record.workspace_bytes() != command.query().workspace().as_bytes()
        {
            return Err(ControlError::ScopeMismatch.into());
        }
        if record.revision() != command.expected_revision() {
            return Err(ControlError::StaleRevision.into());
        }
        let root = self
            .inner
            .workspaces
            .get(&command.query().workspace())
            .ok_or(ControlError::ScopeMismatch)?;
        crate::product_control::prepare_init_patch(
            root,
            command.query().workspace(),
            generation,
            revision,
            proposal,
        )
        .map_err(|_| ControlError::StaleRevision.into())
    }
    pub(crate) async fn apply_workbench_init(
        &self,
        actor: ActorId,
        session: SessionId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        let service = self.clone();
        let command = command.clone();
        match tokio::task::spawn_blocking(move || service.apply_init(actor, session, &command))
            .await
        {
            Ok(result) => result.map_or_else(error_response, AppResponsePayload::WorkbenchReceipt),
            Err(_) => error_response(Error::Corrupt("initialization worker did not settle")),
        }
    }

    fn apply_init(
        &self,
        actor: ActorId,
        session: SessionId,
        command: &WorkbenchCommand,
    ) -> Result<WorkbenchReceipt, Error> {
        let WorkbenchIntent::ApplyInitDiff(proposal) = command.intent() else {
            return Err(ControlError::InvalidInput.into());
        };
        self.control_workspace(command.query())?;
        let fingerprint = proposal.fingerprint().map_err(|_| ControlError::InvalidInput)?;
        if let Some(receipt) = self.with_controls(false, |store| {
            resolve_initialization(store, actor, command, fingerprint.as_bytes())
        })? {
            return Ok(receipt);
        }
        let revision = RevisionNumber::new(command.expected_revision())
            .map_err(|_| ControlError::InvalidInput)?;
        let patch = self.prepare_init_patch(actor, command, Generation::first(), revision)?;
        self.with_controls(false, |store| {
            if let Some(receipt) =
                resolve_initialization(store, actor, command, fingerprint.as_bytes())?
            {
                return Ok(receipt);
            }
            let outcome = self.apply_authorized_folder_patch(
                store,
                actor,
                session,
                command,
                patch,
                command.expected_revision(),
            )?;
            let manifest = outcome.applied_patch().installed_manifest().to_vec();
            let operation = ControlOperation::new(
                OperationId::new(command.operation().into_bytes())?,
                ConversationId::new(command.query().conversation().into_bytes())?,
                actor,
                command.query().workspace(),
                command.expected_revision(),
                ControlIntent::RecordInitialization {
                    proposal_digest: fingerprint.into_bytes(),
                    patch_digest: *outcome.patch_identity().as_bytes(),
                    transaction_manifest_digest: peritus_codec::sha256(&manifest).into_bytes(),
                },
            );
            // A failed publication never re-applies an effect: C1 has already durably consumed
            // this action. Such a failure remains explicit until reconciliation, not success.
            let receipt = store.accept_initialization(&operation, manifest)?;
            receipt_projection(command, &receipt)
        })
    }

    pub(in crate::product_run) fn resolve_workbench_initialization(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> Result<WorkbenchReceipt, Error> {
        let WorkbenchIntent::ApplyInitDiff(proposal) = command.intent() else {
            return Err(ControlError::InvalidInput.into());
        };
        self.control_workspace(command.query())?;
        let fingerprint = proposal.fingerprint().map_err(|_| ControlError::InvalidInput)?;
        self.with_controls(false, |store| {
            resolve_initialization(store, actor, command, fingerprint.as_bytes())?
                .ok_or_else(|| ControlError::NotFound.into())
        })
    }
}

fn resolve_initialization(
    store: &ControlStore,
    actor: ActorId,
    command: &WorkbenchCommand,
    fingerprint: &[u8; 32],
) -> Result<Option<WorkbenchReceipt>, Error> {
    let id = ConversationId::new(command.query().conversation().into_bytes())?;
    let record = store.load(id)?.ok_or(ControlError::NotFound)?;
    if record.owner_bytes() != actor.as_bytes()
        || record.workspace_bytes() != command.query().workspace().as_bytes()
    {
        return Err(ControlError::ScopeMismatch.into());
    }
    let Some(operation) =
        store.operation(id, OperationId::new(command.operation().into_bytes())?)?
    else {
        return Ok(None);
    };
    let ControlIntent::RecordInitialization { proposal_digest, .. } = operation.intent() else {
        return Err(ControlError::IdempotencyConflict.into());
    };
    if proposal_digest != fingerprint
        || operation.expected_revision() != command.expected_revision()
    {
        return Err(ControlError::IdempotencyConflict.into());
    }
    let receipt =
        store.resolve(&operation)?.ok_or(Error::Corrupt("initialization receipt missing"))?;
    receipt_projection(command, &receipt).map(Some)
}
