//! Authenticated, read-only task-brief projection from the authoritative input ledger.

use super::{ProductRunService, error_response, inputs::project_row};
use peritus_app_protocol::{
    AppResponsePayload, ControlOperationId, WorkbenchBrief, WorkbenchBriefEntry,
    WorkbenchBriefField, WorkbenchBriefObservation, WorkbenchBriefObservationKind,
    WorkbenchBriefProposal, WorkbenchInputText, WorkbenchInvocationId, WorkbenchQuery,
};
use peritus_product_runner::control::{BriefField, ControlError, ConversationId};
use peritus_types::ActorId;

pub(super) const fn domain_field(field: WorkbenchBriefField) -> BriefField {
    match field {
        WorkbenchBriefField::Objective => BriefField::Objective,
        WorkbenchBriefField::Acceptance => BriefField::Acceptance,
        WorkbenchBriefField::Constraints => BriefField::Constraints,
        WorkbenchBriefField::Assumptions => BriefField::Assumptions,
    }
}
const fn project_field(field: BriefField) -> WorkbenchBriefField {
    match field {
        BriefField::Objective => WorkbenchBriefField::Objective,
        BriefField::Acceptance => WorkbenchBriefField::Acceptance,
        BriefField::Constraints => WorkbenchBriefField::Constraints,
        BriefField::Assumptions => WorkbenchBriefField::Assumptions,
    }
}
impl ProductRunService {
    pub(crate) fn workbench_brief(
        &self,
        actor: ActorId,
        query: WorkbenchQuery,
    ) -> AppResponsePayload {
        let result = self.control_workspace(query).and_then(|()| {
            let id = ConversationId::new(query.conversation().into_bytes())?;
            self.with_controls(false, |store| {
                let record = store.load(id)?.ok_or(ControlError::NotFound)?;
                if record.owner_bytes() != actor.as_bytes()
                    || record.workspace_bytes() != query.workspace().as_bytes()
                {
                    return Err(ControlError::ScopeMismatch.into());
                }
                let mut entries = Vec::with_capacity(record.brief().bindings().len());
                for binding in record.brief().bindings() {
                    let source = record
                        .inputs()
                        .latest(binding.selected().id())
                        .ok_or(ControlError::InvalidInput)?;
                    if source.selection() != binding.selected() {
                        return Err(ControlError::InvalidInput.into());
                    }
                    entries.push(
                        WorkbenchBriefEntry::new(
                            project_field(binding.field()),
                            project_row(source)?,
                        )
                        .map_err(|_| ControlError::InvalidInput)?,
                    );
                }

                let mut excluded = 0_u32;
                let mut proposals = Vec::new();
                for reply in record.replies().iter().rev() {
                    if proposals.len() == peritus_app_protocol::MAX_WORKBENCH_BRIEF_PROPOSALS {
                        excluded = excluded.checked_add(1).ok_or(ControlError::Capacity)?;
                        continue;
                    }
                    let Ok(text) = WorkbenchInputText::new(store.reply_text(reply)?) else {
                        excluded = excluded.checked_add(1).ok_or(ControlError::Capacity)?;
                        continue;
                    };
                    proposals.push(
                        WorkbenchBriefProposal::new(
                            ControlOperationId::new(*reply.operation().as_bytes())
                                .map_err(|_| ControlError::InvalidInput)?,
                            WorkbenchInvocationId::new(*reply.after_invocation().as_bytes())
                                .map_err(|_| ControlError::InvalidInput)?,
                            reply.digest(),
                            text,
                        )
                        .map_err(|_| ControlError::InvalidInput)?,
                    );
                }
                proposals.reverse();

                let mut observations = Vec::new();
                for image in record.images().entries() {
                    let source = image.image();
                    observations.push(
                        WorkbenchBriefObservation::new(
                            WorkbenchBriefObservationKind::Image,
                            ControlOperationId::new(*source.operation().as_bytes())
                                .map_err(|_| ControlError::InvalidInput)?,
                            None,
                            source.label().to_owned(),
                            source.digest(),
                            source.bytes(),
                            image.selected(),
                        )
                        .map_err(|_| ControlError::InvalidInput)?,
                    );
                }
                for file in record.files().entries() {
                    let source = file.file();
                    observations.push(
                        WorkbenchBriefObservation::new(
                            WorkbenchBriefObservationKind::File,
                            ControlOperationId::new(*source.operation().as_bytes())
                                .map_err(|_| ControlError::InvalidInput)?,
                            Some(
                                ControlOperationId::new(*file.current().operation().as_bytes())
                                    .map_err(|_| ControlError::InvalidInput)?,
                            ),
                            source.source().label().to_owned(),
                            file.current().observation().digest(),
                            file.current().observation().bytes(),
                            file.selected(),
                        )
                        .map_err(|_| ControlError::InvalidInput)?,
                    );
                }
                observations.sort_by_key(WorkbenchBriefObservation::operation);
                WorkbenchBrief::with_sources(
                    query,
                    record.revision(),
                    entries,
                    proposals,
                    observations,
                    excluded,
                )
                .map_err(|_| ControlError::InvalidInput.into())
            })
        });
        result.map_or_else(error_response, AppResponsePayload::WorkbenchBrief)
    }
}
