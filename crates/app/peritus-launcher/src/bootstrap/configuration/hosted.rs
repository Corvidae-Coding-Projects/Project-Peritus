//! Named hosted provider route rendering from a selected protocol.
use super::{
    CompatibleProtocol, DirectProviderProfile, LauncherError, invalid, profile_block, toml_string,
};

pub(super) fn render(
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
    let profile_id = direct.route_identity().to_string();
    let mut text = format!(
        "\n[[providers]]\nkind = \"hosted\"\nhosted_service = {}\nwire_protocol = {}\ncredential_reference = {}\n",
        toml_string(service),
        toml_string(protocol),
        toml_string(direct.credential_reference())
    );
    let facts = direct.model_facts().ok_or_else(|| {
        invalid("hosted model capacity is unknown; reopen provider setup to resolve it")
    })?;
    text.push_str(&profile_block(
        &profile_id,
        facts,
        &[],
    ));
    Ok(text)
}
