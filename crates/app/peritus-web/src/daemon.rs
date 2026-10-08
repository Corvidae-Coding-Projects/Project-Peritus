//! Native conversation adapter over the existing negotiated application client.

use crate::{
    error::{Result, problem},
    state::{App, Project, hex},
};
use peritus_app_client::Client;
use peritus_app_protocol::{
    AppErrorCode, AppRequestPayload, AppResponsePayload, ConversationId,
    ProductArtifactQuery, ProductArtifactReference, ProductDeliverable,
    ProductDeliverableIndexKind, ProductDeliverableIndexQuery,
    ProductDeliverableIndexReference,
    ProductInteractionMode, ProductInteractionQuery, ProductModelChoice, ProductModelEffort,
    ProductModelQuery, ProductProviderSelection, ProductRoleModels, ProductRunControl,
    ProductRunControlAction, ProductRunPageCursor, ProductRunReferenceQuery,
    ProductRunOperation, ProductRunReferenceSnapshot, ProductRunSnapshot, ProductRunStoreId,
    WorkbenchQuery, product_deliverable_index_reference,
};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

pub fn bytes(text: &str) -> Result<[u8; 16]> {
    if text.len() != 32 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(problem("Expected a 32-digit identity"));
    }
    let mut bytes = [0; 16];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).map_err(problem)?;
    }
    Ok(bytes)
}
pub fn endpoint(app: &App) -> Result<PathBuf> {
    Ok(current_target(app)?.endpoint().to_owned())
}
pub fn facts(app: &App, project: &Project) -> Result<Value> {
    discovery::current_facts(app, project)
}

mod chat;
mod connection;
mod conversation;
mod discovery;
mod prepared;
use prepared::PreparedChat;
pub mod improvements;
mod readiness;
pub mod receipts;
#[cfg(all(test, unix))]
mod tests;
pub use readiness::ready_facts;
pub(crate) use discovery::{
    NativeOwner, connect_owned, current_target, current_target_async, owner_for_project, raw_request_owned,
    raw_request_target,
};

