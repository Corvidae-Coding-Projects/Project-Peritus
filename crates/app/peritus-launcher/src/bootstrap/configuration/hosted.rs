//! Named hosted provider route rendering from a selected protocol.
use super::{
    CompatibleProtocol, DirectProviderProfile, LauncherError, ProfileFeatures, ProviderKind,
    invalid, profile_block, toml_string,
};
use std::fmt::Write as _;

pub(super) fn render(
    provider: ProviderKind,
    direct: &DirectProviderProfile,
    service: &str,
) -> Result<String, LauncherError> {
    let protocol = match direct.compatible_protocol() {
        Some(CompatibleProtocol::Responses) => "open-ai",
        Some(CompatibleProtocol::ChatCompletions) => "compatible-chat-completions",
        Some(CompatibleProtocol::AnthropicMessages) => "anthropic",
        Some(CompatibleProtocol::GoogleGenerateContent) => "google-generate-content",
        None => {
            return Err(invalid(
                "hosted provider has no discovered or explicitly selected protocol",
            ));
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
            vec!["streaming", "tool-calls", "usage-detail", "reasoning-replay"],
            false,
        ),
    ));
    Ok(text)
}
