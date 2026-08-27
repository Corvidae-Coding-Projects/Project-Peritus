//! Credentialed one-turn qualification for the Claude account runtime.

use std::error::Error;
use std::io;
use std::time::Duration;

use peritus_model_protocol::{
    BoundedText, CachePolicy, CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance,
    ContentBlock, GenerationConfig, Message, ModelEvent, ModelLimits, ModelName, ModelRequest,
    OutputLimitEnforcement, ParallelToolPolicy, PersistencePolicy, ProtocolLimits, ProviderName,
    ProviderProfile, ReasoningPolicy, RequestId, RequestOptions, RequestedCapabilities, ResumeKind,
    Role, StateMode, StructuredOutput, ToolChoice, WireDialect, negotiate,
};
use peritus_provider_anthropic::{ClaudeExecutable, ClaudeRuntimeConfig, ClaudeRuntimeProvider};
use peritus_provider_core::{CancellationToken, ModelProvider, ProcessLimits};
use peritus_types::ProviderProfileId;

const CANARY: &str = "PERITUS_CLAUDE_ACCOUNT_ROUTE_OK";
const DEFAULT_MODEL: &str = "sonnet";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let model = std::env::args().nth(1).unwrap_or_else(|| DEFAULT_MODEL.to_owned());
    let profile = profile(&model)?;
    let request = request(&profile)?;
    let executable = ClaudeExecutable::discover()?;
    let process_limits =
        ProcessLimits::new(16 * 1024 * 1024, 16 * 1024 * 1024, 64 * 1024, Duration::from_mins(3))?;
    let provider =
        ClaudeRuntimeProvider::new(ClaudeRuntimeConfig::new(executable, profile, process_limits)?);
    let cancellation = CancellationToken::new();

    provider.require_authenticated(&cancellation).await?;
    let mut stream = provider.start(request, cancellation).await?;
    let mut text = Vec::new();
    let mut events = 0_u64;
    let mut usage_observed = false;
    let mut completed = false;

    while let Some(envelope) = stream.pull().await? {
        events = events.checked_add(1).ok_or_else(|| io::Error::other("event count overflow"))?;
        if envelope.sequence() != events {
            return Err(io::Error::other("adapter emitted a non-contiguous sequence").into());
        }
        match envelope.event() {
            ModelEvent::TextDelta { fragment, .. } => text.extend_from_slice(fragment.expose()),
            ModelEvent::Usage(_) => usage_observed = true,
            ModelEvent::ToolCallStarted { .. } | ModelEvent::ToolArgumentDelta { .. } => {
                return Err(io::Error::other("tool activity escaped a tool-free request").into());
            }
            ModelEvent::ResponseCompleted => completed = true,
            ModelEvent::ResponseFailed(failure) => {
                return Err(io::Error::other(format!(
                    "Claude account runtime returned a normalized failure: {failure:?}"
                ))
                .into());
            }
            ModelEvent::ResponseCancelled => {
                return Err(io::Error::other("Claude account runtime was cancelled").into());
            }
            _ => {}
        }
    }

    if !completed || !stream.terminal_observed() {
        return Err(io::Error::other("Claude account runtime did not complete").into());
    }
    if !usage_observed {
        return Err(io::Error::other("Claude account runtime omitted required usage").into());
    }
    let text = String::from_utf8(text)?;
    if text.trim() != CANARY {
        return Err(io::Error::other("Claude account runtime returned unexpected text").into());
    }

    println!(
        "{}",
        serde_json::json!({
            "provider": "anthropic-claude-account",
            "model": model,
            "events": events,
            "usage_observed": true,
            "terminal": "completed",
            "canary": "matched",
        })
    );
    Ok(())
}

fn profile(model: &str) -> Result<ProviderProfile, Box<dyn Error>> {
    Ok(ProviderProfile::new(
        ProviderProfileId::new([0xB6; 16])
            .map_err(|_| io::Error::other("provider profile identity is invalid"))?,
        1,
        ProviderName::new("anthropic".to_owned())?,
        ModelName::new(model.to_owned())?,
        WireDialect::AnthropicClaudeRuntime,
        CapabilityMatrix::new(
            &[Capability::ToolCalls, Capability::ParallelToolCalls, Capability::UsageDetail],
            &[],
        )?,
        CapabilityProvenance::Profiled,
        ModelLimits::new(200_000, 32_000, 32, 8, 1)?,
        OutputLimitEnforcement::Advisory,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )?)
}

fn request(profile: &ProviderProfile) -> Result<ModelRequest, Box<dyn Error>> {
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(&[Capability::UsageDetail], &[], profile.limits())?,
    )?;
    let messages = vec![
        message(Role::System, "Return the user's requested text exactly, with no decoration.")?,
        message(Role::User, &format!("Return exactly {CANARY} and nothing else."))?,
    ];
    Ok(ModelRequest::new(
        profile,
        negotiated,
        RequestId::new("live-claude-account-qualification".to_owned())?,
        messages,
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        RequestOptions::new(
            StructuredOutput::Text,
            ReasoningPolicy::Disabled,
            GenerationConfig::new(64, Vec::new(), None, None, None)?,
            CachePolicy::Disabled,
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        ProtocolLimits::PRODUCTION,
    )?)
}

fn message(role: Role, text: &str) -> Result<Message, Box<dyn Error>> {
    Ok(Message::new(
        role,
        vec![ContentBlock::Text(BoundedText::new(text.to_owned(), ProtocolLimits::PRODUCTION)?)],
        ProtocolLimits::PRODUCTION,
    )?)
}
