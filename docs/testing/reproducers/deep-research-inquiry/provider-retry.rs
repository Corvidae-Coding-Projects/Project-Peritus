//! Actual official-client timeout followed by live inference through the public developer loop.
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
use serde_json::{Value, json};
use std::{error::Error, fs, io::Write, path::PathBuf, time::Duration};

struct NoTools;
impl DeveloperToolExecutor for NoTools {
    fn execute(
        &mut self,
        _: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        Err(DeveloperLoopError::Tool(
            "No application effects are authorized in this consumer".into(),
        ))
    }
}
struct Trace {
    output: PathBuf,
    envelopes: fs::File,
    retries: Vec<Value>,
}
impl DeveloperTrace for Trace {
    fn record(&mut self, event: DeveloperTraceEvent<'_>) -> Result<(), DeveloperLoopError> {
        let result = match event {
            DeveloperTraceEvent::ProviderEnvelope(bytes) => self
                .envelopes
                .write_all(&(bytes.len() as u64).to_le_bytes())
                .and_then(|()| self.envelopes.write_all(bytes)),
            DeveloperTraceEvent::RetryScheduled(r) => {
                self.retries.push(json!({"attempt":r.attempt(),"maximum":r.max_attempts(),
                    "elapsed_millis":r.elapsed_millis(),"delay_millis":r.delay_millis(),
                    "reason":r.reason().as_str()}));
                fs::write(
                    self.output.join("retry-observed.json"),
                    serde_json::to_vec_pretty(&self.retries).unwrap(),
                )
            }
            _ => Ok(()),
        };
        result.map_err(|e| DeveloperLoopError::Trace(e.to_string()))
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let output = PathBuf::from(std::env::args().nth(1).ok_or("output directory required")?);
    fs::create_dir(&output)?;
    let profile = ProviderProfile::new(
        ProviderProfileId::new([93; 16])
            .map_err(|_| std::io::Error::other("invalid consumer profile identity"))?,
        1,
        ProviderName::new("openai".into())?,
        ModelName::new("gpt-6.1-sol".into())?,
        WireDialect::OpenAiCodexRuntime,
        CapabilityMatrix::new(&[Capability::ToolCalls, Capability::UsageDetail], &[])?,
        CapabilityProvenance::Profiled,
        ModelLimits::new(200_000, 32_768, 64, 1, 16 * 1024 * 1024)?,
        OutputLimitEnforcement::Advisory,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )?;
    let executable = CodexExecutable::discover()?;
    let provider = CodexRuntimeProvider::new(CodexRuntimeConfig::new(
        executable,
        profile,
        ProcessLimits::new(2 * 1024 * 1024, 2 * 1024 * 1024, 64 * 1024, Duration::from_secs(125))?,
    )?);
    let mut trace = Trace {
        output: output.clone(),
        envelopes: fs::File::create(output.join("provider-envelopes.bin"))?,
        retries: Vec::new(),
    };
    fs::write(
        output.join("ready.json"),
        serde_json::to_vec(&json!({"pid":std::process::id(),
        "fault":"Observer SIGSTOPs only the first owned Codex exec process; real process deadline is 125 seconds. Authentication probes and retry are left alone."}))?,
    )?;
    let result = DeveloperLoop::run(
        &provider,
        DeveloperLoopRequest {
            request_prefix: format!("actual-provider-retry-{}", std::process::id()),
            system: "Return exactly RETRY-RECOVERED. No tools, research or other action.".into(),
            prompt: "Reply with the requested marker.".into(),
            attachments: Vec::new(),
            tools: Vec::new(),
            limits: DeveloperLoopLimits::new(2, 1)?,
            cancellation: CancellationToken::new(),
        },
        &mut NoTools,
        &mut trace,
    )
    .await;
    match result {
        Ok(result) => {
            if result.text.trim() != "RETRY-RECOVERED"
                || result.retries != 1
                || result.tool_calls != 0
                || trace.retries.len() != 1
                || trace.retries[0]["elapsed_millis"].as_u64().unwrap_or(0) < 120_000
            {
                return Err("Actual timeout did not recover under the expected contract".into());
            }
            fs::write(
                output.join("receipt.json"),
                serde_json::to_vec_pretty(&json!({"status":"PASS",
                "scope":"Actual stopped official Codex process and real runtime deadline, then live provider inference through DeveloperLoop; no mock provider or application effects",
                "marker":result.text,"tool_calls":result.tool_calls,"retries":trace.retries}))?,
            )?;
            println!("PASS {}", output.display());
        }
        Err(error) => {
            fs::write(
                output.join("failure.json"),
                serde_json::to_vec_pretty(
                    &json!({"error":error.to_string(),"retries":trace.retries}),
                )?,
            )?;
            return Err(error.into());
        }
    }
    Ok(())
}
