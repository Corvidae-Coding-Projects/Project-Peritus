//! Provider settings actions and explicit small inference checks.

use crate::{LauncherError, terminal::Terminal};
use peritus_product_state::DirectProviderProfile;

pub(super) fn existing(
    terminal: &mut Terminal<'_>,
    profile: &DirectProviderProfile,
    oauth_root: &std::path::Path,
) -> Result<DirectProviderProfile, LauncherError> {
    if profile.kind() == peritus_product_state::ProviderKind::ChatGptOauth {
        terminal.line(
            "ChatGPT direct: r selects or reauthorizes an account; s signs out this registration.",
        )?;
    }
    loop {
        let allowance = if profile.kind() == peritus_product_state::ProviderKind::ChatGptOauth {
            "ChatGPT plan allowance"
        } else {
            "paid tokens"
        };
        let answer = terminal.prompt(&format!("{} / {}: Enter to keep, m to change model using saved credentials, r to change account/settings, t to test connection (up to 3 small requests; uses {allowance}): ", profile.kind().label(), profile.model()))?;
        match answer.to_ascii_lowercase().as_str() {
            "" => return Ok(profile.clone()),
            "r" if profile.kind() == peritus_product_state::ProviderKind::ChatGptOauth => {
                return super::chatgpt::setup(terminal, oauth_root);
            }
            "s" if profile.kind() == peritus_product_state::ProviderKind::ChatGptOauth => {
                super::chatgpt::sign_out(terminal, profile)?;
            }
            "r" => return super::direct::setup(terminal, profile.kind()),
            "m" => {
                terminal.line("Discovering models with the saved credential; no key replacement or inference is requested.")?;
                let discovered = peritus_provider_onboarding::discover_direct_models(profile);
                let (model, protocol) = super::models::choose_direct_with_default(
                    terminal,
                    profile.kind(),
                    discovered,
                    Some(profile),
                )?;
                return Ok(DirectProviderProfile::new(
                    profile.kind(),
                    profile.credential_reference().to_owned(),
                    profile.endpoint().map(str::to_owned),
                    model,
                    protocol.or_else(|| profile.compatible_protocol()),
                    profile.credential_header().map(str::to_owned),
                )?);
            }
            "t" => {
                super::direct::ensure_runtime(terminal, profile.kind())?;
                test(terminal, profile)?;
            }
            _ => terminal.line("Press Enter, m, r, or t.")?,
        }
    }
}

pub(super) fn offer(
    terminal: &mut Terminal<'_>,
    profile: &DirectProviderProfile,
) -> Result<(), LauncherError> {
    let prompt = if profile.kind() == peritus_product_state::ProviderKind::ChatGptOauth {
        "Test generation and tool calling now? Up to 3 small requests use ChatGPT plan allowance. [y/N]: "
    } else {
        "Test generation and tool calling now? Up to 3 small requests may use paid tokens. [y/N]: "
    };
    if terminal.confirm(prompt, false)? {
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
            terminal.line(if matches!(profile.kind(), peritus_product_state::ProviderKind::OpenCodeZen | peritus_product_state::ProviderKind::OpenCodeGo) {
                "Check the saved API key, workspace billing/model access, and OpenCode CLI version. You can replace settings or test again; a separate OpenCode login is unnecessary."
            } else {
                "Check authentication, billing/model access, and selected protocol. You can replace settings or test again."
            })?;
            if profile.kind() == peritus_product_state::ProviderKind::CompatibleEndpoint {
                let route = match profile.compatible_protocol() {
                    Some(peritus_product_state::CompatibleProtocol::Responses) => "/v1/responses",
                    Some(peritus_product_state::CompatibleProtocol::ChatCompletions) => {
                        "/v1/chat/completions"
                    }
                    _ => "the selected protocol operation path",
                };
                terminal.line(&format!(
                    "Peritus posts to the exact configured URL. Verify that it includes {route}, rather than only the API base URL."
                ))?;
            }
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
