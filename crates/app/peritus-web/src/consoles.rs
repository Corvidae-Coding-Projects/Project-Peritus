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
#[derive(Clone, Serialize, Deserialize)]
pub struct SavedConsole {
    pub(crate) console: Console,
    pub(crate) closed: bool,
}
pub fn list(app: &App) -> Result<Value> {
    crate::processes::reap(app)?;
    let consoles = app.snapshot()?.consoles;
    Ok(json!(
        consoles
            .values()
            .filter(|saved| !saved.closed)
            .map(|saved| {
                let mut value = serde_json::to_value(&saved.console)?;
                match Terminal::get(app, &saved.console.id).and_then(|terminal| terminal.finished())
                {
                    Ok(ended) => value["ended"] = json!(ended),
                    Err(error) => {
                        value["ended"] = json!(false);
                        value["error"] = json!(error.to_string());
                    }
                }
                Ok(value)
            })
            .collect::<Result<Vec<_>>>()?
    ))
}
pub fn close(app: &App, id: &str) -> Result<Value> {
    let terminal = Terminal::get(app, id)?;
    terminal.close()?;
    app.update(|state| {
        state.consoles.get_mut(id).ok_or_else(|| problem("Console not found"))?.closed = true;
        Ok(())
    })?;
    Ok(json!({"closed":true}))
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
        title: crate::sessions::title(input["title"].as_str().unwrap_or("CLI command"))?
            .as_str()
            .to_owned(),
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
        session.run.clone(),
    ];
    let console = Console {
        id: id()?,
        project: project.id,
        session: Some(session.id),
        title: crate::sessions::title(&session.title)?.as_str().to_owned(),
        suggestion: suggestion.into(),
    };
    Terminal::start(app, console.clone(), args)?;
    Ok(json!(console))
}
