//! Ordinary public constructors and actual hardened Reqwest over owned loopback TCP.

use super::common::{self, Case, Metrics, Mode, Server};
use peritus_provider_anthropic::{AnthropicClient, AnthropicConfig};
use peritus_provider_compatible::{
    CompatibleAuth, CompatibleClient, CompatibleConfig, CompatibleProfile, CompatibleRetryStatuses,
};
use peritus_provider_core::{FramingLimits, HttpLimits, ModelProvider};
use peritus_provider_google::{GoogleClient, GoogleConfig};
use std::sync::Arc;

pub fn check(case: &Case) -> Result<Metrics, String> {
    let server = Server::new(case)?;
    let profile = common::profile(case.mode)?;
    let credentials = Arc::new(common::SyntheticCredential);
    let provider: Arc<dyn ModelProvider> = match case.mode {
        Mode::Anthropic => Arc::new(
            AnthropicClient::new(
                AnthropicConfig::new(
                    server.endpoint.clone(),
                    common::credential()?,
                    profile,
                    vec![],
                    HttpLimits::PRODUCTION,
                    FramingLimits::PRODUCTION,
                    common::retry_policy()?,
                )
                .map_err(|e| e.to_string())?,
                Box::new(common::SyntheticCredential),
            )
            .map_err(|e| e.to_string())?,
        ),
        Mode::GoogleGenerate | Mode::GoogleInteractions => Arc::new(
            GoogleClient::new(
                GoogleConfig::new(
                    server.endpoint.clone(),
                    common::credential()?,
                    profile,
                    HttpLimits::PRODUCTION,
                    FramingLimits::PRODUCTION,
                    common::retry_policy()?,
                )
                .map_err(|e| e.to_string())?,
                Box::new(common::SyntheticCredential),
            )
            .map_err(|e| e.to_string())?,
        ),
        Mode::CompatibleChat | Mode::CompatibleResponses => {
            let endpoint = server
                .endpoint
                .with_path(if case.mode == Mode::CompatibleChat {
                    "/v1/chat/completions"
                } else {
                    "/v1/responses"
                })
                .map_err(|e| e.to_string())?;
            let config = CompatibleConfig::new(
                endpoint,
                CompatibleAuth::bearer(common::credential()?).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?
            .with_retry_policy(common::retry_policy()?)
            .with_retry_statuses(CompatibleRetryStatuses::none().with_server_errors());
            let profile = if case.mode == Mode::CompatibleChat {
                CompatibleProfile::chat_completions(profile)
            } else {
                CompatibleProfile::responses(profile)
            }
            .map_err(|e| e.to_string())?;
            Arc::new(
                CompatibleClient::new(config, profile, credentials).map_err(|e| e.to_string())?,
            )
        }
        _ => return Err("public HTTP fixture received a private/owned-runtime mode".into()),
    };
    let mut metrics = Metrics::new();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(common::drive(provider, case, &mut metrics))?;
    let requests = server.finish()?;
    common::inspect_http(case, &requests, &mut metrics)?;
    Ok(metrics)
}
