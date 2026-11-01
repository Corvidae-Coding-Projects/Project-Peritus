use super::{
    Case, Scenario,
    fixture::{self, Fixture},
};
use peritus_app_protocol::*;
use serde_json::json;
use std::{collections::BTreeMap, fs};
fn bump(m: &mut BTreeMap<String, u64>, key: &str) {
    *m.entry(key.into()).or_default() += 1;
}
pub(super) async fn check(case: &Case) -> Result<BTreeMap<String, u64>, String> {
    let mut f = Fixture::new(case).await?;
    let result = execute(&mut f, case).await;
    if result.is_err() {
        let _ = f.kill();
        if let Some(output) = std::env::var_os("PRAXIS_OUTPUT") {
            let folder = std::path::Path::new(&output).join("first-daemon-failure-evidence");
            if fs::create_dir(&folder).is_ok() {
                for name in [
                    "case.json",
                    "requests.jsonl",
                    "child.log",
                    "ready.json",
                    "committed-reply.json",
                ] {
                    let _ = fs::copy(f.root.join(name), folder.join(name));
                }
            }
        }
    }
    result
}
async fn execute(f: &mut Fixture, case: &Case) -> Result<BTreeMap<String, u64>, String> {
    let mut m = BTreeMap::new();
    bump(&mut m, "real_daemon_sessions");
    if case.scenario == Scenario::SteerAtFinalization {
        late_finalization(f, case, &mut m).await?;
        bump(&mut m, "scenario_SteerAtFinalization");
        return Ok(m);
    }
    if case.scenario == Scenario::CrashAfterEffect {
        super::build::prepare(f).await?;
        crash(f, case, &mut m).await?;
        bump(&mut m, "scenario_CrashAfterEffect");
        return Ok(m);
    }
    if case.scenario == Scenario::SteerBeforeTool {
        f.script(0, &json!({"tool":"workspace_list","arguments":{"path":"."}}))?;
    } else {
        f.script(0, &json!({"text":"initial answer"}))?;
    }
    if case.scenario == Scenario::SteerBeforeTool {
        fs::write(f.root.join("enable-admission-barrier"), b"once").map_err(|e| e.to_string())?;
    }
    f.begin().await?;
    let rows = f.wait_requests(1).await?;
    if rows[0]["model"] != "fixture-a" || rows[0]["effort"] != "low" {
        return Err(format!("initial explicit model/effort not retained: {}", rows[0]));
    }
    match case.scenario {
        Scenario::SteerAtFinalization => return Err("fixture scenario routing failed".into()),
        Scenario::SteerBeforeTool | Scenario::SteerBeforeFinal => steer(f, case, &mut m).await?,
        Scenario::ModelChange => model_change(f, &mut m).await?,
        Scenario::CancelRetry => cancel_retry(f, &mut m).await?,
        Scenario::CrashProvider | Scenario::CrashAfterEffect => crash(f, case, &mut m).await?,
        Scenario::IdleNewRun => idle_new(f, &mut m).await?,
        Scenario::UnsupportedModel => unsupported(f, &mut m).await?,
    }
    bump(&mut m, &format!("scenario_{:?}", case.scenario));
    Ok(m)
}
async fn steer(f: &mut Fixture, case: &Case, m: &mut BTreeMap<String, u64>) -> Result<(), String> {
    if case.scenario == Scenario::SteerBeforeTool {
        f.release(0)?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !f.root.join("admission-ready.json").exists() {
            if std::time::Instant::now() > deadline {
                return Err("did not reach actual host tool admission".into());
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        bump(m, "actual_host_admission_barriers");
    }
    let receipt = f.enqueue(83, &case.text).await?;
    f.enqueue(84, "HELD_INPUT_SENTINEL").await?;
    f.command(WorkbenchIntent::Queue(WorkbenchQueueIntent::Hold {
        selected: WorkbenchInputSelection::new(
            WorkbenchInputId::new([84; 16]).map_err(|e| format!("{e:?}"))?,
            1,
        )
        .map_err(|e| e.to_string())?,
        held: true,
    }))
    .await?;
    f.enqueue(85, "WITHDRAWN_INPUT_SENTINEL").await?;
    f.command(WorkbenchIntent::Queue(WorkbenchQueueIntent::Withdraw(
        WorkbenchInputSelection::new(
            WorkbenchInputId::new([85; 16]).map_err(|e| format!("{e:?}"))?,
            1,
        )
        .map_err(|e| e.to_string())?,
    )))
    .await?;
    bump(m, "held_inputs_excluded");
    bump(m, "withdrawn_inputs_excluded");

    let accepted = f.interaction().await?;
    if accepted.received() <= accepted.incorporated() {
        return Err(
            "new accepted input was falsely marked incorporated while provider is blocked".into()
        );
    }
    let mut text = case.text.clone();
    let mut revision = 1;
    for n in 0..case.edits {
        text = format!("{} changed {n}", case.text);
        f.command(WorkbenchIntent::Queue(WorkbenchQueueIntent::Edit {
            selected: WorkbenchInputSelection::new(
                WorkbenchInputId::new([83; 16]).map_err(|e| format!("{e:?}"))?,
                revision,
            )
            .map_err(|e| format!("{e:?}"))?,
            text: WorkbenchInputText::new(text.clone()).map_err(|e| format!("{e:?}"))?,
        }))
        .await?;
        revision += 1;
        bump(m, "pending_edits");
    }
    if case.reconnect {
        f.reconnect().await?;
        bump(m, "fresh_ipc_connections");
    }
    if receipt.accepted_revision() == 0 {
        return Err("acceptance missing durable revision".into());
    }
    f.release(0)?;
    if case.scenario == Scenario::SteerBeforeTool {
        fs::write(f.root.join("release-admission"), b"release").map_err(|e| e.to_string())?;
    }
    let requests = f.wait_requests(2).await?;
    if !requests[1]["messages"].as_array().is_some_and(|messages| {
        messages.iter().any(|m| m.as_str().is_some_and(|s| s.contains(&text)))
    }) {
        return Err("subsequent actual request lost accepted latest input".into());
    }
    if requests[1]["messages"].to_string().contains("HELD_INPUT_SENTINEL")
        || requests[1]["messages"].to_string().contains("WITHDRAWN_INPUT_SENTINEL")
    {
        return Err("held/withdrawn input reached actual request".into());
    }
    f.release(1)?;
    let done = f.wait_phase(ProductRunPhase::WaitingForUser).await?;
    verify_committed_reply(f, m).await?;
    if done.received() != done.incorporated() {
        return Err("accepted input left unaccounted at idle finalization".into());
    }
    if case.scenario == Scenario::SteerBeforeTool
        && done.activities().iter().any(|a| a.kind() == ProductActivityKind::Tool)
    {
        return Err("stale tool reached ordinary public execution activity".into());
    }
    let page = f
        .request(AppRequestPayload::QueryWorkbenchQueue(
            WorkbenchQueueQuery::new(fixture::query(), 0, 0, true).map_err(|e| format!("{e:?}"))?,
        ))
        .await?;
    let AppResponsePayload::WorkbenchQueue(page) = page else {
        return Err("queue inspection response".into());
    };
    if !page.rows().iter().any(|r| {
        r.selected().id().into_bytes() == [83; 16]
            && r.text().as_str() == text
            && r.selected().revision() == revision
            && r.state() == WorkbenchInputState::Incorporated
    }) {
        return Err("exact latest input revision not durably incorporated".into());
    }
    if !page.rows().iter().any(|r| {
        r.selected().id().into_bytes() == [84; 16] && r.state() == WorkbenchInputState::Held
    }) || !page.rows().iter().any(|r| {
        r.selected().id().into_bytes() == [85; 16] && r.state() == WorkbenchInputState::Withdrawn
    }) {
        return Err("held or withdrawn state was not conserved".into());
    }
    f.command(WorkbenchIntent::Queue(WorkbenchQueueIntent::Hold {
        selected: WorkbenchInputSelection::new(
            WorkbenchInputId::new([84; 16]).map_err(|e| format!("{e:?}"))?,
            1,
        )
        .map_err(|e| e.to_string())?,
        held: false,
    }))
    .await?;
    f.request(AppRequestPayload::ContinueWorkbenchExecution(WorkbenchContinuation::new(
        fixture::query(),
        ProductInteractionMode::Chat,
    )))
    .await?;
    let rows = f.wait_requests(3).await?;
    if !rows[2]["messages"].to_string().contains("HELD_INPUT_SENTINEL")
        || rows[2]["messages"].to_string().contains("WITHDRAWN_INPUT_SENTINEL")
    {
        return Err("unhold/withdraw visibility on next request is incorrect".into());
    }
    f.release(2)?;
    f.wait_phase(ProductRunPhase::WaitingForUser).await?;
    bump(m, "unheld_inputs_incorporated");
    bump(m, "steering_dispatch_schedules");
    Ok(())
}
async fn model_change(f: &mut Fixture, m: &mut BTreeMap<String, u64>) -> Result<(), String> {
    let updated = f
        .request(AppRequestPayload::UpdateModels(ProductModelUpdate::new(
            fixture::run(),
            fixture::models("fixture-b", true)?,
        )))
        .await?;
    if !matches!(updated, AppResponsePayload::Interaction(_)) {
        return Err(format!("model update rejected: {updated:?}"));
    }
    let active_text = "Apply steering with the new selected model in this active invocation.";
    let receipt = f.enqueue(84, active_text).await?;
    let pending = f.interaction().await?;
    if receipt.accepted_revision() == 0 || pending.received() <= pending.incorporated() {
        return Err("model-change steering lacked a durable pending acceptance".into());
    }
    f.release(0)?;
    let requests = f.wait_requests(2).await?;
    if requests[0]["model"] != "fixture-a"
        || requests[1]["model"] != "fixture-b"
        || requests[0]["effort"] != "low"
        || requests[1]["effort"] != "high"
        || invocation_prefix(&requests[0])? != invocation_prefix(&requests[1])?
        || !requests[1]["messages"].as_array().is_some_and(|messages| {
            messages.iter().any(|message| message.as_str().is_some_and(|s| s.contains(active_text)))
        })
    {
        return Err("same-invocation immutable model/effort boundary violated".into());
    }
    f.release(1)?;
    f.wait_phase(ProductRunPhase::WaitingForUser).await?;
    input_accounted(f, 84, active_text).await?;
    verify_committed_reply(f, m).await?;
    f.request(AppRequestPayload::UpdateModels(ProductModelUpdate::new(
        fixture::run(),
        fixture::models("fixture-a", false)?,
    )))
    .await?;
    let idle_text = "A later idle continuation retains the next selected model.";
    f.enqueue(88, idle_text).await?;
    f.request(AppRequestPayload::ContinueWorkbenchExecution(WorkbenchContinuation::new(
        fixture::query(),
        ProductInteractionMode::Chat,
    )))
    .await?;
    let rows = f.wait_requests(3).await?;
    if rows[2]["model"] != "fixture-a"
        || rows[2]["effort"] != "low"
        || !rows[2]["messages"].as_array().is_some_and(|messages| {
            messages.iter().any(|message| message.as_str().is_some_and(|s| s.contains(idle_text)))
        })
    {
        return Err("idle continuation model selection was substituted".into());
    }
    f.release(2)?;
    f.wait_phase(ProductRunPhase::WaitingForUser).await?;
    input_accounted(f, 88, idle_text).await?;
    bump(m, "active_model_update_boundaries");
    bump(m, "idle_model_update_boundaries");
    Ok(())
}

fn invocation_prefix(row: &serde_json::Value) -> Result<&str, String> {
    let (head, attempt) = row["request_id"]
        .as_str()
        .and_then(|id| id.rsplit_once("-attempt-"))
        .ok_or("request identity missing attempt suffix")?;
    let (prefix, turn) = head.rsplit_once('-').ok_or("request identity missing turn suffix")?;
    attempt.parse::<u8>().map_err(|error| error.to_string())?;
    turn.parse::<u16>().map_err(|error| error.to_string())?;
    Ok(prefix)
}

async fn input_accounted(f: &mut Fixture, id: u8, text: &str) -> Result<(), String> {
    let interaction = f.interaction().await?;
    if interaction.received() != interaction.incorporated() {
        return Err("model-change steering remained unaccounted at idle".into());
    }
    let response = f
        .request(AppRequestPayload::QueryWorkbenchQueue(
            WorkbenchQueueQuery::new(fixture::query(), 0, 0, true).map_err(|e| e.to_string())?,
        ))
        .await?;
    let AppResponsePayload::WorkbenchQueue(page) = response else {
        return Err("model-change queue inspection response".into());
    };
    if !page.rows().iter().any(|row| {
        row.selected().id().into_bytes() == [id; 16]
            && row.selected().revision() == 1
            && row.text().as_str() == text
            && row.state() == WorkbenchInputState::Incorporated
    }) {
        return Err("model-change steering lost its exact incorporated queue revision".into());
    }
    Ok(())
}
async fn cancel_retry(f: &mut Fixture, m: &mut BTreeMap<String, u64>) -> Result<(), String> {
    f.request(AppRequestPayload::ControlProductRun(ProductRunControl::new(
        fixture::run(),
        ProductRunControlAction::Cancel,
    )))
    .await?;
    f.wait_phase(ProductRunPhase::Cancelled).await?;
    f.request(AppRequestPayload::ControlProductRun(ProductRunControl::new(
        fixture::run(),
        ProductRunControlAction::Retry,
    )))
    .await?;
    f.wait_requests(2).await?;
    f.release(1)?;
    f.wait_phase(ProductRunPhase::WaitingForUser).await?;
    bump(m, "cancel_retry_boundaries");
    Ok(())
}
async fn crash(f: &mut Fixture, case: &Case, m: &mut BTreeMap<String, u64>) -> Result<(), String> {
    let requests = if case.scenario == Scenario::CrashAfterEffect {
        super::build::AFTER_EFFECT_REQUESTS
    } else {
        1
    };
    f.kill()?;
    bump(m, "actual_daemon_kills");
    f.start().await?;
    let restored = f.interaction().await?;
    if restored.snapshot().phase() != ProductRunPhase::RecoveryRequired {
        return Err(format!(
            "crash recovery falsely terminal/current: {:?}",
            restored.snapshot().phase()
        ));
    }
    if f.requests()?.len() != requests {
        return Err("restart automatically repeated provider work".into());
    }
    f.request(AppRequestPayload::ControlProductRun(ProductRunControl::new(
        fixture::run(),
        ProductRunControlAction::Retry,
    )))
    .await?;
    f.wait_requests(requests + 1).await?;
    if case.scenario == Scenario::CrashAfterEffect {
        super::build::finish_retry(f).await?;
        bump(m, "retained_ordinary_command_effects");
    } else {
        f.release(u64::try_from(requests).map_err(|e| e.to_string())?)?;
        f.wait_phase(ProductRunPhase::WaitingForUser).await?;
    }
    bump(m, "explicit_recovery_retries");
    Ok(())
}
async fn idle_new(f: &mut Fixture, m: &mut BTreeMap<String, u64>) -> Result<(), String> {
    f.release(0)?;
    f.wait_phase(ProductRunPhase::WaitingForUser).await?;
    let query = WorkbenchQuery::new(
        ConversationId::new([85; 16]).map_err(|e| format!("{e:?}"))?,
        fixture::workspace(),
    );
    let id = super::provider::profile("fixture-a")?.profile_id();
    let inputs = [
        WorkbenchIntent::CreateConversation(
            ConversationTitle::new("second conversation".into()).map_err(|e| format!("{e:?}"))?,
        ),
        WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(
            WorkbenchNewInput::new(
                WorkbenchInputId::new([86; 16]).map_err(|e| format!("{e:?}"))?,
                WorkbenchInputText::new("second fresh task".into())
                    .map_err(|e| format!("{e:?}"))?,
                WorkbenchInputOrder::new(Vec::new()).map_err(|e| format!("{e:?}"))?,
            )
            .map_err(|e| format!("{e:?}"))?,
        )),
        WorkbenchIntent::StartExecution(WorkbenchExecutionSettings::new(
            peritus_types::RunId::new([87; 16]).map_err(|e| format!("{e:?}"))?,
            ProductProviderSelection::new(id, id, id),
            ProductInteractionMode::Chat,
            fixture::models("fixture-a", false)?,
        )),
    ];
    for (n, intent) in inputs.into_iter().enumerate() {
        let response = f
            .request(AppRequestPayload::WorkbenchCommand(WorkbenchCommand::new(
                ControlOperationId::new([90 + u8::try_from(n).map_err(|e| format!("{e:?}"))?; 16])
                    .map_err(|e| format!("{e:?}"))?,
                query,
                n as u64,
                intent,
            )))
            .await?;
        if !matches!(response, AppResponsePayload::WorkbenchReceipt(_)) {
            return Err(format!("idle run blocked fresh same-workspace work: {response:?}"));
        }
    }
    f.wait_requests(2).await?;
    f.release(1)?;
    bump(m, "idle_workspace_reuse");
    Ok(())
}
async fn unsupported(f: &mut Fixture, m: &mut BTreeMap<String, u64>) -> Result<(), String> {
    let response = f
        .request(AppRequestPayload::UpdateModels(ProductModelUpdate::new(
            fixture::run(),
            fixture::models("unsupported-model", true)?,
        )))
        .await?;
    if !matches!(response, AppResponsePayload::Error(_)) {
        return Err("unsupported selection did not return an actionable error".into());
    }
    f.release(0)?;
    f.wait_phase(ProductRunPhase::WaitingForUser).await?;
    if f.requests()?.len() != 1 {
        return Err("unsupported model update started substitute inference".into());
    }
    if f.interaction().await?.models() != &fixture::models("fixture-a", false)? {
        return Err("failed choice mutated durable model selections".into());
    }
    f.enqueue(89, "Continue after rejected selection with the previous model.").await?;
    f.request(AppRequestPayload::ContinueWorkbenchExecution(WorkbenchContinuation::new(
        fixture::query(),
        ProductInteractionMode::Chat,
    )))
    .await?;
    let prior = f.wait_requests(2).await?;
    if prior[1]["model"] != "fixture-a" || prior[1]["effort"] != "low" {
        return Err("invalid selection changed subsequent request model or effort".into());
    }
    f.release(1)?;
    f.wait_phase(ProductRunPhase::WaitingForUser).await?;
    let defaults = f
        .request(AppRequestPayload::UpdateModels(ProductModelUpdate::new(
            fixture::run(),
            ProductRoleModels::default(),
        )))
        .await?;
    if !matches!(defaults, AppResponsePayload::Interaction(_)) {
        return Err("documented configured-model default was rejected".into());
    }
    f.enqueue(90, "Configured profile choice remains explicit provider authority.").await?;
    f.request(AppRequestPayload::ContinueWorkbenchExecution(WorkbenchContinuation::new(
        fixture::query(),
        ProductInteractionMode::Chat,
    )))
    .await?;
    let configured = f.wait_requests(3).await?;
    if configured[2]["model"] != "fixture-a" {
        return Err("configured profile default was substituted".into());
    }
    f.release(2)?;
    f.wait_phase(ProductRunPhase::WaitingForUser).await?;
    bump(m, "configured_profile_defaults");
    bump(m, "unsupported_choices_rejected");
    Ok(())
}

async fn verify_committed_reply(
    f: &mut Fixture,
    m: &mut BTreeMap<String, u64>,
) -> Result<(), String> {
    let response = f
        .request(AppRequestPayload::QueryWorkbenchBriefPage(
            WorkbenchBriefPageRequest::new(fixture::query(), 0, 0, 0).map_err(|e| e.to_string())?,
        ))
        .await?;
    let AppResponsePayload::WorkbenchBriefPage(page) = response else {
        return Err(format!("brief reply page: {response:?}"));
    };
    if page.proposal_total() != 1 || page.proposals().len() != 1 {
        return Err(format!(
            "expected only final current reply, saw {} immutable replies",
            page.proposal_total()
        ));
    }
    let reference = page.proposals()[0];
    let response = f
        .request(AppRequestPayload::QueryWorkbenchBriefProposal(
            WorkbenchBriefProposalRequest::new(fixture::query(), page.revision(), reference, 0)
                .map_err(|e| e.to_string())?,
        ))
        .await?;
    let AppResponsePayload::WorkbenchBriefProposal(body) = response else {
        return Err(format!("exact reply body: {response:?}"));
    };
    fixture::save(
        &f.root.join("committed-reply.json"),
        &json!({"total":page.proposal_total(),"text":body.text(),"source_bytes":reference.bytes(),"digest":fixture::hex(reference.digest().as_bytes())}),
    )?;
    if body.next().is_some()
        || body.text() != "done"
        || u64::try_from(body.text().len()).map_err(|e| e.to_string())? != reference.bytes()
        || peritus_codec::sha256(body.text().as_bytes()) != reference.digest()
    {
        return Err("stale or inexact terminal reply was committed".into());
    }
    bump(m, "exact_committed_current_replies");
    Ok(())
}

async fn late_finalization(
    f: &mut Fixture,
    case: &Case,
    m: &mut BTreeMap<String, u64>,
) -> Result<(), String> {
    f.script(0, &json!({"text":"initial answer"}))?;
    fs::write(f.root.join("enable-finalization-barrier"), b"once").map_err(|e| e.to_string())?;
    f.begin().await?;
    f.wait_requests(1).await?;
    f.release(0)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !f.root.join("finalization-ready.json").exists() {
        if std::time::Instant::now() > deadline {
            return Err("did not reach actual final publication boundary".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    f.enqueue(83, &case.text).await?;
    let accepted = f.interaction().await?;
    if accepted.received() <= accepted.incorporated() {
        return Err("late accepted input prematurely incorporated".into());
    }
    fs::write(f.root.join("release-finalization"), b"release").map_err(|e| e.to_string())?;
    let rows = f.wait_requests(2).await?;
    if !rows[1]["messages"].as_array().is_some_and(|messages| {
        messages.iter().any(|v| v.as_str().is_some_and(|t| t.contains(&case.text)))
    }) {
        return Err("late accepted input was lost between finish and automatic retry".into());
    }
    f.release(1)?;
    let idle = f.wait_phase(ProductRunPhase::WaitingForUser).await?;
    if idle.received() != idle.incorporated() {
        return Err("late input remained unaccounted after automatic continuation".into());
    }
    bump(m, "after_runner_finalization_barriers");
    bump(m, "automatic_accepted_input_retries");
    Ok(())
}
