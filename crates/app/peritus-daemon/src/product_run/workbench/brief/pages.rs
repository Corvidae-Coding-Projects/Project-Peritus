//! Paged metadata never opens reply bodies; exact body reads verify the retained publication.
use super::{ProductRunService, project_field, project_row};
use crate::product_control::ControlStoreError as Error;
use peritus_app_protocol::{
    AppResponsePayload, ControlOperationId, WORKBENCH_BRIEF_BODY_BYTES, WORKBENCH_BRIEF_PAGE_ITEMS,
    WorkbenchBriefEntry, WorkbenchBriefObservation, WorkbenchBriefObservationKind,
    WorkbenchBriefPage, WorkbenchBriefPageRequest, WorkbenchBriefProposalPage,
    WorkbenchBriefProposalReference, WorkbenchBriefProposalRequest, WorkbenchInvocationId,
    WorkbenchQuery,
};
use peritus_product_runner::control::{
    ControlError, ConversationId, ConversationRecord, PublicReplyReference,
};
use peritus_types::ActorId;

impl ProductRunService {
    pub(crate) fn workbench_brief_page(
        &self,
        actor: ActorId,
        request: WorkbenchBriefPageRequest,
    ) -> AppResponsePayload {
        let result = self.control_workspace(request.query()).and_then(|()| {
            self.with_controls(false, |store| {
                let record = store
                    .load(ConversationId::new(request.query().conversation().into_bytes())?)?
                    .ok_or(ControlError::NotFound)?;
                check_scope(&record, actor, request.query(), request.revision())?;
                let mut replies: Vec<_> = record.replies().iter().collect();
                replies.sort_by_key(|reply| *reply.after_invocation().as_bytes());
                let proposals = replies
                    .iter()
                    .skip(index(request.proposals())?)
                    .take(WORKBENCH_BRIEF_PAGE_ITEMS)
                    .map(|reply| reference(reply))
                    .collect::<Result<_, _>>()?;
                let (observation_total, observations) =
                    observations(&record, request.observations())?;
                WorkbenchBriefPage::new(
                    request,
                    record.revision(),
                    entries(&record)?,
                    replies.len() as u64,
                    proposals,
                    observation_total,
                    observations,
                )
                .map_err(|_| ControlError::InvalidInput.into())
            })
        });
        result.map_or_else(super::super::error_response, AppResponsePayload::WorkbenchBriefPage)
    }

    pub(crate) fn workbench_brief_proposal(
        &self,
        actor: ActorId,
        request: WorkbenchBriefProposalRequest,
    ) -> AppResponsePayload {
        let result = self.control_workspace(request.query()).and_then(|()| {
            self.with_controls(false, |store| {
                let record = store
                    .load(ConversationId::new(request.query().conversation().into_bytes())?)?
                    .ok_or(ControlError::NotFound)?;
                check_scope(&record, actor, request.query(), request.revision())?;
                let reply = record
                    .replies()
                    .iter()
                    .find(|reply| {
                        reply.operation().as_bytes() == request.proposal().operation().as_bytes()
                    })
                    .ok_or(ControlError::NotFound)?;
                if reference(reply)? != request.proposal() {
                    return Err(ControlError::InvalidInput.into());
                }
                let text = store.reply_text(reply)?;
                let start = index(request.offset())?;
                if !text.is_char_boundary(start) {
                    return Err(ControlError::InvalidInput.into());
                }
                let mut end = start.saturating_add(WORKBENCH_BRIEF_BODY_BYTES).min(text.len());
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                let body = text.get(start..end).ok_or(ControlError::InvalidInput)?.to_owned();
                WorkbenchBriefProposalPage::new(request, body)
                    .map_err(|_| ControlError::InvalidInput.into())
            })
        });
        result.map_or_else(super::super::error_response, AppResponsePayload::WorkbenchBriefProposal)
    }
}
fn check_scope(
    record: &ConversationRecord,
    actor: ActorId,
    query: WorkbenchQuery,
    revision: u64,
) -> Result<(), Error> {
    if record.owner_bytes() != actor.as_bytes()
        || record.workspace_bytes() != query.workspace().as_bytes()
    {
        return Err(ControlError::ScopeMismatch.into());
    }
    if revision != 0 && record.revision() != revision {
        return Err(ControlError::StaleRevision.into());
    }
    Ok(())
}
fn index(value: u64) -> Result<usize, Error> {
    usize::try_from(value).map_err(|_| ControlError::InvalidInput.into())
}
fn operation(bytes: [u8; 16]) -> Result<ControlOperationId, Error> {
    ControlOperationId::new(bytes).map_err(|_| ControlError::InvalidInput.into())
}
fn reference(reply: &PublicReplyReference) -> Result<WorkbenchBriefProposalReference, Error> {
    WorkbenchBriefProposalReference::new(
        operation(*reply.operation().as_bytes())?,
        WorkbenchInvocationId::new(*reply.after_invocation().as_bytes())
            .map_err(|_| ControlError::InvalidInput)?,
        reply.digest(),
        reply.bytes(),
    )
    .map_err(|_| ControlError::InvalidInput.into())
}
fn entries(record: &ConversationRecord) -> Result<Vec<WorkbenchBriefEntry>, Error> {
    record
        .brief()
        .bindings()
        .iter()
        .map(|binding| {
            let source = record
                .inputs()
                .latest(binding.selected().id())
                .ok_or(ControlError::InvalidInput)?;
            if source.selection() != binding.selected() {
                return Err(ControlError::InvalidInput.into());
            }
            WorkbenchBriefEntry::new(project_field(binding.field()), project_row(source)?)
                .map_err(|_| ControlError::InvalidInput.into())
        })
        .collect()
}
fn observations(
    record: &ConversationRecord,
    offset: u64,
) -> Result<(u64, Vec<WorkbenchBriefObservation>), Error> {
    // Sort borrowed identities only. Labels and metadata are cloned for the requested page.
    let mut keys: Vec<_> = record
        .images()
        .entries()
        .iter()
        .enumerate()
        .map(|(i, row)| (*row.image().operation().as_bytes(), true, i))
        .chain(
            record
                .files()
                .entries()
                .iter()
                .enumerate()
                .map(|(i, row)| (*row.file().operation().as_bytes(), false, i)),
        )
        .collect();
    keys.sort_by_key(|(operation, _, _)| *operation);
    let total = keys.len() as u64;
    let rows = keys
        .into_iter()
        .skip(index(offset)?)
        .take(WORKBENCH_BRIEF_PAGE_ITEMS)
        .map(|(id, image, index)| {
            let value = if image {
                let row = &record.images().entries()[index];
                WorkbenchBriefObservation::new(
                    WorkbenchBriefObservationKind::Image,
                    operation(id)?,
                    None,
                    row.image().label().to_owned(),
                    row.image().digest(),
                    row.image().bytes(),
                    row.selected(),
                )
            } else {
                let row = &record.files().entries()[index];
                WorkbenchBriefObservation::new(
                    WorkbenchBriefObservationKind::File,
                    operation(id)?,
                    Some(operation(*row.current().operation().as_bytes())?),
                    row.file().source().label().to_owned(),
                    row.current().observation().digest(),
                    row.current().observation().bytes(),
                    row.selected(),
                )
            };
            value.map_err(|_| ControlError::InvalidInput.into())
        })
        .collect::<Result<_, Error>>()?;
    Ok((total, rows))
}
