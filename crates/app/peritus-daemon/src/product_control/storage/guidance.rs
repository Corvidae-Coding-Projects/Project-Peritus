//! Atomic guidance sidecar publication and authenticated rebuildable inspection.

use super::ControlStore;
use crate::product_control::{
    ControlStoreError as Error,
    guidance::{
        GUIDANCE_CONTROL_NAMESPACE, GUIDANCE_NAMESPACE, GuidanceMutation, GuidanceObservation,
        catalog_key, guidance_key, plan_guidance, replay_catalog, replay_guidance,
    },
};
use peritus_app_protocol::{
    ControlOperationId, MAX_WORKBENCH_GUIDANCE_PAGE, WorkbenchGuidanceRender,
    WorkbenchGuidanceScope, WorkbenchMemory, WorkbenchMemoryQuery, WorkbenchMemoryRow,
    render_guidance_for_request,
};
use peritus_product_runner::control::{
    ControlError, ControlIntent, ControlOperation, ControlReceipt, ConversationId,
};
use peritus_types::{ActorId, WorkspaceId};

impl ControlStore {
    pub(crate) fn accept_guidance(
        &mut self,
        operation: &ControlOperation,
        mutation: GuidanceMutation,
    ) -> Result<ControlReceipt, Error> {
        if !matches!(operation.intent(), ControlIntent::UpdateGuidance(_)) {
            return Err(ControlError::InvalidInput.into());
        }
        if let Some(receipt) = self.resolve(operation)? {
            return Ok(receipt);
        }
        let workspace = WorkspaceId::new(*operation.workspace_bytes())
            .map_err(|_| ControlError::InvalidInput)?;
        let save_operation = ControlOperationId::new(*operation.id().as_bytes())
            .map_err(|_| ControlError::InvalidInput)?;
        let identity = mutation.identity(save_operation);
        let catalog =
            self.journal.state_record(GUIDANCE_CONTROL_NAMESPACE, &catalog_key(workspace))?;
        let key = guidance_key(workspace, identity);
        let guidance = self.journal.state_record(GUIDANCE_NAMESPACE, &key)?;
        let tombstone = self.journal.state_record(GUIDANCE_CONTROL_NAMESPACE, &key)?;
        let plan = plan_guidance(
            save_operation,
            workspace,
            mutation,
            GuidanceObservation::new(catalog.as_ref(), guidance.as_ref(), tombstone.as_ref()),
        )?;
        self.accept_installs(operation, plan.into_installs())
    }

    pub(crate) fn guidance_page(
        &self,
        actor: ActorId,
        query: WorkbenchMemoryQuery,
    ) -> Result<WorkbenchMemory, Error> {
        self.authorize_guidance_query(actor, query)?;
        let workspace = query.query().workspace();
        let catalog_record =
            self.journal.state_record(GUIDANCE_CONTROL_NAMESPACE, &catalog_key(workspace))?;
        let catalog = replay_catalog(workspace, catalog_record.as_ref())?;
        if query.dependency_revision() != 0 && query.dependency_revision() != catalog.revision() {
            return Err(ControlError::StaleRevision.into());
        }
        let rows = self.guidance_rows(
            workspace,
            query.query().conversation(),
            query.include_forgotten(),
            catalog.identities(),
        )?;
        let total = u32::try_from(rows.len()).map_err(|_| ControlError::Capacity)?;
        let offset = usize::try_from(query.offset()).map_err(|_| ControlError::Capacity)?;
        if offset > rows.len() {
            return Err(ControlError::InvalidInput.into());
        }
        let page = rows.into_iter().skip(offset).take(MAX_WORKBENCH_GUIDANCE_PAGE).collect();
        WorkbenchMemory::new(query, catalog.revision(), total, page)
            .map_err(|_| ControlError::InvalidInput.into())
    }

    pub(crate) fn guidance_for_request(
        &self,
        workspace: WorkspaceId,
        conversation: peritus_app_protocol::ConversationId,
    ) -> Result<WorkbenchGuidanceRender, Error> {
        let catalog_record =
            self.journal.state_record(GUIDANCE_CONTROL_NAMESPACE, &catalog_key(workspace))?;
        let catalog = replay_catalog(workspace, catalog_record.as_ref())?;
        let rows = self.guidance_rows(workspace, conversation, false, catalog.identities())?;
        let records = rows
            .into_iter()
            .filter_map(|row| match row {
                WorkbenchMemoryRow::Active(record) => Some(record),
                WorkbenchMemoryRow::Forgotten(_) => None,
            })
            .collect::<Vec<_>>();
        render_guidance_for_request(workspace, conversation, catalog.revision(), &records, &[])
            .map_err(|_| ControlError::InvalidInput.into())
    }

    fn authorize_guidance_query(
        &self,
        actor: ActorId,
        query: WorkbenchMemoryQuery,
    ) -> Result<(), Error> {
        let conversation = ConversationId::new(query.query().conversation().into_bytes())?;
        let record = self.load(conversation)?.ok_or(ControlError::NotFound)?;
        if record.owner_bytes() != actor.as_bytes()
            || record.workspace_bytes() != query.query().workspace().as_bytes()
        {
            return Err(ControlError::ScopeMismatch.into());
        }
        Ok(())
    }

    fn guidance_rows(
        &self,
        workspace: WorkspaceId,
        conversation: peritus_app_protocol::ConversationId,
        include_forgotten: bool,
        identities: &[ControlOperationId],
    ) -> Result<Vec<WorkbenchMemoryRow>, Error> {
        let mut rows = Vec::with_capacity(identities.len());
        for identity in identities {
            let key = guidance_key(workspace, *identity);
            let guidance = self
                .journal
                .state_record(GUIDANCE_NAMESPACE, &key)?
                .ok_or(Error::Corrupt("guidance catalog entry is missing state"))?;
            let tombstone = self.journal.state_record(GUIDANCE_CONTROL_NAMESPACE, &key)?;
            let row = replay_guidance(workspace, *identity, &guidance, tombstone.as_ref())?;
            let visible = match &row {
                WorkbenchMemoryRow::Active(record) => {
                    scope_matches(record.content().scope(), conversation)
                }
                WorkbenchMemoryRow::Forgotten(tombstone) => {
                    include_forgotten && scope_matches(tombstone.prior().scope(), conversation)
                }
            };
            if visible {
                rows.push(row);
            }
        }
        Ok(rows)
    }
}

fn scope_matches(
    scope: WorkbenchGuidanceScope,
    conversation: peritus_app_protocol::ConversationId,
) -> bool {
    matches!(scope, WorkbenchGuidanceScope::Project)
        || matches!(scope, WorkbenchGuidanceScope::Conversation(id) if id.as_bytes() == conversation.as_bytes())
}
