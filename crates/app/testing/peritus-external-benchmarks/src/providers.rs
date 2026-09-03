//! Credential-owning provider composition used by benchmark runs.

use std::{sync::Arc, time::Duration};

use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, ModelLimits, ModelName,
    OutputLimitEnforcement, ProviderName, ProviderProfile, ResumeKind, StateMode, WireDialect,
};
use peritus_product_runner::RoleProviders;
use peritus_provider_anthropic::{ClaudeExecutable, ClaudeRuntimeConfig, ClaudeRuntimeProvider};
use peritus_provider_core::{
    CancellationToken, ModelProvider, ProcessLimits, ProviderAvailability, ProviderQualification,
    ProviderRequirement, ProviderRoute, verify_live_provider,
};
use peritus_provider_openai::{CodexExecutable, CodexRuntimeConfig, CodexRuntimeProvider};
use peritus_types::ProviderProfileId;

use crate::{BenchmarkError, evidence::ProviderRouteReport};

pub const WRITER_MODEL: &str = "gpt-5.6-sol";
pub const REVIEWER_MODEL: &str = "sonnet";

pub struct AuthenticatedProviders {
    pub roles: RoleProviders,
    pub routes: Vec<ProviderRouteReport>,
}

pub fn declared_routes() -> Result<Vec<ProviderRouteReport>, BenchmarkError> {
    Ok(vec![
        declared_route(
            "writer",
            &profile([0xB1; 16], "openai", WRITER_MODEL, WireDialect::OpenAiCodexRuntime)?,
        ),
        declared_route(
            "reviewer",
            &profile([0xB2; 16], "anthropic", REVIEWER_MODEL, WireDialect::AnthropicClaudeRuntime)?,
        ),
    ])
}

pub async fn authenticated(
    cancellation: &CancellationToken,
) -> Result<AuthenticatedProviders, BenchmarkError> {
    let writer = codex()?;
    let reviewer = claude()?;
    let requirement = ProviderRequirement::new(false, 1, true)
        .map_err(|error| BenchmarkError::Provider(error.to_string()))?;
    let writer_qualification =
        verify_live_provider(writer.as_ref(), requirement, cancellation.clone())
            .await
            .map_err(|error| BenchmarkError::Provider(error.to_string()))?;
    let reviewer_qualification =
        verify_live_provider(reviewer.as_ref(), requirement, cancellation.clone())
            .await
            .map_err(|error| BenchmarkError::Provider(error.to_string()))?;
    let routes = vec![
        route_report("writer", writer.as_ref(), writer_qualification),
        route_report("reviewer", reviewer.as_ref(), reviewer_qualification),
    ];
    let writer: Arc<dyn ModelProvider> = writer;
    let reviewer: Arc<dyn ModelProvider> = reviewer;
    Ok(AuthenticatedProviders { roles: authorized_roles(writer, reviewer), routes })
}

fn route_report(
    role: &'static str,
    provider: &dyn ModelProvider,
    qualification: ProviderQualification,
) -> ProviderRouteReport {
    ProviderRouteReport {
        role,
        provider: provider.profile().provider().as_str().to_owned(),
        model: provider.profile().model().as_str().to_owned(),
        route: route_name(qualification.route()),
        availability: availability_name(qualification.availability()),
        text: true,
        image_input: qualification.image_input(),
        maximum_context_tokens: qualification.maximum_context_tokens(),
        tool_protocol: qualification.tool_protocol(),
    }
}

fn declared_route(role: &'static str, profile: &ProviderProfile) -> ProviderRouteReport {
    ProviderRouteReport {
        role,
        provider: profile.provider().as_str().to_owned(),
        model: profile.model().as_str().to_owned(),
        route: route_name(ProviderRoute::from_dialect(profile.dialect())),
        availability: availability_name(ProviderAvailability::Unchecked),
        text: true,
        image_input: profile.capabilities().supports(Capability::ImageInput),
        maximum_context_tokens: profile.limits().max_input_tokens(),
        tool_protocol: profile.capabilities().supports(Capability::ToolCalls),
    }
}

