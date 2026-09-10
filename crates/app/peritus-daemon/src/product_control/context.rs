//! Authenticated read-only context inspection. No recovery, provider, workspace read, or mutation.

use super::{ControlStore, ControlStoreError as Error};
use peritus_app_protocol::{
    MAX_WORKBENCH_CONTEXT_PAGE, WorkbenchContextDisposition as D, WorkbenchContextPage,
    WorkbenchContextPreference as P, WorkbenchContextQuery, WorkbenchContextRow,
    WorkbenchContextSource as S, WorkbenchContextView as V, WorkbenchInputId,
    WorkbenchInputSelection, WorkbenchInvocationId,
};
use peritus_product_runner::control::{
    ContextPreference, ContextTarget, ControlError, ConversationId, ConversationRecord, InputState,
    InvocationId,
};
use peritus_types::ActorId;

impl ControlStore {
    pub(crate) fn context_page(
        &self,
        actor: ActorId,
        query: WorkbenchContextQuery,
    ) -> Result<WorkbenchContextPage, Error> {
        let scope = query.query();
        let id = ConversationId::new(scope.conversation().into_bytes())?;
        let record = self.load(id)?.ok_or(ControlError::NotFound)?;
        if record.owner_bytes() != actor.as_bytes()
            || record.workspace_bytes() != scope.workspace().as_bytes()
        {
            return Err(ControlError::ScopeMismatch.into());
        }
        if query.revision() != 0 && query.revision() != record.revision() {
            return Err(ControlError::StaleRevision.into());
        }
        let query =
            WorkbenchContextQuery::new(scope, record.revision(), query.offset(), query.view())
                .map_err(|_| ControlError::InvalidInput)?;
        let (total, seal, rows) = match query.view() {
            V::Next => {
                let rows = next_rows(&record)?;
                (rows.len(), None, page(rows, query.offset()))
            }
            V::Invocation(invocation) => {
                let bytes =
                    self.invocation_manifest(id, InvocationId::new(invocation.into_bytes())?)?;
                let (seal, _, rows) = super::inputs::inspect_manifest(&bytes)?;
                (rows.len(), Some(seal), page(rows, query.offset()))
            }
            V::History => (
                record.inputs().invocations().len(),
                None,
                self.history_rows(&record, query.offset())?,
            ),
        };
        WorkbenchContextPage::new(
            query,
            u32::try_from(total).map_err(|_| ControlError::Capacity)?,
            seal,
            rows,
        )
        .map_err(|_| ControlError::InvalidInput.into())
    }

    fn history_rows(
        &self,
        record: &ConversationRecord,
        offset: u32,
    ) -> Result<Vec<WorkbenchContextRow>, Error> {
        record
            .inputs()
            .invocations()
            .iter()
            .skip(offset as usize)
            .take(MAX_WORKBENCH_CONTEXT_PAGE)
            .map(|binding| {
                let bytes = self.invocation_manifest(record.id(), binding.invocation())?;
                let (seal, request_bytes, _) = super::inputs::inspect_manifest(&bytes)?;
                WorkbenchContextRow::new(
                    S::Invocation {
                        id: seal.invocation(),
                        manifest_digest: seal.manifest_digest(),
                    },
                    seal.request_digest(),
                    request_bytes,
                    D::Included,
                )
                .map_err(|_| Error::Corrupt("invalid invocation index metadata"))
            })
            .collect()
    }
}

fn page(rows: Vec<WorkbenchContextRow>, offset: u32) -> Vec<WorkbenchContextRow> {
    rows.into_iter().skip(offset as usize).take(MAX_WORKBENCH_CONTEXT_PAGE).collect()
}

