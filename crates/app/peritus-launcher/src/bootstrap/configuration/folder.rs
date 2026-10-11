//! Direct-folder configuration, independent of canonical Git registrations.

use super::{toml_path, toml_string};
use crate::{AppLayout, LauncherError};
use peritus_product_state::{ProductState, WorkspaceProfile, WorkspaceTrust};
use std::{fmt::Write as _, path::Path};

pub(super) fn render(
    text: &mut String,
    layout: &AppLayout,
    state: &ProductState,
) -> Result<(), LauncherError> {
    let active = state.workspaces().active().map(WorkspaceProfile::workspace_id);
    for profile in state.workspaces().recent().iter().filter(|profile| profile.is_direct_folder()) {
        // Remembered folders retain only their original identity and trust. An
        // unavailable inactive folder must not prevent launching the chosen one.
        if Some(profile.workspace_id()) != active && !available(profile) {
            continue;
        }
        writeln!(text, "\n[[folders]]\nworkspace_id = {}\nroot = {}\nidentity = {}\nwritable = {}\nprotected_paths = [{}, {}, {}]\n",
            toml_string(profile.workspace_id()), toml_string(profile.repository_root()),
            toml_string(profile.repository_identity()), profile.trust_level() == WorkspaceTrust::Trusted,
            toml_path(layout.config_root())?, toml_path(layout.state_root())?, toml_path(layout.cache_root())?,
        ).expect("writing to String cannot fail");
    }
    Ok(())
}

fn available(profile: &WorkspaceProfile) -> bool {
    let root = Path::new(profile.repository_root());
    let Ok(observed) = peritus_workspace::FolderIdentity::observe(root) else {
        return false;
    };
    let digest = observed.digest().as_bytes().iter().fold(String::new(), |mut text, byte| {
        write!(text, "{byte:02x}").expect("writing to String cannot fail");
        text
    });
    observed.root() == root && digest == profile.repository_identity()
}
