//! Provider settings actions and explicit small inference checks.

use crate::{LauncherError, terminal::Terminal};
use peritus_product_state::DirectProviderProfile;

pub(super) fn existing(
    terminal: &mut Terminal<'_>,
    profile: &DirectProviderProfile,
) -> Result<DirectProviderProfile, LauncherError> {
    super::direct::ensure_runtime(terminal, profile.kind())?;
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
            terminal.line(if matches!(profile.kind(), peritus_product_state::ProviderKind::OpenCodeZen | peritus_product_state::ProviderKind::OpenCodeGo) {
                "Check the saved API key, workspace billing/model access, and OpenCode CLI version. You can replace settings or test again; a separate OpenCode login is unnecessary."
            } else {
                "Check the API key, billing/model access, and selected protocol. You can replace settings or test again."
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
