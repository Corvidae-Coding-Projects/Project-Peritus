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

pub(super) const AFTER_EFFECT_REQUESTS: usize = 11;

fn expansion_records(domains_only: bool) -> Value {
    let domains = json!([{
        "id":"domain-local-effect",
        "discipline":"Crash-safe local file effects",
        "applicability":"The requested append must occur exactly once and survive interruption.",
        "expected_sources":["exact user source","repository tool contracts"],
        "risks":["duplicate append after recovery","untracked target mutation"],
        "research_required":true,
        "external_authority_required":false,
        "internal_mechanical":null,
        "research_skip_rationale":null
    }]);
    if domains_only {
        return json!({
            "domains":domains,"evidence":[],"obligations":[],"acceptance_mappings":[],
            "material_question":null,"expanded_markdown":null,"partial_markdown":null,
            "limitations":[],"change_ledger":[]
        });
    }
    json!({
        "domains":domains,
        "evidence":[{
            "id":"evidence-exact-request","domain_ids":["domain-local-effect"],
            "obligation_ids":["obligation-exactly-once"],"title":"Exact user effect contract",
            "version":null,"authority":"source_design",
            "reference":{"kind":"source_range","start":0,"end":10}
        }],
        "obligations":[{
            "id":"obligation-exactly-once","class":"user",
            "statement":"Append the exact input bytes once and preserve the effect across interruption.",
            "origin":"exact user source","rationale":"Duplicate execution would violate the requested effect.",
            "applicability":"The ordinary command and its recovery path.",
            "affected_contracts":["effect-counter.txt","command recovery receipt"],
            "failure_behavior":"Inspect the retained target and do not repeat a settled append.",
            "evidence_ids":["evidence-exact-request"],"acceptance_ids":["acceptance-exactly-once"],
            "status":"supported"
        }],
        "acceptance_mappings":[{
            "id":"acceptance-exactly-once",
            "criterion":"After interruption and retry, effect-counter.txt contains exactly one line equal to effect.",
            "obligation_ids":["obligation-exactly-once"]
        }],
        "material_question":null,"expanded_markdown":null,"partial_markdown":null,
        "limitations":[],
        "change_ledger":[{
            "id":"change-preserve-effect","kind":"preserved",
            "source_evidence_ids":["evidence-exact-request"],
            "obligation_ids":["obligation-exactly-once"],
            "summary":"Preserve the user's exactly-once command and recovery requirement."
        }]
    })
}

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
    let complete = expansion_records(false);
    let scripts = [
        json!({"tool":"workspace_list","arguments":{"path":"."}}),
        json!({"tool":"run_pipeline","arguments":{}}),
        json!({"tool":"design_research_state","arguments":{"action":"domain_assessment","expected_version":1,"records":expansion_records(true),"markdown":null}}),
        json!({"tool":"design_research_state","arguments":{"action":"focused_research","expected_version":2,"records":complete,"markdown":null}}),
        json!({"tool":"design_research_state","arguments":{"action":"expansion_draft","expected_version":3,"records":complete,"markdown":null}}),
        json!({"tool":"design_research_state","arguments":{"action":"expansion_review","expected_version":4,"records":complete,"markdown":null}}),
        json!({"tool":"design_research_state","arguments":{"action":"publish_ready","expected_version":5,"records":complete,"markdown":"# Exact counter effect\n\nCreate effect-counter.txt by one ordinary append command in the disposable workspace. Declare only that file as task scope. Preserve unrelated files and verify that it contains exactly one line, effect. Do not repeat the append after interruption; inspect the retained file first."}}),
        json!({"text":"The source-backed implementation design is published."}),
        json!({"tool":"workspace_scope","arguments":{"paths":["effect-counter.txt"]}}),
        json!({"tool":"run_command","arguments":{"program":"dd","args":["if=effect-input.txt","of=effect-counter.txt","oflag=append","conv=notrunc","status=none"],"purpose":"external_effect","timeout_seconds":5}}),
        // This request is blocked after ResponseStarted, before the worker is killed.
        json!({"text":"The command effect is retained; its current state still needs inspection."}),
        // Exact recovery resumes the retained pipeline and inspects state before another effect.
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
        .ok_or_else(|| {
            format!(
                "no actual host command result at the provider boundary: {}",
                request["messages"]
            )
        })
}

fn verify_completed_command(request: &Value) -> Result<(), String> {
    let result = completed_command(request)?;
    if result["success"] != true || result["exit_code"] != 0 || result["timed_out"] != false {
        return Err(format!("counter exists without successful ordinary command result: {result}"));
    }
    let encoded = request.to_string();
    for required in [
        "Accepted implementation design identity",
        "source_sha256=",
        "manifest_sha256=",
        "expanded_sha256=",
        "task_sha256=",
        "evidence_sha256=",
        "input_revision=",
        "policy=",
        "# Exact counter effect",
    ] {
        if !encoded.contains(required) {
            return Err(format!(
                "writer provider boundary omitted expanded-contract evidence {required:?}",
            ));
        }
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
