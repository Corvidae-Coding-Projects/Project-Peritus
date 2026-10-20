//! Agent-loop consumer for the provider retry and rejected-reply narration.
//!
//! It drives the real developer loop with the real OpenCode adapter against the scripted vendor
//! stand-in and records the public activities the loop publishes. One logical turn must announce
//! its model request once; a bounded retry and each refused reply report their own transition.
use peritus_agent::{
    DeveloperActivity, DeveloperControlFlow, DeveloperInput, DeveloperInteraction, DeveloperLoop,
    DeveloperLoopError, DeveloperLoopLimits, DeveloperLoopRequest, DeveloperModelRole,
    DeveloperRequestAdmission, DeveloperToolEffect, DeveloperToolExecutor, DeveloperToolObservation,
    DeveloperTrace, DeveloperTraceEvent,
};
use peritus_daemon::{ProviderRegistry, ProviderRegistryLimits, ProviderRoute};
use peritus_model_protocol::{CompletedToolCall, ModelRequest, UsageCounters};
use peritus_provider_core::{
    CancellationToken, Credential, CredentialReference, CredentialSource, ModelProvider,
    ProviderCoreError,
};
use serde_json::json;
use std::{error::Error, sync::{Arc, Mutex}};
type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct LocalCredential;

impl CredentialSource for LocalCredential {
    fn resolve(
        &self,
        reference: &CredentialReference,
    ) -> std::result::Result<Credential, ProviderCoreError> {
        let _reference = reference;
        let key = std::fs::read(
            std::env::var("PERITUS_REAL_KEY_FILE")
                .map_err(|_| ProviderCoreError::credential("key file not selected"))?,
        )
        .map_err(|_| ProviderCoreError::credential("test credential unavailable"))?;
        let key = std::str::from_utf8(&key)
            .map_err(|_| ProviderCoreError::credential("test credential encoding"))?
            .trim();
        Credential::new(key.as_bytes().to_vec())
    }
}

fn provider(value: serde_json::Value) -> Result<Arc<dyn ModelProvider>> {
    let route: ProviderRoute = serde_json::from_value(value)?;
    let declaration = route.declaration()?;
    let id = declaration.profile().profile_id();
    let registry = ProviderRegistry::build(
        vec![declaration],
        ProviderRegistryLimits::PRODUCTION,
        Some(Arc::new(LocalCredential)),
    )?;
    Ok(registry.current_provider(id).ok_or("no provider")?)
}

#[derive(Default)]
struct Activities {
    observed: Mutex<Vec<String>>,
}

impl Activities {
    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.observed.lock().expect("activity lock"))
    }
}

impl DeveloperInteraction for Activities {
    fn provider(
        &self,
        _role: DeveloperModelRole,
    ) -> std::result::Result<Option<Arc<dyn ModelProvider>>, DeveloperLoopError> {
        Ok(None)
    }

    fn input(&self) -> std::result::Result<DeveloperInput, DeveloperLoopError> {
        Ok(DeveloperInput {
            revision: 1,
            conversation: "Reply with the scripted vendor turn.".to_owned(),
            images: Vec::new(),
        })
    }

    fn prepare_request(
        &self,
        _revision: u64,
        _request: &ModelRequest,
    ) -> std::result::Result<DeveloperRequestAdmission, DeveloperLoopError> {
        Ok(DeveloperRequestAdmission::Accepted)
    }

    fn prepare_role_request(
        &self,
        _role: DeveloperModelRole,
        _revision: u64,
        _request: &ModelRequest,
    ) -> std::result::Result<DeveloperRequestAdmission, DeveloperLoopError> {
        Ok(DeveloperRequestAdmission::Accepted)
    }

    fn complete_role_request(
        &self,
        _role: DeveloperModelRole,
        _request_id: &str,
        _usage: UsageCounters,
    ) -> std::result::Result<DeveloperControlFlow, DeveloperLoopError> {
        Ok(DeveloperControlFlow::Continue)
    }

    fn admit_tool(
        &self,
        _role: DeveloperModelRole,
        _invocation: &str,
        _sequence: u32,
        _input_revision: u64,
        _effect: DeveloperToolEffect,
    ) -> std::result::Result<DeveloperControlFlow, DeveloperLoopError> {
        Ok(DeveloperControlFlow::Continue)
    }

    fn complete_tool(
        &self,
        _role: DeveloperModelRole,
        _invocation: &str,
        _sequence: u32,
    ) -> std::result::Result<DeveloperControlFlow, DeveloperLoopError> {
        Ok(DeveloperControlFlow::Continue)
    }

    fn observe(
        &self,
        activity: DeveloperActivity<'_>,
    ) -> std::result::Result<(), DeveloperLoopError> {
        let line = match activity {
            DeveloperActivity::ModelStarted { model, .. } => format!("ModelStarted {model}"),
            DeveloperActivity::ProviderRetry { next_attempt, max_attempts, reason } => {
                format!("ProviderRetry {next_attempt}/{max_attempts} {}", reason.as_str())
            }
            DeveloperActivity::ResponseRejected { attempt, reason } => {
                format!("ResponseRejected {attempt} {reason}")
            }
            DeveloperActivity::ResponseHealed => "ResponseHealed".to_owned(),
            DeveloperActivity::ModelWaiting { .. } => return Ok(()),
            DeveloperActivity::ReviewRetry { next_attempt, .. } => {
                format!("ReviewRetry {next_attempt}")
            }
            DeveloperActivity::Text(bytes) => format!("Text {} bytes", bytes.len()),
            DeveloperActivity::ReasoningSummary(bytes) => {
                format!("ReasoningSummary {} bytes", bytes.len())
            }
            DeveloperActivity::ToolStarted { name, .. } => format!("ToolStarted {name}"),
            DeveloperActivity::ToolFinished { name, .. } => format!("ToolFinished {name}"),
            DeveloperActivity::ToolSkipped { name } => format!("ToolSkipped {name}"),
        };
        self.observed.lock().expect("activity lock").push(line);
        Ok(())
    }

