//! Durable browser selections and exact daemon-run adoption.
use crate::{
    daemon,
    error::{Result, problem},
    state::{App, Session, hex},
};
use peritus_app_protocol::{
    AppRequestPayload, AppResponsePayload, ProductModelChoice, ProductModelEffort,
    ProductModelUpdate, ProductRoleModels, ProductRunConversationQuery,
};
use peritus_types::{ProviderProfileId, RunId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub models: BTreeMap<String, Model>,
    pub providers: BTreeMap<String, String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub id: String,
    pub manual: bool,
    pub effort: String,
}
impl Settings {
    pub fn validate(&self) -> Result<ProductRoleModels> {
        for role in self.models.keys().chain(self.providers.keys()) {
            if !["writer", "reviewer", "fixer"].contains(&role.as_str()) {
                return Err(problem("Unknown model role"));
            }
        }
        for id in self.providers.values().filter(|id| !id.is_empty()) {
            ProviderProfileId::new(daemon::bytes(id)?).map_err(|e| problem(format!("{e:?}")))?;
        }
        let choice = |role: &str| -> Result<ProductModelChoice> {
            let Some(model) = self.models.get(role) else {
                return Ok(ProductModelChoice::default());
            };
            let choice = if model.id.is_empty() {
                ProductModelChoice::default()
            } else {
                ProductModelChoice::new(model.id.clone(), model.manual).map_err(problem)?
            };
            let effort = ProductModelEffort::parse(&model.effort)
                .ok_or_else(|| problem("Unknown model effort"))?;
            Ok(choice.with_effort(effort))
        };
        Ok(ProductRoleModels::new(choice("writer")?, choice("reviewer")?, choice("fixer")?))
    }
}

pub async fn configure(app: &App, input: &Value) -> Result<Value> {
    let id = input["session"].as_str().ok_or_else(|| problem("Choose a session"))?;
    app.session(id)?;
    let settings: Settings = serde_json::from_value(input["settings"].clone())?;
    let models = settings.validate()?;
    // Existing conversations own their governing models. Save through the same native
    // update used by the TUI; both clients observe the daemon-owned selection.
    let conversation = if input["existing"].as_bool().unwrap_or(false) {
        let run = RunId::new(daemon::bytes(id)?).map_err(|e| problem(format!("{e:?}")))?;
        daemon::response(
            daemon::receipts::recorded(
                app,
                input["operation"].as_str().unwrap_or(""),
                AppRequestPayload::UpdateModels(ProductModelUpdate::new(run, models)),
            )
            .await?,
        )?
    } else {
        Value::Null
    };
    save(app, input, &conversation).map_err(|error| {
        if input["existing"] == true { crate::error::uncertain(error.0) } else { error }
    })
}

// Also completes the local half after recovery of an already-observed native update.
pub fn save(app: &App, input: &Value, conversation: &Value) -> Result<Value> {
    let settings: Settings = serde_json::from_value(input["settings"].clone())?;
    settings.validate()?;
    app.update(|state| {
        let target = state
            .sessions
            .iter_mut()
            .find(|s| input["session"] == s.id)
            .ok_or_else(|| problem("Session missing"))?;
        target.settings = settings;
        Ok(json!({"session":target,"conversation":conversation}))
    })
}

pub async fn open_run(app: &App, id: &str) -> Result<Value> {
    let run_id = RunId::new(daemon::bytes(id)?).map_err(|e| problem(format!("{e:?}")))?;
    let reply = daemon::raw_request(
        app,
        AppRequestPayload::QueryInteraction(ProductRunConversationQuery::new(run_id)),
    )
    .await?;
    let AppResponsePayload::Interaction(conversation) = reply else {
        return Err(problem(
            "This run has no interactive conversation. Inspect it or use its exact-run CLI controls.",
        ));
    };
    let workspace_id = hex(conversation.snapshot().workspace_id().as_bytes());
    let project = app
        .snapshot()?
        .projects
        .into_iter()
        .find(|project| {
            daemon::facts(app, project).is_ok_and(|facts| facts["workspace"]["id"] == workspace_id)
        })
        .ok_or_else(|| problem("Open this run's project before opening its conversation."))?;
    app.update(|state| {
        let project = state
            .projects
            .iter_mut()
            .find(|p| p.id == project.id)
            .ok_or_else(|| problem("Project missing"))?;
        project.closed = false;
        if let Some(session) = state.sessions.iter_mut().find(|s| s.id == id) {
            if session.project != project.id {
                return Err(problem("Run belongs to another project"));
            }
            session.closed = false;
            return Ok(json!(session));
        }
        let session = Session {
            id: id.into(),
            project: project.id.clone(),
            parent: None,
            title: conversation.snapshot().task().chars().take(60).collect(),
            closed: false,
            settings: Settings::default(),
        };
        state.sessions.push(session.clone());
        Ok(json!(session))
    })
}