const fn route_name(route: ProviderRoute) -> &'static str {
    match route {
        ProviderRoute::FirstPartyApi => "first_party_api",
        ProviderRoute::CompatibleApi => "compatible_api",
        ProviderRoute::AccountRuntime => "account_runtime",
    }
}

const fn availability_name(availability: ProviderAvailability) -> &'static str {
    match availability {
        ProviderAvailability::Unchecked => "unchecked",
        ProviderAvailability::CredentialPresent => "credential_present",
        ProviderAvailability::LiveCanary => "live_canary",
        ProviderAvailability::Unavailable => "unavailable",
    }
}

fn authorized_roles(
    writer: Arc<dyn ModelProvider>,
    reviewer: Arc<dyn ModelProvider>,
) -> RoleProviders {
    RoleProviders {
        writer: Arc::clone(&writer),
        reviewer: Arc::clone(&reviewer),
        fixer: Arc::clone(&writer),
        fallbacks: vec![writer, reviewer],
    }
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
    let mut supported = vec![
        Capability::ToolCalls,
        Capability::ParallelToolCalls,
        Capability::ReasoningControls,
        Capability::UsageDetail,
    ];
    let inline_media_bytes = if dialect == WireDialect::OpenAiCodexRuntime {
        supported.push(Capability::ImageInput);
        32 * 1024 * 1024
    } else {
        1
    };
    ProviderProfile::new(
        ProviderProfileId::new(identity)
            .map_err(|_| BenchmarkError::Provider("provider identity is invalid".to_owned()))?,
        1,
        ProviderName::new(provider.to_owned())
            .map_err(|error| BenchmarkError::Provider(error.to_string()))?,
        ModelName::new(model.to_owned())
            .map_err(|error| BenchmarkError::Provider(error.to_string()))?,
        dialect,
        CapabilityMatrix::new(&supported, &[])
            .map_err(|error| BenchmarkError::Provider(error.to_string()))?,
        CapabilityProvenance::Profiled,
        ModelLimits::new(200_000, 32_000, 32, 8, inline_media_bytes)
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

#[cfg(test)]
mod tests {
    use peritus_model_protocol::ModelRequest;
    use peritus_provider_core::{BoxFuture, OwnedModelStream, ProviderCoreError};

    use super::*;

    struct StubProvider(ProviderProfile);

    impl ModelProvider for StubProvider {
        fn profile(&self) -> &ProviderProfile {
            &self.0
        }

        fn start(
            &self,
            _request: ModelRequest,
            _cancellation: CancellationToken,
        ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
            Box::pin(async {
                Err(ProviderCoreError::configuration(
                    "benchmark_provider_test",
                    "provider invocation is outside this composition test",
                ))
            })
        }
    }

    #[test]
    fn every_authenticated_route_is_an_explicit_fallback_candidate() {
        let writer: Arc<dyn ModelProvider> = Arc::new(StubProvider(
            profile([0xC1; 16], "openai", "writer", WireDialect::OpenAiCodexRuntime)
                .expect("writer profile"),
        ));
        let reviewer: Arc<dyn ModelProvider> = Arc::new(StubProvider(
            profile([0xC2; 16], "anthropic", "reviewer", WireDialect::AnthropicClaudeRuntime)
                .expect("reviewer profile"),
        ));
        let writer_id = writer.profile().profile_id();
        let reviewer_id = reviewer.profile().profile_id();

        let roles = authorized_roles(writer, reviewer);

        assert_eq!(roles.writer.profile().profile_id(), writer_id);
        assert_eq!(roles.fixer.profile().profile_id(), writer_id);
        assert_eq!(roles.reviewer.profile().profile_id(), reviewer_id);
        assert!(roles.writer.profile().capabilities().supports(Capability::ImageInput));
        assert!(!roles.reviewer.profile().capabilities().supports(Capability::ImageInput));
        assert_eq!(
            roles
                .fallbacks
                .iter()
                .map(|provider| provider.profile().profile_id())
                .collect::<Vec<_>>(),
            vec![writer_id, reviewer_id],
        );
    }
}
