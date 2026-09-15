//! Credential-owning provider composition used by benchmark runs.

use std::{env, sync::Arc, time::Duration};

use peritus_daemon::DaemonComponents;
use peritus_launcher::{AppLayout, ProductBootstrap};
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

const PROVIDER_SOURCE_ENV: &str = "PERITUS_BENCHMARK_PROVIDER_SOURCE";
const WRITER_MODEL: &str = "gpt-5.6-sol";
const REVIEWER_MODEL: &str = "sonnet";

pub struct AuthenticatedProviders {
    pub roles: RoleProviders,
    pub routes: Vec<ProviderRouteReport>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProviderSource {
    AccountRuntimes,
    Configured,
}

impl ProviderSource {
    fn from_environment() -> Result<Self, BenchmarkError> {
        let Some(value) = env::var_os(PROVIDER_SOURCE_ENV) else {
            return Ok(Self::AccountRuntimes);
        };
        let value = value.into_string().map_err(|_| {
            BenchmarkError::Arguments(format!("{PROVIDER_SOURCE_ENV} is not UTF-8"))
        })?;
        Self::parse(Some(&value))
    }

    fn parse(value: Option<&str>) -> Result<Self, BenchmarkError> {
        match value {
            None | Some("account-runtimes") => Ok(Self::AccountRuntimes),
            Some("configured") => Ok(Self::Configured),
            Some(other) => Err(BenchmarkError::Arguments(format!(
                "{PROVIDER_SOURCE_ENV} must be account-runtimes or configured, not {other:?}"
            ))),
        }
    }
}

pub struct ProviderPlan {
    writer: Arc<dyn ModelProvider>,
    reviewer: Arc<dyn ModelProvider>,
    fixer: Arc<dyn ModelProvider>,
    fallbacks: Vec<Arc<dyn ModelProvider>>,
}

impl ProviderPlan {
    pub(crate) fn for_harness() -> Result<Self, BenchmarkError> {
        Self::load(None)
    }

    pub(crate) fn for_model(model_id: &str) -> Result<Self, BenchmarkError> {
        Self::load(Some(model_id))
    }

    fn load(configured_model: Option<&str>) -> Result<Self, BenchmarkError> {
        match ProviderSource::from_environment()? {
            ProviderSource::AccountRuntimes => Self::account_runtimes(),
            ProviderSource::Configured => Self::configured(configured_model),
        }
    }

    pub(crate) fn declared_routes(&self) -> Vec<ProviderRouteReport> {
        vec![
            declared_route("writer", self.writer.as_ref()),
            declared_route("reviewer", self.reviewer.as_ref()),
        ]
    }

    pub(crate) fn writer_label(&self) -> String {
        provider_label(self.writer.as_ref())
    }

    pub(crate) fn reviewer_label(&self) -> String {
        provider_label(self.reviewer.as_ref())
    }

    pub(crate) async fn authenticate(
        self,
        cancellation: &CancellationToken,
    ) -> Result<AuthenticatedProviders, BenchmarkError> {
        let requirement = provider_requirement()?;
        let writer_qualification = qualify(self.writer.as_ref(), requirement, cancellation).await?;
        let reviewer_qualification = if Arc::ptr_eq(&self.writer, &self.reviewer) {
            writer_qualification
        } else {
            qualify(self.reviewer.as_ref(), requirement, cancellation).await?
        };
        let routes = vec![
            route_report("writer", self.writer.as_ref(), writer_qualification),
            route_report("reviewer", self.reviewer.as_ref(), reviewer_qualification),
        ];
        Ok(AuthenticatedProviders {
            roles: RoleProviders {
                writer: self.writer,
                reviewer: self.reviewer,
                fixer: self.fixer,
                fallbacks: self.fallbacks,
            },
            routes,
        })
    }

    pub(crate) async fn authenticate_writer(
        self,
        cancellation: &CancellationToken,
    ) -> Result<Arc<dyn ModelProvider>, BenchmarkError> {
        qualify(self.writer.as_ref(), provider_requirement()?, cancellation).await?;
        Ok(self.writer)
    }

