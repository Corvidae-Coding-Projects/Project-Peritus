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
    // Named hosted profiles are the only routes that emit `discover_model_limits`. Check all of
    // them so HTTP-hosted routes released before that field existed also migrate.
    if !state.providers().enabled().iter().any(|kind| kind.hosted_service().is_some()) {
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
            return Err(LauncherError::filesystem("read prior hosted configuration", &path, error));
        }
    };
    // Preserve old immutable bytes. Admit only exact earlier generated hosted profiles;
    // arbitrary edits to token settings, routes, authority, or workspace paths still fail closed.
    let current = super::render_configuration(layout, state)?;
    // v0.1.8 generated hosted profiles without this field. Later releases enabled live model
    // capacity discovery, so the same durable choices need a new immutable generation.
    let without_discovery = remove_discovered_model_limits(&current);
    // Older OpenCode releases also omitted parallel tool calls from the generated capabilities.
    let without_discovery_or_parallel_tools =
        remove_opencode_parallel_tool_calls(&without_discovery);
    let legacy = [&without_discovery, &without_discovery_or_parallel_tools].into_iter().any(
        |configuration| {
            configuration.as_str() != current
                && super::compatibility::matches_current_or_legacy_timeout_configuration(
                    &actual,
                    configuration,
                )
        },
    );
    if legacy {
        state.advance_configuration_generation()?;
        store.commit(state)?;
    }
    Ok(())
}

fn remove_discovered_model_limits(configuration: &str) -> String {
    configuration
        .split("\n[[providers]]\n")
        .map(|section| {
            if section.contains("hosted_service = ") {
                section.replacen("discover_model_limits = true\n", "", 1)
            } else {
                section.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n[[providers]]\n")
}

fn remove_opencode_parallel_tool_calls(configuration: &str) -> String {
    configuration
        .split("\n[[providers]]\n")
        .map(|section| {
            if section.contains("hosted_service = \"opencode-zen\"")
                || section.contains("hosted_service = \"opencode-go\"")
            {
                section.replacen(
                    "capabilities = [\"parallel-tool-calls\", \"streaming\", \"tool-calls\", \"usage-detail\"]",
                    "capabilities = [\"streaming\", \"tool-calls\", \"usage-detail\"]",
                    1,
                )
            } else {
                section.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n[[providers]]\n")
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

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_product_state::ProviderSelection;

    #[test]
    fn migrates_v018_hosted_configuration_generation() {
        migrate_case(
            ProviderKind::OpenCodeGo,
            CompatibleProtocol::OpenCodeRuntime,
            "deepseek-v4.1-flash",
        );
        migrate_case(ProviderKind::OpenRouter, CompatibleProtocol::ChatCompletions, "vendor/model");
    }

    fn migrate_case(kind: ProviderKind, protocol: CompatibleProtocol, model: &str) {
        let root = tempfile::tempdir().expect("temporary application root");
        let layout = crate::AppLayout::for_test(
            root.path().join("config"),
            root.path().join("state"),
            root.path().join("cache"),
        )
        .prepare()
        .expect("prepare isolated roots");
        crate::ProductBootstrap::new(layout.clone()).prepare().expect("initialize product state");
        let profile = DirectProviderProfile::new(
            kind,
            "peritus-secret-v1:00000000000000000000000000000000:0123456789abcdef".to_owned(),
            None,
            model.to_owned(),
            Some(protocol),
            None,
        )
        .expect("valid hosted profile");
        let selection =
            ProviderSelection::with_direct_profiles(vec![kind], Some(kind), vec![profile])
                .expect("valid hosted selection");
        let prepared = crate::ProductBootstrap::new(layout.clone())
            .configure_providers(selection)
            .expect("publish current hosted configuration");
        let previous_path = prepared.daemon_config_path();
        let previous_generation = prepared.state().generation();
        let current = std::fs::read_to_string(&previous_path).expect("read current configuration");
        assert!(current.contains("discover_model_limits = true"));

        // Simulate the exact hosted bytes generated by v0.1.8 before live capacity discovery.
        let legacy = current.replacen("discover_model_limits = true\n", "", 1);
        std::fs::write(&previous_path, &legacy).expect("write legacy configuration");

        let migrated = crate::ProductBootstrap::new(layout.clone())
            .prepare()
            .expect("migrate unchanged prior hosted configuration");
        assert_eq!(migrated.state().generation(), previous_generation + 1);
        assert_ne!(migrated.daemon_config_path(), previous_path);
        let migrated_text = std::fs::read_to_string(migrated.daemon_config_path())
            .expect("read migrated configuration");
        assert!(migrated_text.contains("discover_model_limits = true"));
        assert_eq!(
            std::fs::read_to_string(&previous_path).expect("read preserved prior configuration"),
            legacy
        );

        // Arbitrary token edits remain outside the admitted legacy shape.
        let custom =
            migrated_text.replacen("max_input_tokens = 32768", "max_input_tokens = 32769", 1);
        std::fs::write(migrated.daemon_config_path(), custom).expect("write custom configuration");
        let error = match crate::ProductBootstrap::new(layout).prepare() {
            Ok(_) => panic!("custom configuration must fail closed"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("has different content"));
    }
}