async fn request(app: &App, payload: AppRequestPayload) -> Result<AppResponsePayload> {
    let response = raw_request(app, payload).await?;
    if let AppResponsePayload::Error(error) = &response {
        return Err(problem(format!("Daemon rejected the request: {error}")));
    }
    Ok(response)
}
pub(crate) async fn request_owned(
    app: &App,
    owner: &NativeOwner,
    payload: AppRequestPayload,
) -> Result<AppResponsePayload> {
    let response = raw_request_owned(app, owner, payload).await?;
    if let AppResponsePayload::Error(error) = &response {
        return Err(problem(format!("Daemon rejected the request: {error}")));
    }
    Ok(response)
}
pub async fn raw_request(app: &App, payload: AppRequestPayload) -> Result<AppResponsePayload> {
    let target = current_target_async(app).await?;
    raw_request_target(app, &target, payload).await.map(|(response, _)| response)
}
pub async fn status(app: &App) -> Result<Value> {
    match request(app, AppRequestPayload::DaemonStatus).await? {
        AppResponsePayload::DaemonStatus(status) => Ok(
            json!({"connected":true,"ready":status.mutation_ready(),"readiness":format!("{:?}",status.readiness()),"diagnostic":status.diagnostic(),"detail":format!("{status:?}")}),
        ),
        _ => Err(problem("Unexpected daemon status response")),
    }
}
fn snapshot(value: &ProductRunSnapshot) -> Value {
    let controls = value.operation().legal_controls();
    json!({"id":hex(value.run_id().as_bytes()),"workspace":hex(value.workspace_id().as_bytes()),"providers":{"writer":hex(value.providers().writer().as_bytes()),"reviewer":hex(value.providers().reviewer().as_bytes()),"fixer":hex(value.providers().fixer().as_bytes())},"phase":format!("{:?}",value.phase()),"task":value.task(),"status":value.status(),"diff":value.diff(),"gates":value.gates(),"review":value.review(),"summary":value.summary(),"operation":{"kind":format!("{:?}",value.operation().kind()),"state":format!("{:?}",value.operation().state()),"identity":value.operation().identity(),"known":value.operation().known(),"uncertainty":value.operation().uncertainty(),"legalControls":{"stop":controls.cancel(),"retry":controls.retry(),"accept":controls.accept(),"commit":controls.commit(),"export":controls.export(),"discard":controls.discard(),"acknowledge":controls.acknowledge()}},"deliverable":value.deliverable().map(|d|json!({"root":d.workspace_path(),"paths":d.changed_paths(),"instructions":d.run_instructions(),"qualification":format!("{:?}",d.qualification()),"accepted":d.accepted(),"commitRevision":d.commit_revision(),"exportPath":d.export_path(),"discarded":d.discarded()}))})
}
fn model_values(models: &ProductRoleModels) -> Value {
    let choice = |value: &ProductModelChoice| json!({"id":value.id(),"manual":value.manual(),"effort":value.effort().label()});
    json!({"writer":choice(models.writer()),"reviewer":choice(models.reviewer()),"fixer":choice(models.fixer())})
}
fn model_choice(value: &Value, role: &str) -> Result<ProductModelChoice> {
    let model = value[role]["id"].as_str().unwrap_or("");
    let choice = if model.is_empty() {
        ProductModelChoice::default()
    } else {
        ProductModelChoice::new(model.into(), value[role]["manual"].as_bool().unwrap_or(false))
            .map_err(problem)?
    };
    let effort = ProductModelEffort::parse(value[role]["effort"].as_str().unwrap_or("default"))
        .ok_or_else(|| problem("Unknown model effort"))?;
    Ok(choice.with_effort(effort))
}
fn role_models(value: &Value) -> Result<ProductRoleModels> {
    Ok(ProductRoleModels::new(
        model_choice(value, "writer")?,
        model_choice(value, "reviewer")?,
        model_choice(value, "fixer")?,
    ))
}
fn interaction_mode(value: &str) -> Result<ProductInteractionMode> {
    match value {
        "chat" => Ok(ProductInteractionMode::Chat),
        "plan" => Ok(ProductInteractionMode::Plan),
        "review" => Ok(ProductInteractionMode::Review),
        "build" => Ok(ProductInteractionMode::Build),
        _ => Err(problem("Unknown conversation mode")),
    }
}
pub(super) fn interaction_response(
    value: &peritus_app_protocol::ProductInteractionSnapshot,
    activities: &[peritus_app_protocol::ProductActivity],
) -> Value {
    let history = value.activity_window().map(|window| {
        let unavailable = activities
            .first()
            .map_or(0, |activity| activity.sequence().saturating_sub(1));
        json!({
            "digest": hex(window.history().as_bytes()),
            "total": window.total().to_string(),
            "unavailable": unavailable.to_string(),
            "liveOmitted": window.omitted().to_string(),
            "retainedErrors": window.retained_errors().iter().map(|activity| json!({
                "id": activity.sequence().to_string(),
                "kind": format!("{:?}", activity.kind()).to_lowercase(),
                "text": activity.text(),
                "detail": activity.detail(),
            })).collect::<Vec<_>>(),
            "omittedErrors": window.omitted_errors().to_string(),
            "terminalError": window.terminal_error().map(|activity| json!({
                "id": activity.sequence().to_string(),
                "kind": format!("{:?}", activity.kind()).to_lowercase(),
                "text": activity.text(),
                "detail": activity.detail(),
            })),
        })
    });
    json!({
        "run": snapshot(value.snapshot()),
        "models": model_values(value.models()),
        "mode": format!("{:?}", value.mode()).to_lowercase(),
        "received": value.received().to_string(),
        "incorporated": value.incorporated().to_string(),
        "history": history,
        "activities": activities.iter().map(|activity| json!({
            "id": activity.sequence().to_string(),
            "kind": format!("{:?}", activity.kind()).to_lowercase(),
            "text": activity.text(),
            "detail": activity.detail(),
        })).collect::<Vec<_>>(),
    })
}
pub fn response(value: AppResponsePayload) -> Result<Value> {
    match value {
        AppResponsePayload::Improvements(inbox) => Ok(improvements::projection(&inbox)),
        AppResponsePayload::ImprovementPage(page) => Ok(improvements::page_projection(&page)),
        AppResponsePayload::ImprovementEvidencePage(page) => {
            Ok(improvements::evidence_projection(&page))
        }
        AppResponsePayload::ImprovementTextPage(page) => {
            Ok(improvements::text_projection(&page))
        }
        AppResponsePayload::Interaction(value) => {
            Ok(interaction_response(&value, value.activities()))
        }
        AppResponsePayload::ProductRunAccepted(value) => Ok(json!({"run":snapshot(&value)})),
        AppResponsePayload::ProductRunSettled(value) => {
            Ok(json!({"run":snapshot(value.snapshot())}))
        }
        AppResponsePayload::Acknowledged(_) => Ok(json!({"acknowledged":true})),
        _ => Err(problem("Unexpected conversation response from daemon")),
    }
}

