//! Ordinary public launcher consumer, using real immutable files and bootstrap locking.
use peritus_launcher::{AppLayout, ProductBootstrap};
use peritus_product_state::{
    CompatibleProtocol, DirectProviderProfile, ProviderKind, ProviderSelection, WorkspaceProfile,
};
use std::{collections::BTreeMap, error::Error};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    let layout = AppLayout::discover()?.prepare()?;
    let mut product = if args[1] == "seed" {
        let enabled: Vec<ProviderKind> = serde_json::from_str(&args[2])?;
        let mut profiles = Vec::new();
        let mut models = BTreeMap::new();
        for kind in &enabled {
            if kind.is_account() {
                models.insert(*kind, "fixture-account-model".to_owned());
                continue;
            }
            let endpoint = match kind {
                ProviderKind::AnthropicApi => Some("https://api.anthropic.com".to_owned()),
                ProviderKind::GoogleGeminiApi => {
                    Some("https://generativelanguage.googleapis.com".to_owned())
                }
                ProviderKind::CompatibleEndpoint => Some("https://example.invalid/v1".to_owned()),
                _ => None,
            };
            let protocol = match kind {
                ProviderKind::OpenCodeZen | ProviderKind::OpenCodeGo => {
                    Some(CompatibleProtocol::OpenCodeRuntime)
                }
                ProviderKind::CompatibleEndpoint => Some(CompatibleProtocol::ChatCompletions),
                _ if kind.hosted_service().is_some() => Some(CompatibleProtocol::ChatCompletions),
                _ => None,
            };
            profiles.push(DirectProviderProfile::new(
                *kind,
                format!(
                    "peritus-secret-v1:{:032x}:{:064x}",
                    enabled.iter().position(|value| value == kind).unwrap() + 1,
                    33
                ),
                endpoint,
                "fixture-route-model".to_owned(),
                protocol,
                None,
            )?);
        }
        let selection = ProviderSelection::with_direct_profiles_and_failover(
            enabled.clone(),
            enabled.first().copied(),
            profiles,
            enabled.len() > 1,
        )?
        .with_account_models(models)?;
        ProductBootstrap::new(layout.clone()).configure_providers(selection)?
    } else {
        ProductBootstrap::new(layout.clone()).prepare()?
    };
    if args[1] == "seed" {
        for number in 1..=2 {
            let root = layout.cache_root().join(format!("workspace-{number}"));
            std::fs::create_dir_all(&root)?;
            let observed = peritus_workspace::FolderIdentity::observe(&root)?;
            let digest = observed
                .digest()
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let profile = WorkspaceProfile::restricted(
                root.to_str().ok_or("non-UTF-8 fixture root")?.to_owned(),
                digest,
                format!("{number:032x}"),
                format!("{:032x}", number + 2),
                format!("{:032x}", number + 4),
                format!("{:032x}", number + 6),
            )?
            .into_direct_folder()?;
            product = ProductBootstrap::new(layout.clone()).configure_workspace(profile)?;
        }
    }
    println!(
        "{}",
        serde_json::json!({
            "state": serde_json::from_slice::<serde_json::Value>(&product.state().canonical_json()?)?,
            "configuration": product.daemon_config_path(),
            "endpoint": product.endpoint_path(),
        })
    );
    Ok(())
}
