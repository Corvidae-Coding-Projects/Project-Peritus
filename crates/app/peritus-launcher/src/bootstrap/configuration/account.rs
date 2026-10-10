//! Reconcile official-client updates before immutable configuration publication.

use crate::{AppLayout, LauncherError, persistence::ProductStateStore};
use peritus_product_state::{ProductState, ProviderKind};
use peritus_provider_onboarding::AccountProvider;
use std::collections::BTreeMap;

pub(in crate::bootstrap) fn refresh_executables(
    layout: &AppLayout,
    store: &ProductStateStore,
    state: &mut ProductState,
) -> Result<(), LauncherError> {
    let accounts: Vec<_> =
        state.providers().enabled().iter().copied().filter(|kind| kind.is_account()).collect();
    if accounts.is_empty() {
        return Ok(());
    }
    let prior = read_prior(layout, state)?;
    let mut paths = BTreeMap::new();
    for kind in accounts {
        let discovered = AccountProvider::discover(kind).ok();
        let path = if let Some(account) = &discovered {
            Some(
                account
                    .executable()
                    .to_str()
                    .ok_or_else(|| super::invalid("account executable path is not UTF-8"))?,
            )
        } else {
            state
                .providers()
                .account_executable(kind)
                .or_else(|| prior.get(&kind).map(String::as_str))
        };
        if let Some(path) = path {
            paths.insert(kind, path.to_owned());
        }
    }
    reconcile(store, state, paths)
}

fn reconcile(
    store: &ProductStateStore,
    state: &mut ProductState,
    paths: BTreeMap<ProviderKind, String>,
) -> Result<(), LauncherError> {
    let selection = state.providers().clone().with_account_executables(paths)?;
    if state.configure_providers(selection) {
        store.commit(state)?;
    }
    Ok(())
}

fn read_prior(
    layout: &AppLayout,
    state: &ProductState,
) -> Result<BTreeMap<ProviderKind, String>, LauncherError> {
    let path = layout.daemon_config(state.generation());
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => {
            return Err(LauncherError::filesystem("read prior account configuration", path, error));
        }
    };
    let configuration: toml::Value = toml::from_str(&text)
        .map_err(|_| super::invalid("prior account configuration is malformed"))?;
    let mut paths = BTreeMap::new();
    for route in
        configuration.get("providers").and_then(toml::Value::as_array).into_iter().flatten()
    {
        let kind = match route.get("kind").and_then(toml::Value::as_str) {
            Some("codex-runtime") => ProviderKind::CodexAccount,
            Some("claude-runtime") => ProviderKind::ClaudeAccount,
            _ => continue,
        };
        if let Some(path) = route.get("executable") {
            let path = path
                .as_str()
                .ok_or_else(|| super::invalid("prior account executable is not a path"))?;
            paths.insert(kind, path.to_owned());
        }
    }
    Ok(paths)
}
