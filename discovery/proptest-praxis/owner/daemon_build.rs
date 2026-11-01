//! Ordinary direct-folder pipeline handoff for a real, disposable command effect.
//!
//! The recording provider must use a distinct stable tool-call identity for each request.

use super::{fixture, provider};
use fixture::Fixture;
use peritus_app_protocol::{
    ConversationTitle, ProductInteractionMode, ProductProviderSelection,
    WorkbenchExecutionSettings, WorkbenchIntent,
};
use serde_json::{Value, json};
use std::fs;

pub(super) const AFTER_EFFECT_REQUESTS: usize = 6;

/// Reaches a fresh provider boundary after one real command effect through the normal pipeline.
pub(super) async fn prepare(fixture: &mut Fixture) -> Result<usize, String> {
    configure(fixture)?;
    begin(fixture).await?;
    reach_effect(fixture).await
}

/// Installs provider outputs only; the real host still owns grounding, effects, and receipts.
pub(super) fn configure(fixture: &Fixture) -> Result<(), String> {
    // This is an immutable input to the real command, never the expected output artifact.
    fs::write(fixture.root.join("workspace/effect-input.txt"), b"effect\n")
        .map_err(|error| error.to_string())?;
    let scripts = [
        json!({"tool":"workspace_list","arguments":{"path":"."}}),
        json!({"tool":"run_pipeline","arguments":{}}),
        json!({"text":"# Exact counter effect\n\nCreate effect-counter.txt by one ordinary append command in the disposable workspace. Declare only that file as task scope. Preserve unrelated files and verify that it contains exactly one line, effect. Do not repeat the append after interruption; inspect the retained file first."}),
        json!({"tool":"workspace_scope","arguments":{"paths":["effect-counter.txt"]}}),
        json!({"tool":"run_command","arguments":{"program":"dd","args":["if=effect-input.txt","of=effect-counter.txt","oflag=append","conv=notrunc","status=none"],"purpose":"external_effect","timeout_seconds":5}}),
        // This request is blocked after ResponseStarted, before the worker is killed.
        json!({"text":"The command effect is retained; its current state still needs inspection."}),
        // Explicit recovery enters the ordinary conversational route and inspects retained state.
        json!({"tool":"workspace_read","arguments":{"path":"effect-counter.txt"}}),
        json!({"text":"The retained counter contains exactly one effect line."}),
    ];
    for (turn, script) in scripts.iter().enumerate() {
        fixture.script(u64::try_from(turn).map_err(|error| error.to_string())?, script)?;
    }
    Ok(())
}

/// Direct folders reject Build mode; Chat's ordinary `run_pipeline` tool supplies the capability.
pub(super) async fn begin(fixture: &mut Fixture) -> Result<(), String> {
    fixture
        .command(WorkbenchIntent::CreateConversation(
            ConversationTitle::new("Authorized disposable counter".into())
                .map_err(|error| format!("{error:?}"))?,
        ))
        .await?;
    fixture
        .enqueue(
            81,
            "Implement this change in the disposable workspace: create effect-counter.txt by appending exactly once the complete bytes of the provided effect-input.txt, which contains one line containing effect. Use the ordinary command tool with literal executable and argv. Preserve that effect across interruption; after an interruption inspect the existing counter instead of appending it again. Do not change other files.",
        )
        .await?;
    let profile = provider::profile("fixture-a")?.profile_id();
    fixture
        .command(WorkbenchIntent::StartExecution(WorkbenchExecutionSettings::new(
            fixture::run(),
            ProductProviderSelection::new(profile, profile, profile),
            ProductInteractionMode::Chat,
            fixture::models("fixture-a", false)?,
        )))
        .await?;
    Ok(())
}

/// Stops at a real provider boundary following successful ordinary host command completion.
pub(super) fn reach_effect(
    fixture: &Fixture,
) -> impl Future<Output = Result<usize, String>> + Send + 'static {
    let root = fixture.root.clone();
    let waits =
        (1..=AFTER_EFFECT_REQUESTS).map(|count| fixture.wait_requests(count)).collect::<Vec<_>>();
    async move {
        for (turn, wait) in waits.into_iter().enumerate() {
            let requests = wait.await?;
            if root.join("workspace/effect-counter.txt").exists() {
                verify_counter_at(&root)?;
                verify_completed_command(requests.last().ok_or("missing provider boundary")?)?;
                return Ok(turn + 1);
            }
            if turn == AFTER_EFFECT_REQUESTS - 1 {
                return Err(format!(
                    "no real effect at provider boundary: {}",
                    completed_command(requests.last().ok_or("missing provider boundary")?)?
                ));
            }
            fs::write(root.join(format!("release-{turn}")), b"release")
                .map_err(|e| e.to_string())?;
        }
        Err("effect setup exhausted its bounded provider schedule".into())
    }
}

fn completed_command(request: &Value) -> Result<Value, String> {
    request["messages"]
        .as_array()
        .ok_or("recorded provider messages missing")?
        .iter()
        .rev()
        .filter_map(Value::as_str)
        .filter_map(|message| serde_json::from_str::<Value>(message).ok())
        .find(|value| value.get("handle").is_some() && value.get("success").is_some())
        .ok_or_else(|| "no actual host command result at the provider boundary".into())
}

fn verify_completed_command(request: &Value) -> Result<(), String> {
    let result = completed_command(request)?;
    if result["success"] != true || result["exit_code"] != 0 || result["timed_out"] != false {
        return Err(format!("counter exists without successful ordinary command result: {result}"));
    }
    Ok(())
}

/// Releases the fresh recovery read and its response, without rerunning the effect command.
pub(super) async fn finish_retry(fixture: &mut Fixture) -> Result<(), String> {
    fixture.wait_requests(AFTER_EFFECT_REQUESTS + 1).await?;
    fixture.release(u64::try_from(AFTER_EFFECT_REQUESTS).map_err(|error| error.to_string())?)?;
    fixture.wait_requests(AFTER_EFFECT_REQUESTS + 2).await?;
    fixture
        .release(u64::try_from(AFTER_EFFECT_REQUESTS + 1).map_err(|error| error.to_string())?)?;
    fixture.wait_phase(peritus_app_protocol::ProductRunPhase::WaitingForUser).await?;
    verify_counter(fixture)
}

pub(super) fn verify_counter(fixture: &Fixture) -> Result<(), String> {
    verify_counter_at(&fixture.root)
}
fn verify_counter_at(root: &std::path::Path) -> Result<(), String> {
    let source = fs::read(root.join("workspace/effect-input.txt"))
        .map_err(|error| format!("fixture command input absent: {error}"))?;
    if source != b"effect\n" {
        return Err("ordinary command changed its immutable fixture input".into());
    }
    let bytes = fs::read(root.join("workspace/effect-counter.txt"))
        .map_err(|error| format!("ordinary command effect absent: {error}"))?;
    if bytes != b"effect\n" {
        return Err(format!("expected one exact command effect, found {bytes:?}"));
    }
    Ok(())
}