fn next_rows(record: &ConversationRecord) -> Result<Vec<WorkbenchContextRow>, Error> {
    let replies =
        record.replies().iter().map(|reply| (reply.after_invocation(), String::new())).collect();
    let capture = record.capture_with_replies(&replies, true)?;
    let mut rows = Vec::new();
    for input in record.inputs().revisions() {
        let selected = WorkbenchInputSelection::new(
            WorkbenchInputId::new(*input.selection().id().as_bytes())
                .map_err(|_| ControlError::InvalidInput)?,
            input.selection().revision(),
        )
        .map_err(|_| ControlError::InvalidInput)?;
        let state = if capture.included().contains(&input.selection()) {
            D::Eligible
        } else {
            match input.state() {
                InputState::Held => D::Held,
                InputState::Withdrawn => D::Withdrawn,
                InputState::Superseded => D::Superseded,
                InputState::Queued => D::DependencyBlocked,
                InputState::Incorporated => {
                    return Err(Error::Corrupt("incorporated source missing from eligible view"));
                }
            }
        };
        rows.push(context_row(
            record,
            ContextTarget::Input(input.selection()),
            WorkbenchContextRow::new(
                S::Input(selected),
                peritus_codec::sha256(input.text().as_bytes()),
                input.text().len() as u64,
                state,
            )
            .map_err(|_| ControlError::InvalidInput)?,
        )?);
    }
    for reply in record.replies() {
        let id = WorkbenchInvocationId::new(*reply.after_invocation().as_bytes())
            .map_err(|_| ControlError::InvalidInput)?;
        let target = ContextTarget::PublicReply(reply.after_invocation());
        let disposition =
            if record.context().preference(target) == Some(ContextPreference::Excluded) {
                D::UserExcluded
            } else if capture.public_replies().contains(&reply.after_invocation()) {
                D::Eligible
            } else {
                D::AwaitingLaterInput
            };
        rows.push(context_row(
            record,
            target,
            WorkbenchContextRow::new(
                S::PublicReply(id),
                reply.digest(),
                reply.bytes(),
                disposition,
            )
            .map_err(|_| ControlError::InvalidInput)?,
        )?);
    }
    rows.extend(image_rows(record, capture.included())?);
    for entry in record.files().entries() {
        let target = ContextTarget::File(entry.file().operation());
        let preference = record.context().preference(target);
        let disposition = if preference == Some(ContextPreference::Excluded) {
            D::UserExcluded
        } else if record.eligible_files(capture.included()).contains(&entry) {
            D::Eligible
        } else if !entry.selected() {
            D::Deselected
        } else {
            match record
                .inputs()
                .latest(entry.file().input())
                .ok_or(ControlError::InvalidInput)?
                .state()
            {
                InputState::Held => D::Held,
                InputState::Withdrawn => D::Withdrawn,
                InputState::Superseded => D::Superseded,
                _ => D::DependencyBlocked,
            }
        };
        rows.push(context_row(
            record,
            target,
            WorkbenchContextRow::new(
                file_source(entry.file().operation(), entry.current().operation())?,
                entry.current().observation().digest(),
                entry.current().observation().bytes(),
                disposition,
            )
            .map_err(|_| ControlError::InvalidInput)?,
        )?);
    }
    Ok(rows)
}

pub(super) fn file_source(
    attachment: peritus_product_runner::control::OperationId,
    version: peritus_product_runner::control::OperationId,
) -> Result<S, Error> {
    Ok(S::File {
        attachment: peritus_app_protocol::ControlOperationId::new(*attachment.as_bytes())
            .map_err(|_| ControlError::InvalidInput)?,
        version: peritus_app_protocol::ControlOperationId::new(*version.as_bytes())
            .map_err(|_| ControlError::InvalidInput)?,
    })
}

fn image_rows(
    record: &ConversationRecord,
    included: &[peritus_product_runner::control::InputSelection],
) -> Result<Vec<WorkbenchContextRow>, Error> {
    let eligible = record.eligible_images(included);
    record
        .images()
        .entries()
        .iter()
        .map(|entry| {
            let image = entry.image();
            let target = ContextTarget::Image(image.operation());
            let preference = record.context().preference(target);
            let disposition = if preference == Some(ContextPreference::Excluded) {
                D::UserExcluded
            } else if eligible.contains(&image) {
                D::Eligible
            } else if !entry.selected() {
                D::Deselected
            } else {
                match record
                    .inputs()
                    .latest(image.input())
                    .ok_or(Error::Corrupt("image input missing"))?
                    .state()
                {
                    InputState::Held => D::Held,
                    InputState::Withdrawn => D::Withdrawn,
                    InputState::Superseded => D::Superseded,
                    InputState::Queued => D::DependencyBlocked,
                    InputState::Incorporated => {
                        return Err(Error::Corrupt("incorporated image missing from eligibility"));
                    }
                }
            };
            let row = WorkbenchContextRow::new(
                image_source(image)?,
                image.digest(),
                image.bytes(),
                disposition,
            )
            .map_err(|_| ControlError::InvalidInput)?;
            context_row(record, target, row)
        })
        .collect()
}

fn context_row(
    record: &ConversationRecord,
    target: ContextTarget,
    row: WorkbenchContextRow,
) -> Result<WorkbenchContextRow, Error> {
    let Some(preference) = record.context().preference(target) else {
        return Ok(row);
    };
    let preference = match preference {
        ContextPreference::Pinned => P::Pinned,
        ContextPreference::Excluded => P::Excluded,
    };
    row.with_preference(preference).map_err(|_| ControlError::InvalidInput.into())
}

pub(super) fn image_source(
    image: &peritus_product_runner::control::ImageAttachment,
) -> Result<S, Error> {
    Ok(S::Image {
        operation: peritus_app_protocol::ControlOperationId::new(*image.operation().as_bytes())
            .map_err(|_| ControlError::InvalidInput)?,
        input: WorkbenchInputId::new(*image.input().as_bytes())
            .map_err(|_| ControlError::InvalidInput)?,
        artifact: peritus_types::ArtifactId::new(*image.artifact_bytes())
            .map_err(|_| ControlError::InvalidInput)?,
    })
}
