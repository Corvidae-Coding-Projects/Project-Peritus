//! Native conversation adapter over the existing negotiated application client.

use crate::{
    error::{Result, problem},
    state::{App, Project, hex},
};
use peritus_app_client::Client;
use peritus_app_protocol::{
    AppRequestPayload, AppResponsePayload, ProductInteractionMode, ProductInteractionRequest,
    ProductModelChoice, ProductModelEffort, ProductModelQuery, ProductProviderSelection,
    ProductRoleModels, ProductRunControl, ProductRunControlAction, ProductRunConversationQuery,
    ProductRunQuery, ProductRunRequest, ProductRunSnapshot,
};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub fn bytes(text: &str) -> Result<[u8; 16]> {
    if text.len() != 32 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(problem("Expected a 32-digit identity"));
    }
    let mut bytes = [0; 16];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).map_err(problem)?;
    }
    Ok(bytes)
}
fn latest(root: &Path, prefix: &str, extension: &str) -> Result<PathBuf> {
    let mut paths = std::fs::read_dir(root)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|path| {
            path.file_name().is_some_and(|name| name.to_string_lossy().starts_with(prefix))
                && path.extension().is_some_and(|ext| ext == extension)
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths.pop().ok_or_else(|| problem("Peritus has not been configured. Open Setup console to configure a provider and workspace."))
}
fn configuration(app: &App) -> Result<toml::Value> {
    let path = match &app.options.daemon_config {
        Some(path) => path.clone(),
        None => latest(&app.options.daemon_config_root, "peritus-", "toml")?,
    };
    toml::from_str(&std::fs::read_to_string(path)?).map_err(problem)
}
pub fn endpoint(app: &App) -> Result<PathBuf> {
    if let Some(path) = &app.options.endpoint {
        return Ok(path.clone());
    }
    let config = configuration(app)?;
    let store =
        config["store_id"].as_str().ok_or_else(|| problem("Daemon store identity unavailable"))?;
    let mut hash = Sha256::new();
    hash.update(b"peritus/daemon-endpoint/v1\0");
    hash.update(bytes(store)?);
    let name = format!("peritus-{}", hex(&hash.finalize()[..16]));
    #[cfg(unix)]
    {
        let root = config
            .get("paths")
            .and_then(|v| v.get("state_root"))
            .and_then(toml::Value::as_str)
            .ok_or_else(|| problem("Daemon state directory unavailable"))?;
        peritus_local_socket::bounded_path(
            &PathBuf::from(root).join(format!("{name}.sock")),
            peritus_local_socket::NATIVE_MAX_PATH_BYTES,
        )
        .map_err(problem)
    }
    #[cfg(windows)]
    {
        Ok(PathBuf::from(format!(r"\\.\pipe\{name}")))
    }
}
pub fn facts(app: &App, project: &Project) -> Result<Value> {
    let config = configuration(app)?;
    let providers = config
        .get("providers")
        .and_then(toml::Value::as_array)
        .map(|providers| {
            providers.iter().map(|p| json!({
        "id":p.get("profile").and_then(|v|v.get("profile_id")).and_then(toml::Value::as_str),
        "kind":p.get("kind").and_then(toml::Value::as_str),
        "model":p.get("profile").and_then(|v|v.get("model")).and_then(toml::Value::as_str)
    })).collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let state: Value = serde_json::from_slice(&std::fs::read(latest(
        &app.options.product_state_root,
        "state-",
        "json",
    )?)?)?;
    let workspace = state["workspaces"]["recent"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(state["workspaces"]["retained_registrations"].as_array().into_iter().flatten())
        .find(|w| {
            w["repository_root"].as_str().is_some_and(|root| {
                Path::new(root)
                    .canonicalize()
                    .ok()
                    .is_some_and(|root| root == project.root || root == project.repository)
            })
        });
    Ok(
        json!({"providers":providers,"workspace":workspace.map(|w|json!({"id":w["workspace_id"],"root":w["repository_root"],"execution":w["managed_root"].as_str().unwrap_or_else(|| w["repository_root"].as_str().unwrap_or("")),"trust":w["trust"]})),"endpoint":endpoint(app)?}),
    )
}

mod chat;
pub mod improvements;
mod readiness;
pub mod receipts;
#[cfg(all(test, unix))]
mod tests;
pub use readiness::ready_facts;
async fn request(app: &App, payload: AppRequestPayload) -> Result<AppResponsePayload> {
    let response = raw_request(app, payload).await?;
    if let AppResponsePayload::Error(error) = &response {
        return Err(problem(format!("Daemon rejected the request: {error}")));
    }
    Ok(response)
}
pub async fn raw_request(app: &App, payload: AppRequestPayload) -> Result<AppResponsePayload> {
    let endpoint = endpoint(app)?;
    let required = payload.required_workbench_feature().into_iter().collect::<Vec<_>>();
    let mut client =
        Client::connect(endpoint.as_os_str(), None, Duration::from_secs(30), &required)
            .await
            .map_err(problem)?;
    let identity = Client::new_request_identity().map_err(problem)?;
    let response = client.request(identity, payload).await.map_err(problem)?;
    Ok(response.payload().clone())
}
pub async fn status(app: &App) -> Result<Value> {
    match request(app, AppRequestPayload::DaemonStatus).await? {
        AppResponsePayload::DaemonStatus(status) => Ok(
            json!({"connected":true,"ready":status.mutation_ready(),"readiness":format!("{:?}",status.readiness()),"diagnostic":status.diagnostic(),"detail":format!("{status:?}")}),
        ),
        _ => Err(problem("Unexpected daemon status response")),
    }
}
fn snapshot(value: &ProductRunSnapshot) -> Value {
    json!({"id":hex(value.run_id().as_bytes()),"workspace":hex(value.workspace_id().as_bytes()),"providers":{"writer":hex(value.providers().writer().as_bytes()),"reviewer":hex(value.providers().reviewer().as_bytes()),"fixer":hex(value.providers().fixer().as_bytes())},"phase":format!("{:?}",value.phase()),"busy":!value.phase().terminal(),"task":value.task(),"status":value.status(),"diff":value.diff(),"gates":value.gates(),"review":value.review(),"summary":value.summary(),"deliverable":value.deliverable().map(|d|json!({"root":d.workspace_path(),"paths":d.changed_paths(),"instructions":d.run_instructions(),"qualification":format!("{:?}",d.qualification())}))})
}
fn model_values(models: &ProductRoleModels) -> Value {
    let choice = |value: &ProductModelChoice| json!({"id":value.id(),"manual":value.manual(),"effort":value.effort().label()});
    json!({"writer":choice(models.writer()),"reviewer":choice(models.reviewer()),"fixer":choice(models.fixer())})
}
pub fn response(value: AppResponsePayload) -> Result<Value> {
    match value {
        AppResponsePayload::Improvements(inbox) => Ok(improvements::projection(&inbox)),
        AppResponsePayload::Interaction(value) => Ok(
            json!({"run":snapshot(value.snapshot()),"models":model_values(value.models()),"mode":format!("{:?}", value.mode()).to_lowercase(),"received":value.received().to_string(),"incorporated":value.incorporated().to_string(),"activities":value.activities().iter().map(|a|json!({"id":a.sequence().to_string(),"kind":format!("{:?}",a.kind()).to_lowercase(),"text":a.text(),"detail":a.detail()})).collect::<Vec<_>>()}),
        ),
        AppResponsePayload::ProductRunAccepted(value) => Ok(json!({"run":snapshot(&value)})),
        AppResponsePayload::ProductRunSettled(value) => {
            Ok(json!({"run":snapshot(value.snapshot())}))
        }
        AppResponsePayload::Acknowledged(_) => Ok(json!({"acknowledged":true})),
        _ => Err(problem("Unexpected conversation response from daemon")),
    }
}
pub async fn conversation(app: &App, session: &str) -> Result<Value> {
    app.session(session)?;
    let run = RunId::new(bytes(session)?).map_err(|e| problem(format!("{e:?}")))?;
    response(
        request(app, AppRequestPayload::QueryInteraction(ProductRunConversationQuery::new(run)))
            .await?,
    )
}
fn prepare(app: &App, input: &Value) -> Result<ProductInteractionRequest> {
    let session = app.session(input["session"].as_str().unwrap_or(""))?;
    let project = app.project(&session.project)?;
    let facts = facts(app, &project)?;
    let mut input = input.clone();
    if input.get("models").is_none() {
        input["models"] = serde_json::to_value(&session.settings.models)?;
    }
    if input.get("providers").is_none() {
        input["providers"] = json!(session.settings.providers);
    }
    let workspace =
        WorkspaceId::new(bytes(facts["workspace"]["id"].as_str().ok_or_else(|| {
            problem("Set up this project in the Setup console before sending a message.")
        })?)?)
        .map_err(|e| problem(format!("{e:?}")))?;
    let provider = |role: &str| -> Result<ProviderProfileId> {
        let id = input["providers"][role]
            .as_str()
            .filter(|id| !id.is_empty())
            .or_else(|| facts["providers"][0]["id"].as_str())
            .ok_or_else(|| problem("Select a provider in Setup console."))?;
        ProviderProfileId::new(bytes(id)?).map_err(|e| problem(format!("{e:?}")))
    };
    let choice = |role: &str| -> Result<ProductModelChoice> {
        let model = input["models"][role]["id"].as_str().unwrap_or("");
        let choice = if model.is_empty() {
            ProductModelChoice::default()
        } else {
            ProductModelChoice::new(
                model.into(),
                input["models"][role]["manual"].as_bool().unwrap_or(false),
            )
            .map_err(problem)?
        };
        let effort = ProductModelEffort::parse(
            input["models"][role]["effort"].as_str().unwrap_or("default"),
        )
        .ok_or_else(|| problem("Unknown model effort"))?;
        Ok(choice.with_effort(effort))
    };
    let mode = match input["mode"].as_str().unwrap_or("chat") {
        "chat" => ProductInteractionMode::Chat,
        "plan" => ProductInteractionMode::Plan,
        "review" => ProductInteractionMode::Review,
        "build" => ProductInteractionMode::Build,
        _ => return Err(problem("Unknown conversation mode")),
    };
    let request_value = ProductRunRequest::new(
        RunId::new(bytes(&session.id)?).map_err(|e| problem(format!("{e:?}")))?,
        workspace,
        ProductProviderSelection::new(
            provider("writer")?,
            provider("reviewer")?,
            provider("fixer")?,
        ),
        input["text"].as_str().unwrap_or("").into(),
    )
    .map_err(problem)?;
    Ok(ProductInteractionRequest::new(
        request_value,
        mode,
        ProductRoleModels::new(choice("writer")?, choice("reviewer")?, choice("fixer")?),
    ))
}
pub async fn send(app: &App, input: &Value) -> Result<Value> {
    chat::send(app, input).await
}
pub async fn control(app: &App, session: &str, action: &str, operation: &str) -> Result<Value> {
    app.session(session)?;
    let action = match action {
        "stop" => ProductRunControlAction::Cancel,
        "retry" => ProductRunControlAction::Retry,
        "export" => ProductRunControlAction::Export,
        "discard" => ProductRunControlAction::Discard,
        _ => {
            return Err(problem(
                "Use the candidate console for acceptance and commit, including exact qualification confirmation.",
            ));
        }
    };
    response(
        receipts::recorded(
            app,
            operation,
            AppRequestPayload::ControlProductRun(ProductRunControl::new(
                RunId::new(bytes(session)?).map_err(|e| problem(format!("{e:?}")))?,
                action,
            )),
        )
        .await?,
    )
}
pub async fn models(app: &App, profile: &str) -> Result<Value> {
    match request(
        app,
        AppRequestPayload::QueryModels(ProductModelQuery::new(
            ProviderProfileId::new(bytes(profile)?).map_err(|e| problem(format!("{e:?}")))?,
            false,
        )),
    )
    .await?
    {
        AppResponsePayload::Models(catalog) => Ok(
            json!({"configured":catalog.configured(),"error":catalog.error(),"cached":catalog.cached(),"models":catalog.models().iter().map(|m|json!({"id":m.id(),"label":m.label()})).collect::<Vec<_>>()}),
        ),
        _ => Err(problem("Unexpected model catalog response")),
    }
}
pub async fn runs(app: &App) -> Result<Value> {
    match request(app, AppRequestPayload::QueryProductRuns(ProductRunQuery::recent())).await? {
        AppResponsePayload::ProductRuns(runs) => {
            Ok(json!(runs.iter().map(snapshot).collect::<Vec<_>>()))
        }
        AppResponsePayload::ProductRunSettlements(runs) => {
            Ok(json!(runs.iter().map(|run| snapshot(run.snapshot())).collect::<Vec<_>>()))
        }
        _ => Err(problem("Unexpected runs response")),
    }
}
