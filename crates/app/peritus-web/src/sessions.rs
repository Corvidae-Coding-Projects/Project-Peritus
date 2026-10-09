//! Durable browser selections and exact daemon-run adoption.
use crate::{
    daemon,
    error::{Result, problem},
    state::{App, Session, hex},
};
use peritus_app_protocol::{
    AppRequestPayload, AppResponsePayload, ConversationId, ProductInteractionQuery,
    ProductModelChoice, ProductModelEffort, ProductModelUpdate, ProductRoleModels, WorkbenchQuery,
};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn title(value: &str) -> Result<peritus_app_protocol::ConversationTitle> {
    peritus_app_protocol::ConversationTitle::new(value.to_owned()).map_err(|_| problem(format!(
        "Use a nonblank title with no control characters and at most {} UTF-8 bytes (received {} bytes)",
        peritus_app_protocol::MAX_CONVERSATION_TITLE_BYTES, value.len(),
    )))
}

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
    let session = app.session(id)?;
    let settings: Settings = serde_json::from_value(input["settings"].clone())?;
    let models = settings.validate()?;
    // Existing conversations own their governing models. Save through the same native
    // update used by the TUI; both clients observe the daemon-owned selection.
    let conversation = if input["existing"].as_bool().unwrap_or(false) {
        let run =
            RunId::new(daemon::bytes(&session.run)?).map_err(|e| problem(format!("{e:?}")))?;
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
        AppRequestPayload::QueryInteractionBinding(ProductInteractionQuery::new(run_id)),
    )
    .await?;
    let AppResponsePayload::InteractionBinding(binding) = reply else {
        return Err(problem(
            "This run has no durable conversation. Inspect it or use its exact-run CLI controls.",
        ));
    };
    let conversation = binding.conversation();
    let interaction = binding.interaction();
    open_workbench(
        app,
        &hex(conversation.conversation().as_bytes()),
        id,
        &hex(interaction.snapshot().workspace_id().as_bytes()),
    )
    .await
}

pub async fn open_workbench(
    app: &App,
    conversation: &str,
    run: &str,
    target: &str,
) -> Result<Value> {
    let conversation =
        ConversationId::new(daemon::bytes(conversation)?).map_err(|e| problem(format!("{e:?}")))?;
    let run_id = RunId::new(daemon::bytes(run)?).map_err(|e| problem(format!("{e:?}")))?;
    let target = WorkspaceId::new(daemon::bytes(target)?).map_err(|e| problem(format!("{e:?}")))?;
    let query = WorkbenchQuery::new(conversation, target);
    let reply = daemon::raw_request(app, AppRequestPayload::QueryWorkbench(query)).await?;
    let snapshot = match reply {
        AppResponsePayload::Workbench(snapshot) if snapshot.query() == query => snapshot,
        AppResponsePayload::Error(error) => {
            return Err(problem(format!(
                "The durable evaluation workbench is unavailable: {}",
                error.actionable_message()
            )));
        }
        _ => return Err(problem("The daemon returned another evaluation workbench")),
    };
    let target_hex = hex(target.as_bytes());
    let project = app
        .snapshot()?
        .projects
        .into_iter()
        .find(|project| {
            daemon::facts(app, project).is_ok_and(|facts| facts["workspace"]["id"] == target_hex)
        })
        .ok_or_else(|| problem("Open the evaluation target project before its workbench."))?;
    app.update(|state| {
        let project = state
            .projects
            .iter_mut()
            .find(|candidate| candidate.id == project.id)
            .ok_or_else(|| problem("Project missing"))?;
        project.closed = false;
        let conversation_hex = hex(conversation.as_bytes());
        let run_hex = hex(run_id.as_bytes());
        if let Some(session) =
            state.sessions.iter_mut().find(|session| session.conversation == conversation_hex)
        {
            if session.project != project.id || session.run != run_hex {
                return Err(problem("Evaluation workbench belongs to another local session"));
            }
            session.closed = false;
            return Ok(json!(session));
        }
        let session = Session {
            id: crate::state::id()?,
            conversation: conversation_hex,
            run: run_hex,
            project: project.id.clone(),
            parent: None,
            title: snapshot.title().as_str().to_owned(),
            closed: false,
            settings: Settings::default(),
        };
        state.sessions.push(session.clone());
        Ok(json!(session))
    })
}

#[cfg(test)]
mod title_tests {
    #[test]
    fn title_uses_the_native_utf8_byte_contract() {
        assert!(super::title(&"é".repeat(128)).is_ok());
        let error = super::title(&"é".repeat(129)).unwrap_err();
        assert!(error.0.contains("258 bytes"));
        for invalid in ["", "   ", "title\nnext", "\x1b[31m"] {
            assert!(super::title(invalid).is_err());
        }
    }
}
