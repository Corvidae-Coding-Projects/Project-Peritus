//! Switch presentation scope without restarting the daemon or changing workspace authority.

use super::{diagnostics, workspace_context};
use crate::{LauncherError, PreparedProduct, SiblingBinaries};
use peritus_app_protocol::WorkbenchQuery;
use peritus_tui::ProductLaunchContext;
use peritus_types::{RunId, WorkspaceId};

pub(super) fn run_context(
    prepared: &PreparedProduct,
    binaries: &SiblingBinaries,
    run: RunId,
    workspace: WorkspaceId,
) -> Result<ProductLaunchContext, LauncherError> {
    let context = configured_context(prepared, workspace)?.with_run(Some(run));
    let report = diagnostics::launcher_report(prepared, binaries, workspace)?;
    context.with_launcher_report(report).map_err(LauncherError::Tui)
}

pub(super) fn conversation_context(
    prepared: &PreparedProduct,
    binaries: &SiblingBinaries,
    query: WorkbenchQuery,
) -> Result<ProductLaunchContext, LauncherError> {
    let context = registered_context(prepared, query)?;
    let report = diagnostics::launcher_report(prepared, binaries, query.workspace())?;
    context.with_launcher_report(report).map_err(LauncherError::Tui)
}

fn registered_context(
    prepared: &PreparedProduct,
    query: WorkbenchQuery,
) -> Result<ProductLaunchContext, LauncherError> {
    configured_context(prepared, query.workspace())?
        .with_conversation(query)
        .map_err(LauncherError::Tui)
}

fn configured_context(
    prepared: &PreparedProduct,
    workspace: WorkspaceId,
) -> Result<ProductLaunchContext, LauncherError> {
    use std::fmt::Write as _;
    let mut workspace_id = String::with_capacity(32);
    for byte in workspace.as_bytes() {
        write!(workspace_id, "{byte:02x}").expect("writing to String cannot fail");
    }
    let workspace = prepared
        .state()
        .workspaces()
        .find(&workspace_id)
        .filter(|profile| {
            profile.is_direct_folder()
                || profile.trust_level() == peritus_product_state::WorkspaceTrust::Trusted
        })
        .ok_or_else(|| LauncherError::WorkspaceSetup(format!(
            "workspace {workspace_id} is not an authorized target in this launcher's durable registry. Reopen Peritus after configuring this workspace."
        )))?;
    workspace_context(prepared, workspace)
}

#[cfg(test)]
mod tests;
