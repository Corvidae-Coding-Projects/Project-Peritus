//! Discoverable retained consoles and session-bound interactive CLI entry.
use crate::{
    daemon,
    error::{Result, problem},
    state::{App, id},
    terminal::Terminal,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Serialize, Deserialize)]
pub struct Console {
    pub id: String,
    pub project: String,
    pub session: Option<String>,
    pub title: String,
    pub suggestion: String,
}
pub fn list(app: &App) -> Result<Value> {
    let terminals = app.terminals.lock().map_err(problem)?;
    Ok(json!(
        terminals
            .values()
            .map(|terminal| {
                let mut value = serde_json::to_value(&terminal.console)?;
                value["ended"] = json!(terminal.finished()?);
                Ok(value)
            })
            .collect::<Result<Vec<_>>>()?
    ))
}
pub fn start(app: &App, input: &Value) -> Result<Value> {
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
    if input["daemon"].as_bool().unwrap_or(false) {
        args.splice(
            0..0,
            ["--endpoint".into(), daemon::endpoint(app)?.to_string_lossy().into_owned()],
        );
    }
    let console = Console {
        id: id()?,
        project: project.into(),
        session: None,
        title: input["title"].as_str().unwrap_or("CLI command").chars().take(120).collect(),
        suggestion: String::new(),
    };
    Terminal::start(app, console.clone(), args)?;
    Ok(json!(console))
}
pub fn workbench(app: &App, input: &Value) -> Result<Value> {
    let session =
        app.session(input["session"].as_str().ok_or_else(|| problem("Choose a session"))?)?;
    let project = app.project(&session.project)?;
    let suggestion = input["suggestion"].as_str().unwrap_or("");
    if suggestion.len() > 32768 || suggestion.contains(['\r', '\n', '\0']) {
        return Err(problem("Use one bounded workbench command"));
    }
    let args = vec![
        "--endpoint".into(),
        daemon::endpoint(app)?.to_string_lossy().into_owned(),
        "open".into(),
        project.root.to_string_lossy().into_owned(),
        "--run".into(),
        session.id.clone(),
    ];
    let console = Console {
        id: id()?,
        project: project.id,
        session: Some(session.id),
        title: format!("Harness · {}", session.title),
        suggestion: suggestion.into(),
    };
    Terminal::start(app, console.clone(), args)?;
    Ok(json!(console))
}
