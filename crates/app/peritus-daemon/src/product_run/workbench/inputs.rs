//! Authenticated queue mapping; host-only incorporation has no A3 intent variant.

use super::{Error, ProductRunService, error_response};
use peritus_app_protocol::{
    AppResponsePayload, MAX_WORKBENCH_INPUT_PAGE, WorkbenchInputId, WorkbenchInputOrder,
    WorkbenchInputRow, WorkbenchInputSelection, WorkbenchInputState, WorkbenchInputText,
    WorkbenchQueueIntent, WorkbenchQueuePage, WorkbenchQueueQuery,
};
use peritus_product_runner::control::{
    ControlError, ControlText, ConversationId, InputId, InputRevision, InputSelection, InputState,
    QueueIntent,
};
use peritus_types::ActorId;

impl ProductRunService {
    pub(crate) fn workbench_queue(
        &self,
        actor: ActorId,
        query: WorkbenchQueueQuery,
    ) -> AppResponsePayload {
        let result = self.control_workspace(query.query()).and_then(|()| {
            let id = ConversationId::new(query.query().conversation().into_bytes())?;
            let record =
                self.with_controls(false, |store| store.load(id))?.ok_or(ControlError::NotFound)?;
            if record.owner_bytes() != actor.as_bytes()
                || record.workspace_bytes() != query.query().workspace().as_bytes()
            {
                return Err(ControlError::ScopeMismatch.into());
            }
            if query.revision() != 0 && query.revision() != record.revision() {
                return Err(ControlError::StaleRevision.into());
            }
            let inputs = record.inputs();
            let rows: Vec<_> = if query.history() {
                inputs.revisions().iter().collect()
            } else {
                inputs
                    .order()
                    .iter()
                    .map(|id| inputs.latest(*id).ok_or(ControlError::InvalidInput))
                    .collect::<Result<_, _>>()?
            };
            let total = u32::try_from(rows.len()).map_err(|_| ControlError::Capacity)?;
            let page = rows
                .into_iter()
                .skip(query.offset() as usize)
                .take(MAX_WORKBENCH_INPUT_PAGE)
                .map(project_row)
                .collect::<Result<_, _>>()?;
            let query = WorkbenchQueueQuery::new(
                query.query(),
                record.revision(),
                query.offset(),
                query.history(),
            )
            .map_err(|_| ControlError::InvalidInput)?;
            WorkbenchQueuePage::new(query, total, page)
                .map_err(|_| ControlError::InvalidInput.into())
        });
        result.map_or_else(error_response, AppResponsePayload::WorkbenchQueue)
    }
}

fn selection(value: WorkbenchInputSelection) -> Result<InputSelection, ControlError> {
    InputSelection::new(InputId::new(value.id().into_bytes())?, value.revision())
}
fn ids(value: &WorkbenchInputOrder) -> Result<Vec<InputId>, ControlError> {
    value.ids().iter().map(|id| InputId::new(id.into_bytes())).collect()
}

pub(super) fn domain_intent(value: &WorkbenchQueueIntent) -> Result<QueueIntent, ControlError> {
    Ok(match value {
        WorkbenchQueueIntent::Enqueue(value) => QueueIntent::Enqueue {
            id: InputId::new(value.id().into_bytes())?,
            text: ControlText::new(value.text().as_str().to_owned())?,
            dependencies: ids(value.dependencies())?,
        },
        WorkbenchQueueIntent::Edit { selected, text } => QueueIntent::Edit {
            selected: selection(*selected)?,
            text: ControlText::new(text.as_str().to_owned())?,
        },
        WorkbenchQueueIntent::Correct { original, id, text } => QueueIntent::Correct {
            original: selection(*original)?,
            id: InputId::new(id.into_bytes())?,
            text: ControlText::new(text.as_str().to_owned())?,
        },
        WorkbenchQueueIntent::Hold { selected, held } => {
            QueueIntent::Hold { selected: selection(*selected)?, held: *held }
        }
        WorkbenchQueueIntent::Withdraw(selected) => QueueIntent::Withdraw(selection(*selected)?),
        WorkbenchQueueIntent::Reorder(value) => QueueIntent::Reorder(ids(value)?),
    })
}
pub(super) fn project_row(value: &InputRevision) -> Result<WorkbenchInputRow, Error> {
    let id = WorkbenchInputId::new(*value.selection().id().as_bytes())
        .map_err(|_| ControlError::InvalidInput)?;
    let selected = WorkbenchInputSelection::new(id, value.selection().revision())
        .map_err(|_| ControlError::InvalidInput)?;
    let text =
        WorkbenchInputText::new(value.text().to_owned()).map_err(|_| ControlError::InvalidInput)?;
    let state = match value.state() {
        InputState::Queued => WorkbenchInputState::Queued,
        InputState::Held => WorkbenchInputState::Held,
        InputState::Incorporated => WorkbenchInputState::Incorporated,
        InputState::Superseded => WorkbenchInputState::Superseded,
        InputState::Withdrawn => WorkbenchInputState::Withdrawn,
    };
    let dependencies = value
        .dependencies()
        .iter()
        .map(|id| WorkbenchInputId::new(*id.as_bytes()))
        .collect::<Result<_, _>>()
        .map_err(|_| ControlError::InvalidInput)?;
    let dependencies =
        WorkbenchInputOrder::new(dependencies).map_err(|_| ControlError::InvalidInput)?;
    WorkbenchInputRow::new(selected, text, state, dependencies)
        .map_err(|_| ControlError::InvalidInput.into())
}
