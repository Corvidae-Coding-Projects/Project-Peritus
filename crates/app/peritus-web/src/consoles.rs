//! Discoverable retained consoles and session-bound interactive CLI entry.
use crate::{
    daemon,
    error::{Result, problem},
    state::{App, OperationOwner, id},
    terminal::{CloseDisposition, Terminal},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Console {
    pub id: String,
    pub project: String,
    pub session: Option<String>,
    pub title: String,
    pub suggestion: String,
}
pub async fn list(app: &Arc<App>) -> Result<Value> {
    let app = Arc::clone(app);
    tokio::task::spawn_blocking(move || {
        app.terminals
            .list()?
            .into_iter()
            .map(|terminal| terminal.summary())
            .collect::<Result<Vec<_>>>()
            .map(|consoles| json!(consoles))
    })
    .await
    .map_err(problem)?
}
pub async fn start(app: &Arc<App>, input: &Value, owner: &OperationOwner) -> Result<Value> {
    let project = input["project"].as_str().ok_or_else(|| problem("Choose a project"))?;
    let mut args = input["args"]
        .as_array()
        .map(|args| {
            args.iter()
                .map(|arg| {
                    arg.as_str()
                        .map(String::from)
                        .ok_or_else(|| problem("CLI arguments must be strings"))
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    let daemon = input["daemon"].as_bool().unwrap_or(false);
    let session = input["session"].as_str().filter(|id| !id.is_empty())
        .map(|id| app.session(id)).transpose()?;
    if session.as_ref().is_some_and(|session| session.project != project) {
        return Err(problem("The console session belongs to another project"));
    }
    let native = if daemon {
        Some(native_owner(app, session.as_ref().ok_or_else(|| problem("Choose a session for this daemon console"))?).await?)
    } else { None };
    if let Some(native) = &native {
        args.splice(0..0, [
            "--endpoint".into(), native.target().endpoint().to_string_lossy().into_owned(),
            "--session".into(), crate::state::hex(native.session_id()?.as_bytes()),
        ]);
    }
    let console = Console {
        id: id()?,
        project: project.into(),
        session: session.map(|session| session.id),
        title: input["title"].as_str().unwrap_or("CLI command").into(),
        suggestion: String::new(),
    };
    owner.retain_prepared(json!({
        "kind":"durable-console-launch-v1",
        "console":console,
        "native":native,
    }))?;
    let operation = input["operation"]
        .as_str()
        .ok_or_else(|| problem("A console launch operation identity is required"))?
        .to_owned();
    let launched = console.clone();
    let app = Arc::clone(app);
    tokio::task::spawn_blocking(move || {
        Terminal::start(&app, &operation, launched, args)
    })
    .await
    .map_err(problem)??;
    Ok(json!(console))
}
pub async fn workbench(
    app: &Arc<App>,
    input: &Value,
    owner: &OperationOwner,
) -> Result<Value> {
    let session = input["session"]
        .as_str()
        .ok_or_else(|| problem("Choose a session"))?
        .to_owned();
    let suggestion = input["suggestion"].as_str().unwrap_or("").to_owned();
    if suggestion.contains(['\r', '\n', '\0']) {
        return Err(problem("Use one workbench command without line breaks or NUL bytes"));
    }
    let selected = app.session(&session)?;
    let native = native_owner(app, &selected).await?;
    let endpoint = native.target().endpoint().to_owned();
    let native_session = crate::state::hex(native.session_id()?.as_bytes());
    let app_for_plan = Arc::clone(app);
    let (console, args) = tokio::task::spawn_blocking(move || {
        let session = app_for_plan.session(&session)?;
        let project = app_for_plan.project(&session.project)?;
        let args = vec![
            "--endpoint".into(),
            endpoint.to_string_lossy().into_owned(),
            "--session".into(),
            native_session,
            "open".into(),
            project.root.to_string_lossy().into_owned(),
            "--run".into(),
            session.run.clone(),
        ];
        let title = peritus_app_protocol::ConversationTitle::derived_label(
            "Harness · ",
            &session.title,
            peritus_app_protocol::CONVERSATION_TITLE_LABEL_BYTES,
        )
        .map_err(problem)?;
        let console = Console {
            id: id()?,
            project: project.id,
            session: Some(session.id),
            title: title.as_str().to_owned(),
            suggestion,
        };
        Ok::<_, crate::error::Error>((console, args))
    })
    .await
    .map_err(problem)??;
    owner.retain_prepared(json!({
        "kind":"durable-console-launch-v1",
        "console":console,
        "native":native,
    }))?;
    let operation = input["operation"]
        .as_str()
        .ok_or_else(|| problem("A console launch operation identity is required"))?
        .to_owned();
    let launched = console.clone();
    let app = Arc::clone(app);
    tokio::task::spawn_blocking(move || Terminal::start(&app, &operation, launched, args))
        .await
        .map_err(problem)??;
    Ok(json!(console))
}

async fn native_owner(app: &Arc<App>, session: &crate::state::Session) -> Result<daemon::NativeOwner> {
    let project = app.project(&session.project)?;
    let owner = match app.session_owner(&session.id)? {
        Some(owner) => { owner.facts_async(&project).await?; owner }
        None => daemon::owner_for_project(app, &project).await?.0,
    };
    let _connection = daemon::connect_owned(app, &owner, &[]).await?;
    app.bind_session_owner(&session.id, owner.clone())?;
    Ok(owner)
}

pub async fn close(app: &Arc<App>, input: &Value) -> Result<Value> {
    let identity = input["id"]
        .as_str()
        .ok_or_else(|| problem("Choose a console"))?
        .to_owned();
    let disposition = match input["disposition"].as_str() {
        Some("terminate") => CloseDisposition::Terminate,
        Some("dismiss") => CloseDisposition::Dismiss,
        _ => return Err(problem("Choose an explicit console close disposition")),
    };
    let app = Arc::clone(app);
    let closed = identity.clone();
    tokio::task::spawn_blocking(move || {
        let terminal = app.terminals.get(&closed)?;
        terminal.close(disposition)?;
        app.terminals.remove(&closed)
    })
    .await
    .map_err(problem)??;
    Ok(json!({"closed":true,"disposition":input["disposition"],"id":identity}))
}
