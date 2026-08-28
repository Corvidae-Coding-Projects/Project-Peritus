//! Credential-owning provider composition used by benchmark runs.

use std::{sync::Arc, time::Duration};

use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, ModelLimits, ModelName,
    OutputLimitEnforcement, ProviderName, ProviderProfile, ResumeKind, StateMode, WireDialect,
};
use peritus_product_runner::RoleProviders;
use peritus_provider_anthropic::{ClaudeExecutable, ClaudeRuntimeConfig, ClaudeRuntimeProvider};
use peritus_provider_core::{CancellationToken, ModelProvider, ProcessLimits};
use peritus_provider_openai::{CodexExecutable, CodexRuntimeConfig, CodexRuntimeProvider};
use peritus_types::ProviderProfileId;

use crate::BenchmarkError;

pub const WRITER_MODEL: &str = "gpt-5.6-sol";
pub const REVIEWER_MODEL: &str = "sonnet";

pub async fn authenticated(
    cancellation: &CancellationToken,
) -> Result<RoleProviders, BenchmarkError> {
    let writer = codex()?;
    let reviewer = claude()?;
    writer
        .require_authenticated(cancellation)
        .await
        .map_err(|error| BenchmarkError::Provider(error.to_string()))?;
    reviewer
        .require_authenticated(cancellation)
        .await
        .map_err(|error| BenchmarkError::Provider(error.to_string()))?;
    let writer: Arc<dyn ModelProvider> = writer;
    let reviewer: Arc<dyn ModelProvider> = reviewer;
    Ok(RoleProviders { writer: Arc::clone(&writer), reviewer, fixer: writer })
}

pub async fn codex_authenticated(
    cancellation: &CancellationToken,
) -> Result<Arc<CodexRuntimeProvider>, BenchmarkError> {
    let provider = codex()?;
    provider
        .require_authenticated(cancellation)
        .await
        .map_err(|error| BenchmarkError::Provider(error.to_string()))?;
    Ok(provider)
}

fn codex() -> Result<Arc<CodexRuntimeProvider>, BenchmarkError> {
    let executable =
        CodexExecutable::discover().map_err(|error| BenchmarkError::Provider(error.to_string()))?;
    let config = CodexRuntimeConfig::new(
        executable,
        profile([0xB1; 16], "openai", WRITER_MODEL, WireDialect::OpenAiCodexRuntime)?,
        process_limits()?,
    )
    .map_err(|error| BenchmarkError::Provider(error.to_string()))?;
    Ok(Arc::new(CodexRuntimeProvider::new(config)))
}

fn claude() -> Result<Arc<ClaudeRuntimeProvider>, BenchmarkError> {
    let executable = ClaudeExecutable::discover()
        .map_err(|error| BenchmarkError::Provider(error.to_string()))?;
    let config = ClaudeRuntimeConfig::new(
        executable,
        profile([0xB2; 16], "anthropic", REVIEWER_MODEL, WireDialect::AnthropicClaudeRuntime)?,
        process_limits()?,
    )
    .map_err(|error| BenchmarkError::Provider(error.to_string()))?;
    Ok(Arc::new(ClaudeRuntimeProvider::new(config)))
}

fn profile(
    identity: [u8; 16],
    provider: &str,
    model: &str,
    dialect: WireDialect,
) -> Result<ProviderProfile, BenchmarkError> {
    ProviderProfile::new(
        ProviderProfileId::new(identity)
            .map_err(|_| BenchmarkError::Provider("provider identity is invalid".to_owned()))?,
        1,
        ProviderName::new(provider.to_owned())
            .map_err(|error| BenchmarkError::Provider(error.to_string()))?,
        ModelName::new(model.to_owned())
            .map_err(|error| BenchmarkError::Provider(error.to_string()))?,
        dialect,
        CapabilityMatrix::new(
            &[Capability::ToolCalls, Capability::ParallelToolCalls, Capability::UsageDetail],
            &[],
        )
        .map_err(|error| BenchmarkError::Provider(error.to_string()))?,
        CapabilityProvenance::Profiled,
        ModelLimits::new(200_000, 32_000, 32, 8, 1)
            .map_err(|error| BenchmarkError::Provider(error.to_string()))?,
        OutputLimitEnforcement::Advisory,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .map_err(|error| BenchmarkError::Provider(error.to_string()))
}

fn process_limits() -> Result<ProcessLimits, BenchmarkError> {
    ProcessLimits::new(16 * 1024 * 1024, 16 * 1024 * 1024, 64 * 1024, Duration::from_mins(5))
        .map_err(|error| BenchmarkError::Provider(error.to_string()))
}
