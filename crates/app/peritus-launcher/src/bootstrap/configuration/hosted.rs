//! Named hosted provider route rendering from a selected protocol.
use super::{
    CompatibleProtocol, DirectProviderProfile, LauncherError, ProfileFeatures, ProviderKind,
    invalid, profile_block, toml_string,
};
use std::fmt::Write as _;

pub(in crate::bootstrap) fn refresh_transport(
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
        "\n[[providers]]\nkind = \"hosted\"\nhosted_service = {}\nwire_protocol = {}\ncredential_reference = {}\ndiscover_model_limits = true\n",
        toml_string(service),
        toml_string(protocol),
        toml_string(direct.credential_reference())
    );
    // Token values are fallbacks for unavailable catalog fields, not ceilings on discovered
    // route/model capacities. Adapter features remain separate from numeric model metadata.
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
