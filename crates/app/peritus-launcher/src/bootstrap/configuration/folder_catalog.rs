//! Reconcile only durable direct-folder declarations into a new immutable generation.

use super::{compatibility, render_configuration};
use crate::{AppLayout, LauncherError, persistence::ProductStateStore};
use peritus_daemon::DaemonConfig;
use peritus_product_state::{ProductState, WorkspaceProfile, WorkspaceTrust};
use std::{collections::BTreeSet, fs, path::Path};

const FOLDER_BOUNDARY: &str = "\n[[folders]]\n";

/// Called only while the bootstrap lock owns state/configuration publication.
pub(in crate::bootstrap) fn refresh(
    layout: &AppLayout,
    store: &ProductStateStore,
    state: &mut ProductState,
) -> Result<(), LauncherError> {
    let path = layout.daemon_config(state.generation());
    let actual = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(LauncherError::filesystem("read folder catalog configuration", path, error));
        }
    };
    let expected = render_configuration(layout, state)?;
    if compatibility::matches_current_or_legacy_timeout_configuration(&actual, &expected) {
        return Ok(());
    }
    // No semantic normalization is allowed for any non-folder configuration byte.
    // Returning without migration retains ensure_configuration's strict rejection.
    let Ok(actual_text) = std::str::from_utf8(&actual) else { return Ok(()) };
    let (actual_prefix, catalog) = split_catalog(actual_text);
    let (expected_prefix, _) = split_catalog(&expected);
    if !compatibility::matches_current_or_legacy_timeout_configuration(
        actual_prefix.as_bytes(),
        expected_prefix,
    ) || !matches_durable_catalog(layout, state, catalog)?
    {
        return Ok(());
    }
    // Validate the desired catalog before making a new state generation durable.
    let _ = DaemonConfig::parse(&expected)?;
    let mut next = state.clone();
    next.advance_configuration_generation()?;
    store.commit(&next)?;
    *state = next;
    // finish publishes the new configuration after this commit. If interrupted,
    // prepare resumes from the new state without overwriting an earlier file.
    Ok(())
}

fn split_catalog(text: &str) -> (&str, Option<&str>) {
    match text.find(FOLDER_BOUNDARY) {
        Some(offset) => (&text[..offset], Some(&text[offset + 1..])),
        None => (text, None),
    }
}

fn matches_durable_catalog(
    layout: &AppLayout,
    state: &ProductState,
    catalog: Option<&str>,
) -> Result<bool, LauncherError> {
    let mut seen = BTreeSet::new();
    if let Some(catalog) = catalog {
        let Ok(value) = toml::from_str::<toml::Value>(catalog) else { return Ok(false) };
        let Some(table) = value.as_table() else { return Ok(false) };
        // A provider, tool, identity, or any other table cannot hide in the suffix.
        if table.len() != 1 {
            return Ok(false);
        }
        let Some(folders) = table.get("folders").and_then(toml::Value::as_array) else {
            return Ok(false);
        };
        for folder in folders {
            let Some(id) = folder.get("workspace_id").and_then(toml::Value::as_str) else {
                return Ok(false);
            };
            let Some(profile) = state
                .workspaces()
                .recent()
                .iter()
                .find(|profile| profile.is_direct_folder() && profile.workspace_id() == id)
            else {
                return Ok(false);
            };
            if !seen.insert(profile.workspace_id()) || *folder != declaration(layout, profile)? {
                return Ok(false);
            }
        }
    }
    // Earlier active-only catalogs and later available-recent catalogs both
    // declare the chosen folder. Inactive entries may disappear and reappear,
    // but only with their originally remembered identity and authority.
    Ok(state
        .workspaces()
        .active()
        .filter(|profile| profile.is_direct_folder())
        .is_none_or(|profile| seen.contains(profile.workspace_id())))
}

fn declaration(
    layout: &AppLayout,
    profile: &WorkspaceProfile,
) -> Result<toml::Value, LauncherError> {
    let mut value = toml::Table::new();
    value.insert("workspace_id".to_owned(), toml::Value::String(profile.workspace_id().to_owned()));
    value.insert("root".to_owned(), toml::Value::String(profile.repository_root().to_owned()));
    value.insert(
        "identity".to_owned(),
        toml::Value::String(profile.repository_identity().to_owned()),
    );
    value.insert(
        "writable".to_owned(),
        toml::Value::Boolean(profile.trust_level() == WorkspaceTrust::Trusted),
    );
    let protected = [layout.config_root(), layout.state_root(), layout.cache_root()]
        .into_iter()
        .map(path_value)
        .collect::<Result<Vec<_>, _>>()?;
    value.insert("protected_paths".to_owned(), toml::Value::Array(protected));
    Ok(toml::Value::Table(value))
}

fn path_value(path: &Path) -> Result<toml::Value, LauncherError> {
    path.to_str()
        .map(|path| toml::Value::String(path.to_owned()))
        .ok_or_else(|| super::invalid("protected application path is not UTF-8"))
}
