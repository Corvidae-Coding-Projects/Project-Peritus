//! Native TUI consumer with explicit fixture launch facts, avoiding interactive account setup.
use peritus_launcher::{AppLayout, ProductBootstrap};
use peritus_tui::{ProductLaunchContext, ProductProviderOption, TuiConfig};
use peritus_types::WorkspaceId;
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let prepared = ProductBootstrap::new(AppLayout::discover()?.prepare()?).prepare()?;
    let state = prepared.state();
    let workspace = state.workspaces().active().ok_or("no fixture workspace")?;
    let mut bytes = [0; 16];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&workspace.workspace_id()[i * 2..i * 2 + 2], 16)?;
    }
    let providers = state
        .providers()
        .enabled()
        .iter()
        .map(|kind| {
            peritus_types::ProviderProfileId::new(kind.profile_identity())
                .map(|id| ProductProviderOption::new(id, kind.label()))
                .map_err(|e| format!("{e:?}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let default = state
        .providers()
        .default()
        .and_then(|kind| state.providers().enabled().iter().position(|value| *value == kind));
    let mut context = ProductLaunchContext::new(
        WorkspaceId::new(bytes).map_err(|e| format!("{e:?}"))?,
        workspace.repository_root().to_owned(),
        providers,
        default,
    )?
    .with_direct_folder(false);
    if std::env::args().any(|value| value == "--resume") {
        context = context.with_latest_conversation();
    }
    peritus_tui::run(TuiConfig::new(prepared.endpoint_path()).with_product(context)).await?;
    Ok(())
}