    fn account_runtimes() -> Result<Self, BenchmarkError> {
        let writer: Arc<dyn ModelProvider> = codex()?;
        let reviewer: Arc<dyn ModelProvider> = claude()?;
        Ok(Self {
            writer: Arc::clone(&writer),
            reviewer: Arc::clone(&reviewer),
            fixer: Arc::clone(&writer),
            fallbacks: vec![writer, reviewer],
        })
    }

    fn configured(model_id: Option<&str>) -> Result<Self, BenchmarkError> {
        let layout = AppLayout::discover()
            .and_then(AppLayout::prepare)
            .map_err(|error| configured_error("prepare Peritus application directories", error))?;
        let prepared = ProductBootstrap::new(layout)
            .prepare()
            .map_err(|error| configured_error("load Peritus provider selection", error))?;
        let selected = prepared.state().providers();
        let kind = selected.default().ok_or_else(|| {
            BenchmarkError::Provider(
                "configured benchmark provider source requires a default Peritus provider"
                    .to_owned(),
            )
        })?;
        let profile_id = ProviderProfileId::new(kind.profile_identity()).map_err(|_| {
            BenchmarkError::Provider("configured provider identity is invalid".to_owned())
        })?;
        let components = DaemonComponents::build(prepared.daemon_config())
            .map_err(|error| configured_error("construct configured provider registry", error))?;
        let registry = components.providers();
        let provider = registry.current_provider(profile_id).ok_or_else(|| {
            BenchmarkError::Provider(
                "default Peritus provider is absent or has multiple active revisions".to_owned(),
            )
        })?;
        let configured_model = provider.profile().model().as_str();
        if let Some(model_id) = model_id
            && configured_model != model_id
        {
            return Err(BenchmarkError::Provider(format!(
                "configured provider uses model {configured_model:?}, but rubric invocation requested {model_id:?}; set RUBRIC_MODEL to the exact configured model ID"
            )));
        }
        let fallbacks = if selected.automatic_failover() {
            registry
                .keys()
                .into_iter()
                .filter_map(|key| registry.provider(key.profile_id(), key.revision()))
                .collect()
        } else {
            Vec::new()
        };
        Ok(Self {
            writer: Arc::clone(&provider),
            reviewer: Arc::clone(&provider),
            fixer: Arc::clone(&provider),
            fallbacks,
        })
    }
}

pub async fn authenticated(
    cancellation: &CancellationToken,
) -> Result<AuthenticatedProviders, BenchmarkError> {
    let plan = ProviderPlan::for_harness()?;
    plan.authenticate(cancellation).await
}

async fn qualify(
    provider: &dyn ModelProvider,
    requirement: ProviderRequirement,
    cancellation: &CancellationToken,
) -> Result<ProviderQualification, BenchmarkError> {
    verify_live_provider(provider, requirement, cancellation.clone())
        .await
        .map_err(|error| BenchmarkError::Provider(error.to_string()))
}

fn provider_requirement() -> Result<ProviderRequirement, BenchmarkError> {
    ProviderRequirement::new(false, 1, true)
        .map_err(|error| BenchmarkError::Provider(error.to_string()))
}

fn configured_error(operation: &'static str, error: impl std::fmt::Display) -> BenchmarkError {
    BenchmarkError::Provider(format!("{operation}: {error}"))
}

fn provider_label(provider: &dyn ModelProvider) -> String {
    format!("{}/{}", provider.profile().provider().as_str(), provider.profile().model().as_str())
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

fn declared_route(role: &'static str, provider: &dyn ModelProvider) -> ProviderRouteReport {
    let profile = provider.profile();
    ProviderRouteReport {
        role,
        provider: profile.provider().as_str().to_owned(),
        model: profile.model().as_str().to_owned(),
        route: route_name(provider.route()),
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
mod tests;