    fn allows_semantic_compaction(&self) -> bool {
        false
    }
}

struct NoTools;

impl DeveloperToolExecutor for NoTools {
    fn execute(
        &mut self,
        _call: &CompletedToolCall,
    ) -> std::result::Result<DeveloperToolObservation, DeveloperLoopError> {
        Err(DeveloperLoopError::Tool("this replay declares no application tools".to_owned()))
    }
}

struct NoTrace;

impl DeveloperTrace for NoTrace {
    fn record(
        &mut self,
        _event: DeveloperTraceEvent<'_>,
    ) -> std::result::Result<(), DeveloperLoopError> {
        Ok(())
    }
}

fn count(observed: &[String], prefix: &str) -> usize {
    observed.iter().filter(|line| line.starts_with(prefix)).count()
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let arguments = std::env::args().collect::<Vec<_>>();
    let expectation = arguments.get(1).ok_or("expectation needed (retry|rejected)")?;
    let config: toml::Value =
        toml::from_str(&std::fs::read_to_string(std::env::var("PERITUS_REAL_CONFIG")?)?)?;
    let selected = config["providers"]
        .as_array()
        .ok_or("provider array missing")?
        .iter()
        .find(|route| {
            route
                .get("hosted_service")
                .and_then(toml::Value::as_str)
                .is_some_and(|service| matches!(service, "opencode-zen" | "opencode-go"))
        })
        .ok_or("configuration has no saved OpenCode route")?;
    let mut route = serde_json::to_value(selected)?;
    route["profile"]["model"] = json!("scripted-reply-model");
    route["executable"] = json!(std::env::var("PERITUS_REAL_CAPTURE")?);
    route["profile"]["capabilities"] =
        json!(["parallel-tool-calls", "streaming", "tool-calls", "usage-detail"]);
    let provider = provider(route)?;
    let request = DeveloperLoopRequest {
        request_prefix: "scripted-agent-narration".to_owned(),
        system: "You are the scripted replay role. Never invoke provider-native tools.".to_owned(),
        prompt: "Reply with the scripted vendor turn.".to_owned(),
        attachments: Vec::new(),
        tools: Vec::new(),
        limits: DeveloperLoopLimits::new(4, 8)?,
        cancellation: CancellationToken::new(),
    };
    let activities = Activities::default();
    let outcome = DeveloperLoop::run_interactive(
        provider.as_ref(),
        request,
        &mut NoTools,
        &mut NoTrace,
        None,
        &activities,
    )
    .await;
    let observed = activities.take();
    for line in &observed {
        println!("ACTIVITY {line}");
    }
    let started = count(&observed, "ModelStarted");
    let retries = count(&observed, "ProviderRetry");
    let rejected = count(&observed, "ResponseRejected");
    println!("COUNTS model_started={started} provider_retry={retries} response_rejected={rejected}");
    match &outcome {
        Ok(outcome) => println!("OUTCOME ok turns={} retries={} text={:?}", outcome.model_turns, outcome.retries, outcome.text),
        Err(error) => println!("OUTCOME error {error}"),
    }
    if started != 1 {
        return Err(format!("one logical turn announced {started} model requests").into());
    }
    match expectation.as_str() {
        "retry" => {
            let outcome = outcome?;
            if retries != 1 || rejected != 0 || outcome.retries != 1 {
                return Err(format!("unexpected retry narration: {observed:?}").into());
            }
            if !observed
                .iter()
                .any(|line| line == "ProviderRetry 2/3 retryable_provider_response")
            {
                return Err(format!("missing the bounded retry line: {observed:?}").into());
            }
            if outcome.text != "Scripted reply for the rejected-turn diagnostic replay." {
                return Err(format!("unexpected recovered text {:?}", outcome.text).into());
            }
        }
        "rejected" => {
            let error = outcome.err().ok_or("the refused turn completed unexpectedly")?;
            if !error.to_string().contains("opencode.runtime.invalid_turn_json") {
                return Err(format!("unexpected terminal error {error}").into());
            }
            if retries != 2 || rejected != 6 {
                return Err(format!("unexpected refusal narration: {observed:?}").into());
            }
            let attempts = observed
                .iter()
                .filter_map(|line| line.strip_prefix("ResponseRejected "))
                .map(|rest| rest.to_owned())
                .collect::<Vec<_>>();
            let expected = ["1 opencode.runtime.invalid_turn_json", "2 opencode.runtime.invalid_turn_json"];
            if attempts.len() != 6
                || attempts
                    .chunks(2)
                    .any(|pair| pair != expected)
            {
                return Err(format!("unexpected refused attempts: {attempts:?}").into());
            }
        }
        other => return Err(format!("unknown expectation {other}").into()),
    }
    println!("RESULT {expectation} checks passed");
    Ok(())
}
