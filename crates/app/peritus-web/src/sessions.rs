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
    peritus_app_protocol::ConversationTitle::new(value.to_owned())
        .map_err(|error| problem(format!("Invalid conversation title: {error}")))
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
        let owner = app.session_owner(&session.id)?.ok_or_else(|| {
            crate::error::uncertain(
                "This legacy browser session has no retained native owner. Inspect its run before changing native model settings.",
            )
        })?;
        app.bind_session_owner(&session.id, owner.clone())?;
        daemon::response(
            daemon::receipts::recorded(
                app,
                &owner,
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
    let retained = app.snapshot()?.sessions.into_iter().find(|session| session.run == id);
    let retained_owner = retained
        .as_ref()
        .map(|session| app.session_owner(&session.id))
        .transpose()?
        .flatten();
    let payload = AppRequestPayload::QueryInteractionBinding(ProductInteractionQuery::new(run_id));
    let (reply, connection) = if let Some(owner) = retained_owner.as_ref() {
        (daemon::raw_request_owned(app, owner, payload).await?, None)
    } else {
        let target = daemon::current_target_async(app).await?;
        let (reply, connection) = daemon::raw_request_target(app, &target, payload).await?;
        (reply, Some(connection))
    };
    let AppResponsePayload::InteractionBinding(binding) = reply else {
        return Err(problem(
            "This run has no durable conversation. Inspect it or use its exact-run CLI controls.",
        ));
    };
    let conversation = binding.conversation();
    let interaction = binding.interaction();
    let workspace_id = hex(interaction.snapshot().workspace_id().as_bytes());
    let owner = match retained_owner {
        Some(owner) => {
            if owner.workspace() != workspace_id {
                return Err(problem("The retained run belongs to another native workspace"));
            }
            owner
        }
        None => connection
            .ok_or_else(|| problem("Native run connection identity is unavailable"))?
            .bind(interaction.snapshot().workspace_id())?,
    };
    let retained_project = owner.target().retained_project_async(&workspace_id).await?;
    let project = app.adopt_project(&retained_project.root)?;
    let retained_title = task_title(interaction.snapshot().task())?;
    app.update(|state| {
        let project = state
            .projects
            .iter_mut()
            .find(|p| p.id == project.id)
            .ok_or_else(|| problem("Project missing"))?;
        project.closed = false;
        if let Some(session) = state.sessions.iter_mut().find(|s| s.run == id) {
            if session.project != project.id {
                return Err(problem("Run belongs to another project"));
            }
            if session.native.as_ref().is_some_and(|existing| existing != &owner) {
                return Err(problem("Run belongs to another native owner"));
            }
            session.closed = false;
            session.conversation = hex(conversation.conversation().as_bytes());
            session.native = Some(owner);
            return Ok(json!(session));
        }
        let session = Session {
            id: crate::state::id()?,
            conversation: hex(conversation.conversation().as_bytes()),
            run: id.into(),
            native: Some(owner),
            project: project.id.clone(),
            parent: None,
            title: retained_title,
            closed: false,
            settings: Settings::default(),
        };
        state.sessions.push(session.clone());
        Ok(json!(session))
    })
}

fn task_title(task: &str) -> Result<String> {
    let title: String = task.chars().map(|character| {
        if character.is_control() { ' ' } else { character }
    }).collect();
    let source = if title.trim().is_empty() { "Conversation" } else { &title };
    peritus_app_protocol::ConversationTitle::derived_label(
        "",
        source,
        peritus_app_protocol::CONVERSATION_TITLE_LABEL_BYTES,
    )
    .map(|title| title.as_str().to_owned())
    .map_err(|error| problem(format!("Invalid derived conversation title: {error}")))
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
    let conversation_hex = hex(conversation.as_bytes());
    let run_hex = hex(run_id.as_bytes());
    let target_hex = hex(target.as_bytes());
    let retained = app
        .snapshot()?
        .sessions
        .into_iter()
        .find(|session| session.conversation == conversation_hex);
    let retained_owner = retained
        .as_ref()
        .map(|session| app.session_owner(&session.id))
        .transpose()?
        .flatten();
    let payload = AppRequestPayload::QueryWorkbench(query);
    let (reply, connection) = if let Some(session) = &retained {
        if session.run != run_hex {
            return Err(problem(
                "The evaluation workbench belongs to another retained run",
            ));
        }
        if let Some(owner) = retained_owner.as_ref() {
            if owner.workspace() != target_hex {
                return Err(problem(
                    "The evaluation workbench belongs to another native workspace",
                ));
            }
            (daemon::raw_request_owned(app, owner, payload).await?, None)
        } else {
            let native_target = daemon::current_target_async(app).await?;
            let (reply, connection) =
                daemon::raw_request_target(app, &native_target, payload).await?;
            (reply, Some(connection))
        }
    } else {
        let native_target = daemon::current_target_async(app).await?;
        let (reply, connection) = daemon::raw_request_target(app, &native_target, payload).await?;
        (reply, Some(connection))
    };
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
    let owner = match retained_owner {
        Some(owner) => owner,
        None => connection
            .ok_or_else(|| problem("Native workbench connection identity is unavailable"))?
            .bind(target)?,
    };
    let retained_project = owner.target().retained_project_async(&target_hex).await?;
    let project = app.adopt_project(&retained_project.root)?;
    app.update(|state| {
        let project = state
            .projects
            .iter_mut()
            .find(|candidate| candidate.id == project.id)
            .ok_or_else(|| problem("Project missing"))?;
        project.closed = false;
        if let Some(session) =
            state.sessions.iter_mut().find(|session| session.conversation == conversation_hex)
        {
            if session.project != project.id || session.run != run_hex {
                return Err(problem("Evaluation workbench belongs to another local session"));
            }
            if session.native.as_ref().is_some_and(|existing| existing != &owner) {
                return Err(problem("Evaluation workbench belongs to another native owner"));
            }
            session.closed = false;
            session.native = Some(owner);
            return Ok(json!(session));
        }
        let session = Session {
            id: crate::state::id()?,
            conversation: conversation_hex,
            run: run_hex,
            native: Some(owner),
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
