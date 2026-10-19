//! Operate the real Codex provider or decode a retained real executable transcript.
//! Built as an ordinary consumer, not a Cargo test or a replacement provider.

#[path = "../../../crates/model/peritus-provider-openai/src/runtime/output.rs"]
mod runtime_output;

use peritus_agent::{
    DeveloperLoop, DeveloperLoopError, DeveloperLoopLimits, DeveloperLoopRequest,
    DeveloperToolExecutor, DeveloperToolObservation, DeveloperTrace, DeveloperTraceEvent,
};
use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, CompletedToolCall,
    ModelLimits, ModelName, OutputLimitEnforcement, ProviderName, ProviderProfile, ResumeKind,
    StateMode, WireDialect,
};
use peritus_provider_core::{CancellationToken, ProcessLimits};
use peritus_provider_openai::{CodexExecutable, CodexRuntimeConfig, CodexRuntimeProvider};
use peritus_types::ProviderProfileId;
use serde_json::json;
use std::{collections::BTreeSet, error::Error, fs, path::Path, time::Duration};

struct NoTools;
impl DeveloperToolExecutor for NoTools {
    fn execute(
        &mut self,
        _: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        Err(DeveloperLoopError::Tool("This consumer authorizes no host effects".into()))
    }
}

#[derive(Default)]
struct Trace {
    retries: usize,
}
impl DeveloperTrace for Trace {
    fn record(&mut self, event: DeveloperTraceEvent<'_>) -> Result<(), DeveloperLoopError> {
        if matches!(event, DeveloperTraceEvent::RetryScheduled(_)) {
            self.retries += 1;
        }
        Ok(())
    }
}

fn replay(arguments: &[String]) -> Result<(), Box<dyn Error>> {
    let [input, expected, rest @ ..] = arguments else {
        return Err("replay INPUT EXPECTED [FINAL_MESSAGE]".into());
    };
    let final_message = rest.first().map(fs::read_to_string).transpose()?;
    let bytes = fs::read(input)?;
    let decoded = runtime_output::decode_with_usage(
        &bytes,
        &BTreeSet::new(),
        0..=0,
        final_message.as_deref(),
    );
    let observed = match decoded.turn {
        Ok(turn) => {
            if turn.content != "PERITUS-PROVIDER-READY" || !turn.tool_calls.is_empty() {
                return Err(
                    "Decoded result disagrees with the independently requested marker".into()
                );
            }
            "complete".to_owned()
        }
        Err(error) => format!("{error:?}"),
    };
    println!("{}", json!({"input":input,"observed":observed,"expected":expected}));
    if &observed != expected {
        return Err("Unexpected production decoder result".into());
    }
    Ok(())
}

async fn live(arguments: &[String]) -> Result<(), Box<dyn Error>> {
    let [executable, model, output, expected] = arguments else {
        return Err("live OFFICIAL_CODEX MODEL OUTPUT_DIRECTORY complete|model_unavailable".into());
    };
    let output = Path::new(output);
    fs::create_dir_all(output)?;
    let profile = ProviderProfile::new(
        ProviderProfileId::new([94; 16]).map_err(|_| "invalid consumer identity")?,
        1,
        ProviderName::new("openai".into())?,
        ModelName::new(model.clone())?,
        WireDialect::OpenAiCodexRuntime,
        CapabilityMatrix::new(&[Capability::ToolCalls, Capability::UsageDetail], &[])?,
        CapabilityProvenance::Profiled,
        ModelLimits::new(200_000, 32_768, 64, 1, 16 * 1024 * 1024)?,
        OutputLimitEnforcement::Advisory,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )?;
    let provider = CodexRuntimeProvider::new(CodexRuntimeConfig::new(
        CodexExecutable::pin(executable)?,
        profile,
        ProcessLimits::new(2 * 1024 * 1024, 2 * 1024 * 1024, 64 * 1024, Duration::from_secs(180))?,
    )?);
    let mut trace = Trace::default();
    let result = DeveloperLoop::run(
        &provider,
        DeveloperLoopRequest {
            request_prefix: format!("codex-notification-live-{}", std::process::id()),
            system: "Reply exactly PERITUS-PROVIDER-READY. Do not use tools.".into(),
            prompt: "Return the requested marker.".into(),
            attachments: Vec::new(),
            tools: Vec::new(),
            limits: DeveloperLoopLimits::new(2, 1)?,
            cancellation: CancellationToken::new(),
        },
        &mut NoTools,
        &mut trace,
    )
    .await;
    let (observed, detail) = match result {
        Ok(result) => {
            if result.text.trim() != "PERITUS-PROVIDER-READY" || result.tool_calls != 0 {
                return Err(
                    "Live result disagrees with requested marker/no-effects contract".into()
                );
            }
            ("complete", result.text)
        }
        Err(error @ DeveloperLoopError::ProviderTerminal { .. }) => {
            let DeveloperLoopError::ProviderTerminal { category, ref diagnostic_code, .. } = error
            else {
                return Err("unreachable terminal shape".into());
            };
            if category != peritus_model_protocol::FailureCategory::InvalidRequest
                || diagnostic_code != "openai.codex_runtime.model_unavailable"
            {
                return Err(error.into());
            }
            let detail = error.to_string();
            if !detail.contains("/model") || !detail.contains("Saved work is retained") {
                return Err("Model selection recovery guidance is missing".into());
            }
            ("model_unavailable", detail)
        }
        Err(error) => return Err(error.into()),
    };
    let receipt = json!({"observed":observed,"expected":expected,"detail":detail,
        "retries":trace.retries,"model":model,"official_executable":executable,
        "scope":"Actual authenticated official Codex through the public provider and DeveloperLoop; no native or host tools authorized"});
    fs::write(output.join("receipt.json"), serde_json::to_vec_pretty(&receipt)?)?;
    println!("{receipt}");
    if observed != expected || trace.retries != 0 {
        return Err("Live provider outcome/retry behavior violates the expected contract".into());
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let Some((mode, arguments)) = arguments.split_first() else {
        return Err("Use replay or live".into());
    };
    match mode.as_str() {
        "replay" => replay(arguments),
        "live" => live(arguments).await,
        _ => Err("Use replay or live".into()),
    }
}
