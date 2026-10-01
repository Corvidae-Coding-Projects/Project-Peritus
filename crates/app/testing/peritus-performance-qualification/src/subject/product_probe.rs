//! Real product/provider admission probe required before an H3 subject may collect measurements.

use std::{
    thread,
    time::{Duration, Instant},
};

use peritus_app_protocol::{
    AppRequestPayload, AppResponsePayload, ControlOperationId, ConversationId, ConversationTitle,
    CorrelationId, ProductActivityKind, ProductInteractionMode, ProductProviderSelection,
    ProductRoleModels, ProductRunConversationQuery, ProductRunPhase, RequestId, WorkbenchCommand,
    WorkbenchExecutionSettings, WorkbenchInputId, WorkbenchInputOrder, WorkbenchInputText,
    WorkbenchIntent, WorkbenchNewInput, WorkbenchQuery, WorkbenchQueueIntent, WorkbenchReceipt,
};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};

use crate::{SubjectError, a3::A3Client, identity::IdentitySource};

const PROBE_BOUND: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(20);
const TASK: &str = "Return the deterministic H3 product-path response.";

pub(super) fn qualify(
    client: &mut A3Client,
    identities: &mut IdentitySource,
) -> Result<(), SubjectError> {
    let run = identities.next(RunId::new)?;
    let workspace = WorkspaceId::new([0x55; 16]).map_err(SubjectError::Identifier)?;
    let provider = ProviderProfileId::new([0x44; 16]).map_err(SubjectError::Identifier)?;
    let providers = ProductProviderSelection::new(provider, provider, provider);
    let query = WorkbenchQuery::new(identities.next(ConversationId::new)?, workspace);
    let create = WorkbenchCommand::new(
        identities.next(ControlOperationId::new)?,
        query,
        0,
        WorkbenchIntent::CreateConversation(
            ConversationTitle::new("H3 product path probe".to_owned()).map_err(|error| {
                SubjectError::Configuration(format!(
                    "could not construct H3 probe title: {error:?}"
                ))
            })?,
        ),
    );
    let created = require_receipt(
        request(client, identities, AppRequestPayload::WorkbenchCommand(create.clone()))?,
        &create,
    )?;
    let input = WorkbenchNewInput::new(
        identities.next(WorkbenchInputId::new)?,
        WorkbenchInputText::new(TASK.to_owned()).map_err(|error| {
            SubjectError::Configuration(format!("could not construct H3 probe input: {error:?}"))
        })?,
        WorkbenchInputOrder::new(Vec::new()).map_err(|error| {
            SubjectError::Configuration(format!("could not construct H3 probe order: {error:?}"))
        })?,
    )
    .map_err(|error| {
        SubjectError::Configuration(format!("could not construct H3 probe queue item: {error:?}"))
    })?;
    let enqueue = WorkbenchCommand::new(
        identities.next(ControlOperationId::new)?,
        query,
        created.accepted_revision(),
        WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(input)),
    );
    let queued = require_receipt(
        request(client, identities, AppRequestPayload::WorkbenchCommand(enqueue.clone()))?,
        &enqueue,
    )?;
    let start = WorkbenchCommand::new(
        identities.next(ControlOperationId::new)?,
        query,
        queued.accepted_revision(),
        WorkbenchIntent::StartExecution(WorkbenchExecutionSettings::new(
            run,
            providers,
            ProductInteractionMode::Plan,
            ProductRoleModels::default(),
        )),
    );
    require_receipt(
        request(client, identities, AppRequestPayload::WorkbenchCommand(start.clone()))?,
        &start,
    )?;
    require_interaction(
        request(
            client,
            identities,
            AppRequestPayload::QueryInteraction(ProductRunConversationQuery::new(run)),
        )?,
        run,
        false,
    )?;

    let deadline = Instant::now() + PROBE_BOUND;
    loop {
        let response = request(
            client,
            identities,
            AppRequestPayload::QueryInteraction(ProductRunConversationQuery::new(run)),
        )?;
        if require_interaction(response, run, true)? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(SubjectError::UnexpectedResponse(
                "H3 product/provider probe did not settle within 10 seconds".to_owned(),
            ));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn require_receipt(
    response: AppResponsePayload,
    command: &WorkbenchCommand,
) -> Result<WorkbenchReceipt, SubjectError> {
    let AppResponsePayload::WorkbenchReceipt(receipt) = response else {
        return Err(SubjectError::UnexpectedResponse(format!(
            "H3 workbench probe returned {response:?}"
        )));
    };
    if receipt.operation() != command.operation() || receipt.query() != command.query() {
        return Err(SubjectError::UnexpectedResponse(
            "H3 workbench probe returned a receipt for another operation".to_owned(),
        ));
    }
    Ok(receipt)
}

fn request(
    client: &mut A3Client,
    identities: &mut IdentitySource,
    payload: AppRequestPayload,
) -> Result<AppResponsePayload, SubjectError> {
    let response = client.request(
        identities.next(RequestId::new)?,
        identities.next(CorrelationId::new)?,
        payload,
        identities,
    )?;
    Ok(response.payload().clone())
}

fn require_interaction(
    response: AppResponsePayload,
    run: RunId,
    require_settled: bool,
) -> Result<bool, SubjectError> {
    let AppResponsePayload::Interaction(snapshot) = response else {
        return Err(SubjectError::UnexpectedResponse(format!(
            "H3 product/provider probe returned {response:?}"
        )));
    };
    if snapshot.snapshot().run_id() != run || snapshot.received() != 1 {
        return Err(SubjectError::UnexpectedResponse(
            "H3 product/provider probe returned another run or input revision".to_owned(),
        ));
    }
    if !require_settled {
        return Ok(false);
    }
    let has_user = snapshot
        .activities()
        .iter()
        .any(|activity| activity.kind() == ProductActivityKind::User && activity.text() == TASK);
    let has_answer = snapshot.activities().iter().any(|activity| {
        activity.kind() == ProductActivityKind::Assistant
            && activity.text().contains("h3-product-path")
    });
    Ok(snapshot.incorporated() == 1
        && snapshot.snapshot().phase() == ProductRunPhase::WaitingForUser
        && has_user
        && has_answer)
}