pub(crate) async fn response_owned(
    app: &App,
    owner: &NativeOwner,
    value: AppResponsePayload,
) -> Result<Value> {
    let AppResponsePayload::ProductRunReferencePage(page) = value else {
        return response(value);
    };
    if page.entries().len() != 1 || page.next().is_some() {
        return Err(problem(
            "The daemon control response did not identify exactly one product run",
        ));
    }
    let run = hydrate_run(app, owner.target(), page.entries()[0].snapshot()).await?;
    Ok(json!({"run":snapshot(&run)}))
}
pub async fn conversation(app: &App, session: &str) -> Result<Value> {
    conversation::observe(app, session).await
}
async fn prepare(app: &App, input: &Value) -> Result<PreparedChat> {
    let session = app.session(input["session"].as_str().unwrap_or(""))?;
    let project = app.project(&session.project)?;
    let (owner, facts) = match app.session_owner(&session.id)? {
        Some(owner) => {
            let facts = owner.facts_async(&project).await?;
            (owner, facts)
        }
        None => owner_for_project(app, &project).await?,
    };
    app.bind_session_owner(&session.id, owner.clone())?;
    let mut input = input.clone();
    if input.get("models").is_none() {
        input["models"] = serde_json::to_value(&session.settings.models)?;
    }
    if input.get("providers").is_none() {
        input["providers"] = json!(session.settings.providers);
    }
    let workspace =
        WorkspaceId::new(bytes(facts["workspace"]["id"].as_str().ok_or_else(|| {
            problem("Set up this project in the Setup console before sending a message.")
        })?)?)
        .map_err(|e| problem(format!("{e:?}")))?;
    let provider = |role: &str| -> Result<ProviderProfileId> {
        let id = input["providers"][role]
            .as_str()
            .filter(|id| !id.is_empty())
            .or_else(|| facts["providers"][0]["id"].as_str())
            .ok_or_else(|| problem("Select a provider in Setup console."))?;
        ProviderProfileId::new(bytes(id)?).map_err(|e| problem(format!("{e:?}")))
    };
    let mode = interaction_mode(input["mode"].as_str().unwrap_or("chat"))?;
    Ok(PreparedChat {
        query: WorkbenchQuery::new(
            ConversationId::new(bytes(&session.conversation)?)
                .map_err(|e| problem(format!("{e:?}")))?,
            workspace,
        ),
        run: RunId::new(bytes(&session.run)?).map_err(|e| problem(format!("{e:?}")))?,
        title: crate::sessions::title(&session.title)?,
        providers: ProductProviderSelection::new(
            provider("writer")?,
            provider("reviewer")?,
            provider("fixer")?,
        ),
        mode,
        models: role_models(&input["models"])?,
        text: input["text"].as_str().unwrap_or("").to_owned(),
        attachments: crate::files::attachments::selected(app, &input)?,
        owner: Some(owner),
    })
}
pub async fn send(app: &App, input: &Value) -> Result<Value> {
    chat::send(app, input).await
}
pub(crate) async fn send_owned(app: &App, input: &Value, owner: &crate::state::OperationOwner) -> Result<Value> {
    chat::send_owned(app, input, owner).await
}
pub async fn recover_send(app: &App, operation: &str) -> Result<Option<Value>> {
    chat::inspect(app, operation).await
}
pub async fn retry_send(app: &App, operation: &str) -> Result<Value> {
    chat::retry(app, operation).await
}
pub async fn control(app: &App, session: &str, action: &str, operation: &str) -> Result<Value> {
    let session = app.session(session)?;
    let owner = app.session_owner(&session.id)?.ok_or_else(|| {
        crate::error::uncertain(
            "This legacy browser session has no retained native owner. Reopen its exact run before sending a control.",
        )
    })?;
    let action = match action {
        "stop" => ProductRunControlAction::Cancel,
        "retry" => ProductRunControlAction::Retry,
        "export" => ProductRunControlAction::Export,
        "discard" => ProductRunControlAction::Discard,
        "acknowledge" => ProductRunControlAction::Acknowledge,
        _ => {
            return Err(problem(
                "Use the candidate console for acceptance and commit, including exact qualification confirmation.",
            ));
        }
    };
    app.bind_session_owner(&session.id, owner.clone())?;
    let payload = receipts::recorded(
            app,
            &owner,
            operation,
            AppRequestPayload::ControlProductRun(ProductRunControl::new(
                RunId::new(bytes(&session.run)?).map_err(|e| problem(format!("{e:?}")))?,
                action,
            )),
        )
        .await?;
    response_owned(app, &owner, payload).await
}
pub async fn models(app: &App, profile: &str) -> Result<Value> {
    match request(
        app,
        AppRequestPayload::QueryModels(ProductModelQuery::new(
            ProviderProfileId::new(bytes(profile)?).map_err(|e| problem(format!("{e:?}")))?,
            false,
        )),
    )
    .await?
    {
        AppResponsePayload::Models(catalog) => Ok(
            json!({"configured":catalog.configured(),"error":catalog.error(),"cached":catalog.cached(),"models":catalog.models().iter().map(|m|json!({"id":m.id(),"label":m.label()})).collect::<Vec<_>>()}),
        ),
        _ => Err(problem("Unexpected model catalog response")),
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RetainedRunCursor {
    version: u16,
    target: discovery::NativeTarget,
    store: String,
    highwater_sequence: String,
    highwater_run: String,
    after_sequence: String,
    after_run: String,
}

impl RetainedRunCursor {
    fn encode(target: discovery::NativeTarget, cursor: ProductRunPageCursor) -> Result<String> {
        serde_json::to_string(&Self {
            version: 1,
            target,
            store: hex(cursor.store().as_bytes()),
            highwater_sequence: cursor.highwater_sequence().to_string(),
            highwater_run: hex(cursor.highwater_run().as_bytes()),
            after_sequence: cursor.after_sequence().to_string(),
            after_run: hex(cursor.after_run().as_bytes()),
        })
        .map_err(Into::into)
    }

    fn decode(value: &str, target: &discovery::NativeTarget) -> Result<ProductRunPageCursor> {
        if value.len() > 64 * 1024 {
            return Err(problem("Run page cursor is too large"));
        }
        let retained: Self = serde_json::from_str(value).map_err(problem)?;
        if retained.version != 1 || retained.target != *target {
            return Err(problem(
                "This run page belongs to another daemon target. Refresh run history.",
            ));
        }
        if retained.store != target.store() {
            return Err(problem(
                "This run page belongs to another durable store. Refresh run history.",
            ));
        }
        let store = ProductRunStoreId::new(canonical_identity(&retained.store)?)
            .map_err(|error| problem(format!("{error:?}")))?;
        let highwater_sequence = canonical_u64(&retained.highwater_sequence)?;
        let highwater_run = RunId::new(canonical_identity(&retained.highwater_run)?)
            .map_err(|error| problem(format!("{error:?}")))?;
        let after_sequence = canonical_u64(&retained.after_sequence)?;
        let after_run = RunId::new(canonical_identity(&retained.after_run)?)
            .map_err(|error| problem(format!("{error:?}")))?;
        ProductRunPageCursor::new(
            store,
            highwater_sequence,
            highwater_run,
            after_sequence,
            after_run,
        )
        .map_err(problem)
    }
}

fn canonical_identity(value: &str) -> Result<[u8; 16]> {
    let decoded = bytes(value)?;
    if hex(&decoded) != value {
        return Err(problem("Run page cursor identity is not canonical"));
    }
    Ok(decoded)
}

fn canonical_u64(value: &str) -> Result<u64> {
    let decoded = value.parse::<u64>().map_err(problem)?;
    if decoded.to_string() != value {
        return Err(problem("Run page cursor position is not canonical"));
    }
    Ok(decoded)
}

pub async fn runs(app: &App, cursor: Option<&str>) -> Result<Value> {
    let target = current_target_async(app).await?;
    let query = match cursor {
        Some(value) => ProductRunReferenceQuery::after(RetainedRunCursor::decode(value, &target)?),
        None => ProductRunReferenceQuery::first(),
    };
    let (payload, _) = raw_request_target(
        app,
        &target,
        AppRequestPayload::QueryProductRunReferences(query),
    )
    .await?;
    let page = match payload {
        AppResponsePayload::ProductRunReferencePage(page) => page,
        AppResponsePayload::Error(error) => {
            return Err(problem(format!("Daemon rejected the request: {error}")));
        }
        _ => return Err(problem("Unexpected run page response")),
    };
    let store = hex(page.store().as_bytes());
    if store != target.store() {
        return Err(problem(
            "The daemon returned run history for another durable store. Refresh the target.",
        ));
    }
    let next = page
        .next()
        .map(|cursor| RetainedRunCursor::encode(target.clone(), cursor))
        .transpose()?;
    let mut runs = Vec::with_capacity(page.entries().len());
    for entry in page.entries() {
        runs.push(snapshot(&hydrate_run(app, &target, entry.snapshot()).await?));
    }
    Ok(json!({
        "runs":runs,
        "cursor":next,
        "store":store,
    }))
}

async fn hydrate_run(
    app: &App,
    target: &discovery::NativeTarget,
    value: &ProductRunReferenceSnapshot,
) -> Result<ProductRunSnapshot> {
    let task = read_product_artifact(app, target, value.run_id(), value.task()).await?;
    let status = read_product_artifact(app, target, value.run_id(), value.status()).await?;
    let diff = read_optional_product_artifact(app, target, value.run_id(), value.diff()).await?;
    let gates = read_optional_product_artifact(app, target, value.run_id(), value.gates()).await?;
    let review = read_optional_product_artifact(app, target, value.run_id(), value.review()).await?;
    let summary = read_optional_product_artifact(app, target, value.run_id(), value.summary()).await?;
    let operation = value.operation();
    let operation = ProductRunOperation::new(
        operation.kind(),
        operation.state(),
        read_product_artifact(app, target, value.run_id(), operation.identity()).await?,
        read_product_artifact(app, target, value.run_id(), operation.known()).await?,
        read_optional_product_artifact(app, target, value.run_id(), operation.uncertainty()).await?,
        operation.legal_controls(),
    )
    .map_err(problem)?;
    let mut snapshot = ProductRunSnapshot::new(
        value.run_id(),
        value.workspace_id(),
        value.providers(),
        value.phase(),
        value.cycle(),
        task,
        status,
        diff,
        gates,
        review,
        summary,
        operation,
    )
    .map_err(problem)?;
    if let Some(reference) = value.deliverable() {
        let workspace =
            read_product_artifact(app, target, value.run_id(), reference.workspace_path()).await?;
        let changed_paths = read_product_index(
            app,
            target,
            value.run_id(),
            ProductDeliverableIndexKind::ChangedPaths,
            reference.changed_paths(),
        )
        .await?;
        let successful_commands = read_product_index(
            app,
            target,
            value.run_id(),
            ProductDeliverableIndexKind::SuccessfulCommands,
            reference.successful_commands(),
        )
        .await?;
        let instructions = read_product_artifact(
            app,
            target,
            value.run_id(),
            reference.run_instructions(),
        )
        .await?;
        let mut deliverable = ProductDeliverable::candidate(
            workspace,
            changed_paths,
            successful_commands,
            instructions,
            reference.qualification(),
        )
        .map_err(problem)?;
        if reference.accepted() {
            deliverable = deliverable.mark_accepted();
        }
        if let Some(commit) = reference.commit_revision() {
            deliverable = deliverable
                .mark_committed(
                    read_product_artifact(app, target, value.run_id(), commit).await?,
                )
                .map_err(problem)?;
        }
        if let Some(export) = reference.export_path() {
            deliverable = deliverable
                .mark_exported(
                    read_product_artifact(app, target, value.run_id(), export).await?,
                )
                .map_err(problem)?;
        }
        if reference.discarded() {
            deliverable = deliverable.mark_discarded();
        }
        snapshot = snapshot.with_deliverable(deliverable);
    }
    Ok(snapshot)
}

async fn read_optional_product_artifact(
    app: &App,
    target: &discovery::NativeTarget,
    run: RunId,
    reference: Option<ProductArtifactReference>,
) -> Result<String> {
    match reference {
        Some(reference) => read_product_artifact(app, target, run, reference).await,
        None => Ok(String::new()),
    }
}

async fn read_product_artifact(
    app: &App,
    target: &discovery::NativeTarget,
    run: RunId,
    reference: ProductArtifactReference,
) -> Result<String> {
    let mut bytes = Vec::new();
    let mut offset = 0;
    loop {
        let query = ProductArtifactQuery::new(run, reference, offset).map_err(problem)?;
        let (payload, _) = raw_request_target(
            app,
            target,
            AppRequestPayload::QueryProductArtifact(query),
        )
        .await?;
        let AppResponsePayload::ProductArtifactPage(page) = payload else {
            return Err(problem("Unexpected product artifact response"));
        };
        if page.query() != query {
            return Err(problem("The daemon changed the requested product artifact range"));
        }
        bytes.extend_from_slice(page.bytes());
        let Some(next) = page.next() else { break };
        offset = next;
    }
    if !reference.matches_bytes(&bytes) {
        return Err(problem("The product artifact content changed during retrieval"));
    }
    String::from_utf8(bytes).map_err(|_| problem("The product artifact is not UTF-8"))
}

async fn read_product_index(
    app: &App,
    target: &discovery::NativeTarget,
    run: RunId,
    kind: ProductDeliverableIndexKind,
    index: ProductDeliverableIndexReference,
) -> Result<Vec<String>> {
    let mut values = Vec::new();
    let mut after = None;
    loop {
        let query = ProductDeliverableIndexQuery::new(run, kind, index, after).map_err(problem)?;
        let (payload, _) = raw_request_target(
            app,
            target,
            AppRequestPayload::QueryProductDeliverableIndex(query),
        )
        .await?;
        let AppResponsePayload::ProductDeliverableIndexPage(page) = payload else {
            return Err(problem("Unexpected product deliverable index response"));
        };
        if page.query() != query {
            return Err(problem("The daemon changed the requested product deliverable index"));
        }
        for entry in page.entries() {
            let ordinal = u64::try_from(values.len()).map_err(problem)?;
            if entry.ordinal() != ordinal {
                return Err(problem("The product deliverable index is not contiguous"));
            }
            values.push(read_product_artifact(app, target, run, entry.value()).await?);
        }
        let Some(next) = page.next() else { break };
        after = Some(next);
    }
    if product_deliverable_index_reference(kind, &values).map_err(problem)? != index {
        return Err(problem(
            "The product deliverable index root or count changed during retrieval",
        ));
    }
    Ok(values)
}
