//! Restore one previously registered direct folder through the public bootstrap API.

use peritus_launcher::{AppLayout, ProductBootstrap};
use peritus_product_state::WorkspaceProfile;
use peritus_workspace::FolderIdentity;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let state = arguments.next().ok_or("usage: recover STATE_JSON WORKSPACE_ID")?;
    let workspace = arguments.next().ok_or("missing workspace ID")?;
    if arguments.next().is_some() {
        return Err("unexpected argument".into());
    }
    let state: serde_json::Value = serde_json::from_slice(&std::fs::read(state)?)?;
    let profile = state["workspaces"]["recent"]
        .as_array()
        .ok_or("state does not contain recent workspaces")?
        .iter()
        .find(|profile| profile["workspace_id"].as_str() == Some(workspace.as_str()))
        .ok_or("the specified workspace is not in the supplied state")?;
    let profile: WorkspaceProfile = serde_json::from_value(profile.clone())?;
    if !profile.is_direct_folder() {
        return Err("only an existing direct-folder registration can be restored".into());
    }
    let observed = FolderIdentity::observe(std::path::Path::new(profile.repository_root()))?;
    let observed_digest =
        observed.digest().as_bytes().iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    if observed_digest != profile.repository_identity() {
        return Err("folder filesystem identity changed; registration was not restored".into());
    }
    let prepared =
        ProductBootstrap::new(AppLayout::discover()?.prepare()?).configure_workspace(profile)?;
    let restored = prepared.state().workspaces().active().ok_or("no active workspace")?;
    println!(
        "restored workspace={} direct_folder={} root={}",
        restored.workspace_id(),
        restored.is_direct_folder(),
        restored.repository_root()
    );
    Ok(())
}
