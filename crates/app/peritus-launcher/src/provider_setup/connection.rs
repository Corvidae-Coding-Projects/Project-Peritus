//! Provider settings actions and explicit small inference checks.

use crate::{LauncherError, terminal::Terminal};
use peritus_product_state::DirectProviderProfile;

pub(super) fn existing(
    terminal: &mut Terminal<'_>,
    profile: &DirectProviderProfile,
) -> Result<DirectProviderProfile, LauncherError> {
    loop {
        let answer = terminal.prompt(&format!("{} / {}: Enter to keep, r to replace key/model, t to test connection (up to 3 small requests; may use paid tokens): ", profile.kind().label(), profile.model()))?;
        match answer.to_ascii_lowercase().as_str() {
            "" => return Ok(profile.clone()),
            "r" => return super::direct::setup(terminal, profile.kind()),
            "t" => test(terminal, profile)?,
            _ => terminal.line("Press Enter, r, or t.")?,
        }
    }
}

pub(super) fn offer(
    terminal: &mut Terminal<'_>,
    profile: &DirectProviderProfile,
) -> Result<(), LauncherError> {
    if terminal.confirm(
        "Test generation and tool calling now? Up to 3 small requests may use paid tokens. [y/N]: ",
        false,
    )? {
        test(terminal, profile)?;
    }
    Ok(())
}

fn test(terminal: &mut Terminal<'_>, profile: &DirectProviderProfile) -> Result<(), LauncherError> {
    terminal.line(&format!(
        "Testing {} / {} (45-second deadline)…",
        profile.kind().label(),
        profile.model()
    ))?;
    let result = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let route = route(profile)?;
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| LauncherError::Interaction(error.to_string()))?;
                runtime
                    .block_on(peritus_daemon::test_provider_connection(&route))
                    .map_err(|error| LauncherError::Interaction(error.to_string()))
            })
            .join()
            .map_err(|_| {
                LauncherError::Interaction("provider connection test worker stopped".to_owned())
            })?
    });
    match result {
        Ok(report) => {
            for stage in report.completed {
                terminal.line(&format!("Passed: {}", stage.label()))?;
            }
        }
        Err(error) => {
            terminal.line(&format!("Connection test failed: {error}"))?;
            terminal.line("Check the API key, billing/model access, and selected protocol. You can replace settings or test again.")?;
        }
    }
    Ok(())
}

fn route(profile: &DirectProviderProfile) -> Result<peritus_daemon::ProviderRoute, LauncherError> {
    let text =
        crate::bootstrap::configuration::render_direct_provider(profile.kind(), Some(profile))?;
    let config: toml::Value =
        toml::from_str(&text).map_err(|error| LauncherError::Interaction(error.to_string()))?;
    config
        .get("providers")
        .and_then(toml::Value::as_array)
        .and_then(|providers| providers.first())
        .cloned()
        .ok_or_else(|| {
            LauncherError::Interaction("connection test has no provider route".to_owned())
        })?
        .try_into()
        .map_err(|error: toml::de::Error| LauncherError::Interaction(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_product_state::{CompatibleProtocol, ProviderKind};
    use peritus_provider_core::{
        Credential, CredentialReference, CredentialSource, ProviderCoreError,
    };
    use std::sync::Arc;

    struct NoCredentialRead;
    impl CredentialSource for NoCredentialRead {
        fn resolve(&self, _: &CredentialReference) -> Result<Credential, ProviderCoreError> {
            panic!("factory construction must not read credentials or send a request")
        }
    }

    #[test]
    fn every_named_setup_route_builds_its_production_adapter_without_a_model_whitelist() {
        for kind in ProviderKind::ALL.into_iter().filter(|kind| kind.hosted_service().is_some()) {
            let protocols = if matches!(kind, ProviderKind::OpenCodeZen | ProviderKind::OpenCodeGo)
            {
                vec![
                    CompatibleProtocol::Responses,
                    CompatibleProtocol::ChatCompletions,
                    CompatibleProtocol::AnthropicMessages,
                    CompatibleProtocol::GoogleGenerateContent,
                ]
            } else {
                vec![CompatibleProtocol::ChatCompletions]
            };
            for protocol in protocols {
                let profile = DirectProviderProfile::new(
                    kind,
                    "peritus-secret-v1:fixture-reference".to_owned(),
                    None,
                    "future-model".to_owned(),
                    Some(protocol),
                    None,
                )
                .expect("profile");
                let declaration =
                    route(&profile).expect("generated route").declaration().expect("declaration");
                let id = declaration.profile().profile_id();
                assert_eq!(id.as_bytes(), &kind.profile_identity());
                let registry = peritus_daemon::ProviderRegistry::build(
                    vec![declaration],
                    peritus_daemon::ProviderRegistryLimits::PRODUCTION,
                    Some(Arc::new(NoCredentialRead)),
                )
                .expect("production factory");
                assert_eq!(
                    registry.current_provider(id).expect("provider").profile().model().as_str(),
                    "future-model"
                );
            }
        }
    }
}
