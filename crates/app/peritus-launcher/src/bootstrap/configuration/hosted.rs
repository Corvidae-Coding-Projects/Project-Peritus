//! Named hosted provider route rendering from a selected protocol.
use super::{
    CompatibleProtocol, DirectProviderProfile, LauncherError, ProfileFeatures, ProviderKind,
    invalid, profile_block, toml_string,
};
use std::fmt::Write as _;

pub(in crate::bootstrap) fn refresh_transport(
    layout: &crate::AppLayout,
    store: &crate::persistence::ProductStateStore,
    state: &mut peritus_product_state::ProductState,
) -> Result<(), LauncherError> {
    if !state
        .providers()
        .enabled()
        .iter()
        .any(|kind| matches!(kind, ProviderKind::OpenCodeZen | ProviderKind::OpenCodeGo))
    {
        return Ok(());
    }
    let mut selection = state.providers().clone();
    if selection.migrate_opencode_runtime() && state.configure_providers(selection) {
        store.commit(state)?;
    }
    let path = layout.daemon_config(state.generation());
    let actual = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(LauncherError::filesystem(
                "read prior OpenCode configuration",
                &path,
                error,
            ));
        }
    };
    // Preserve old immutable bytes. Admit only this exact earlier generated native profile;
    // arbitrary edits to other routes, authority, or workspace paths still fail closed.
    let current = super::render_configuration(layout, state)?;
    let legacy = current.split("\n[[providers]]\n").map(|section| {
        if section.contains("hosted_service = \"opencode-zen\"") || section.contains("hosted_service = \"opencode-go\"") {
            section.replacen("capabilities = [\"parallel-tool-calls\", \"streaming\", \"tool-calls\", \"usage-detail\"]",
                "capabilities = [\"streaming\", \"tool-calls\", \"usage-detail\"]", 1)
        } else { section.to_owned() }
    }).collect::<Vec<_>>().join("\n[[providers]]\n");
    if current != legacy
        && super::compatibility::matches_current_or_legacy_timeout_configuration(&actual, &legacy)
    {
        state.advance_configuration_generation()?;
        store.commit(state)?;
    }
    Ok(())
}

pub(super) fn render(
    provider: ProviderKind,
    direct: &DirectProviderProfile,
    service: &str,
) -> Result<String, LauncherError> {
    let opencode = matches!(provider, ProviderKind::OpenCodeZen | ProviderKind::OpenCodeGo);
    let protocol = if opencode {
        "opencode-runtime"
    } else {
        match direct.compatible_protocol() {
            Some(CompatibleProtocol::Responses) => "open-ai",
            Some(CompatibleProtocol::ChatCompletions) => "compatible-chat-completions",
            Some(CompatibleProtocol::AnthropicMessages) => "anthropic",
            Some(CompatibleProtocol::GoogleGenerateContent) => "google-generate-content",
            Some(CompatibleProtocol::OpenCodeRuntime) => {
                return Err(invalid("OpenCode runtime cannot route another hosted service"));
            }
            None => {
                return Err(invalid(
                    "hosted provider has no discovered or explicitly selected protocol",
                ));
            }
        }
    };
    let mut profile_id = String::with_capacity(32);
    for byte in provider.profile_identity() {
        write!(profile_id, "{byte:02x}")
            .map_err(|_| invalid("profile identity formatting failed"))?;
    }
    let mut text = format!(
        "\n[[providers]]\nkind = \"hosted\"\nhosted_service = {}\nwire_protocol = {}\ncredential_reference = {}\n",
        toml_string(service),
        toml_string(protocol),
        toml_string(direct.credential_reference())
    );
    // These conservative request ceilings and required adapter features are not live capability
    // claims. The UI reports catalog metadata and explicit connection tests independently.
    text.push_str(&profile_block(
        &profile_id,
        direct.model(),
        32_768,
        4_096,
        ProfileFeatures::new(
            if opencode {
                vec!["parallel-tool-calls", "streaming", "tool-calls", "usage-detail"]
            } else {
                vec!["streaming", "tool-calls", "usage-detail", "reasoning-replay"]
            },
            false,
        ),
    ));
    Ok(text)
}
