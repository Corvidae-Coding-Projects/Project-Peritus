//! Direct-folder configuration, independent of canonical Git registrations.

use super::{toml_path, toml_string};
use crate::{AppLayout, LauncherError};
use peritus_product_state::{ProductState, WorkspaceTrust};
use std::fmt::Write as _;

pub(super) fn render(
    text: &mut String,
    layout: &AppLayout,
    state: &ProductState,
) -> Result<(), LauncherError> {
    let Some(profile) = state.workspaces().active().filter(|profile| profile.is_direct_folder())
    else {
        return Ok(());
    };
    writeln!(text, "\n[[folders]]\nworkspace_id = {}\nroot = {}\nidentity = {}\nwritable = {}\nprotected_paths = [{}, {}, {}]\n",
        toml_string(profile.workspace_id()), toml_string(profile.repository_root()),
        toml_string(profile.repository_identity()), profile.trust_level() == WorkspaceTrust::Trusted,
        toml_path(layout.config_root())?, toml_path(layout.state_root())?, toml_path(layout.cache_root())?,
    ).expect("writing to String cannot fail");
    Ok(())
}
