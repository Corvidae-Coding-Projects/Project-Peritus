//! Coherent immutable product/configuration discovery and retained native ownership.

mod owner;
mod persistence;

pub(crate) use owner::{
    NativeConnection, NativeOwner, NativeTarget, connect_owned, raw_request_owned,
    raw_request_target,
};

use super::{App, Project, Result, bytes, hex, json};
use crate::error::problem;
use peritus_app_protocol::WellKnownProtocolFeature;
use peritus_product_state::{ProductState, WorkspaceProfile};
use peritus_types::WorkspaceId;
use serde_json::Value;
use std::path::Path;

pub(super) struct Discovery {
    pub(super) target: NativeTarget,
    pub(super) configuration: toml::Value,
    pub(super) state: ProductState,
}

pub(crate) fn current_target(app: &App) -> Result<NativeTarget> {
    persistence::discover(app).map(|discovery| discovery.target)
}

pub(crate) fn current_facts(app: &App, project: &Project) -> Result<Value> {
    let discovery = persistence::discover(app)?;
    facts_from_discovery(&discovery, project)
}

async fn discover_async(app: &App) -> Result<Discovery> {
    let options = app.options.clone();
    tokio::task::spawn_blocking(move || persistence::discover_options(&options)).await.map_err(problem)?
}

pub(crate) async fn current_target_async(app: &App) -> Result<NativeTarget> {
    discover_async(app).await.map(|discovery| discovery.target)
}

pub(crate) async fn current_facts_async(app: &App, project: &Project) -> Result<Value> {
    let discovery = discover_async(app).await?;
    let project = project.clone();
    tokio::task::spawn_blocking(move || facts_from_discovery(&discovery, &project)).await.map_err(problem)?
}

pub(crate) async fn owner_for_project(
    app: &App,
    project: &Project,
) -> Result<(NativeOwner, Value)> {
    let discovery = discover_async(app).await?;
    let project = project.clone();
    let (discovery, facts) = tokio::task::spawn_blocking(move || -> Result<(Discovery, Value)> {
        let facts = facts_from_discovery(&discovery, &project)?;
        Ok((discovery, facts))
    }).await.map_err(problem)??;
    let workspace = WorkspaceId::new(bytes(
        facts["workspace"]["id"]
            .as_str()
            .ok_or_else(|| problem("The project has no retained native workspace"))?,
    )?)
    .map_err(|error| problem(format!("{error:?}")))?;
    let required: Vec<WellKnownProtocolFeature> = Vec::new();
    let client = super::connection::connect(
        app,
        discovery.target.endpoint(),
        &required,
        None,
    )
    .await?;
    let connection = NativeConnection::new(
        discovery.target,
        hex(client.context().session_id().as_bytes()),
        discovery.state,
    );
    Ok((connection.bind(workspace)?, facts))
}

fn facts_from_discovery(discovery: &Discovery, project: &Project) -> Result<Value> {
    let project_root = project.root.canonicalize()?;
    let repository = project.repository.canonicalize()?;
    let profile = profiles(&discovery.state).find(|profile| {
        Path::new(profile.repository_root())
            .canonicalize()
            .ok()
            .is_some_and(|root| root == project_root || root == repository)
    });
    let providers = discovery
        .configuration
        .get("providers")
        .and_then(toml::Value::as_array)
        .map(|providers| {
            providers
                .iter()
                .map(|provider| {
                    json!({
                        "id":provider
                            .get("profile")
                            .and_then(|value| value.get("profile_id"))
                            .and_then(toml::Value::as_str),
                        "kind":provider.get("kind").and_then(toml::Value::as_str),
                        "model":provider
                            .get("profile")
                            .and_then(|value| value.get("model"))
                            .and_then(toml::Value::as_str)
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(json!({
        "providers":providers,
        "workspace":profile.map(profile_value),
        "endpoint":discovery.target.endpoint,
        "nativeTarget":{
            "generation":discovery.target.generation.to_string(),
            "store":discovery.target.store,
            "config":discovery.target.config,
            "productState":discovery.target.product_state
        }
    }))
}

fn profile_value(profile: &WorkspaceProfile) -> Value {
    json!({
        "id":profile.workspace_id(),
        "root":profile.repository_root(),
        "execution":profile.managed_root().unwrap_or_else(|| profile.repository_root()),
        "trust":format!("{:?}", profile.trust_level()).to_lowercase()
    })
}

pub(super) fn profiles(state: &ProductState) -> impl Iterator<Item = &WorkspaceProfile> {
    state.workspaces().profiles().into_iter()
}

pub(super) fn profile_for_workspace<'a>(
    state: &'a ProductState,
    workspace: &str,
) -> Option<&'a WorkspaceProfile> {
    state.workspaces().find(workspace)
}
