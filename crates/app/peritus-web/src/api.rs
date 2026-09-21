//! Authenticated same-origin HTTP routes; all mutation sources share one dispatcher.

mod file_response;

use crate::{
    config::Preferences,
    daemon,
    error::{Result, problem},
    files, git, operations,
    state::{App, Operation, Session, id, save},
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};

pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/api/bootstrap", get(bootstrap))
        .route("/api/query", get(query))
        .route("/api/action", post(action))
        .route("/api/operation-review", post(operation_review))
        .route("/api/file", put(files::edit::save).layer(DefaultBodyLimit::max(files::TEXT_LIMIT)))
        .route("/api/raw", get(file_response::raw))
        .route("/api/terminal/{id}", get(terminal_read).post(terminal_write))
        .fallback_service(
            ServeDir::new(&app.options.assets)
                .not_found_service(ServeFile::new(app.options.assets.join("index.html"))),
        )
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(Arc::clone(&app), protect))
        .with_state(app)
}
async fn protect(State(app): State<Arc<App>>, request: Request, next: Next) -> Response {
    let host = request.headers().get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("");
    let valid_host =
        host == format!("127.0.0.1:{}", app.port) || host == format!("localhost:{}", app.port);
    let origin_ok = request.headers().get(header::ORIGIN).is_none_or(|origin| {
        origin.to_str().is_ok_and(|o| o == format!("http://{host}") || o == "http://127.0.0.1:5173")
    });
    let site_ok = request.headers().get("sec-fetch-site").is_none_or(|s| s != "cross-site");
    let api = request.uri().path().starts_with("/api/");
    let authorized = request.uri().path() == "/api/bootstrap"
        || request.headers().get("x-peritus-token").is_some_and(|v| v == app.token.as_str())
        || request.headers().get(header::COOKIE).and_then(|h| h.to_str().ok()).is_some_and(|c| {
            c.split(';').any(|pair| pair.trim() == format!("peritus-web={}", app.token))
        });
    if !valid_host || !origin_ok || !site_ok || api && !authorized {
        return (StatusCode::FORBIDDEN, "Open Peritus from its local console URL.").into_response();
    }
    let mut response = next.run(request).await;
    response.headers_mut().insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    response.headers_mut().insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    response.headers_mut().entry("content-security-policy").or_insert(HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' blob: data:; media-src 'self' blob:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'"));
    if api {
        response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    response
}
async fn bootstrap(State(app): State<Arc<App>>) -> Result<Response> {
    let config = std::fs::read_to_string(&app.options.config_file)?;
    let parsed = Preferences::parse(&config);
    let workspace = app.snapshot()?;
    let mut response = Json(json!({"workspace":{"projects":workspace.projects,"sessions":workspace.sessions},"consoles":crate::consoles::list(&app)?,"pendingOperations":operations::pending(&app)?,"preferences":parsed.as_ref().ok(),"config":config,"configError":parsed.err().map(|e|e.0),"configPath":app.options.config_file,"token":app.token})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&format!(
            "peritus-web={}; HttpOnly; SameSite=Strict; Path=/",
            app.token
        ))
        .map_err(problem)?,
    );
    Ok(response)
}
#[derive(Default, Deserialize)]
struct QueryArgs {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    project: String,
    #[serde(default)]
    session: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    profile: String,
    #[serde(default)]
    operation: String,
}
async fn query(State(app): State<Arc<App>>, Query(args): Query<QueryArgs>) -> Result<Json<Value>> {
    let value = match args.kind.as_str() {
        "files" => files::list(&app.project(&args.project)?.root, &args.path, args.offset)?,
        "pdf" => {
            let path = files::resolve(&app.project(&args.project)?.root, &args.path)?;
            let bytes = tokio::task::spawn_blocking(move || files::pdf::inspect(&path))
                .await
                .map_err(problem)??;
            json!({"bytes": bytes})
        }
        "text" => {
            let root = app.project(&args.project)?.root;
            tokio::task::spawn_blocking(move || files::text(&root, &args.path))
                .await
                .map_err(problem)??
        }
        "git" => git::status(&app.project(&args.project)?).await?,
        "ignore" => git::ignore(&app.project(&args.project)?, &args.path, false).await?,
        "daemon" => daemon::status(&app).await?,
        "facts" => daemon::ready_facts(&app, &app.project(&args.project)?).await?,
        "conversation" => daemon::conversation(&app, &args.session).await?,
        "models" => daemon::models(&app, &args.profile).await?,
        "runs" => daemon::runs(&app).await?,
        "consoles" => crate::consoles::list(&app)?,
        "operation" => operations::observe(&app, &args.operation)?,
        _ => return Err(problem("Unknown query")),
    };
    Ok(Json(value))
}
async fn action(State(app): State<Arc<App>>, Json(input): Json<Value>) -> Result<Json<Value>> {
    let operation = input["operation"]
        .as_str()
        .ok_or_else(|| problem("An operation identity is required"))?
        .to_owned();
    if operation.is_empty()
        || operation.len() > 80
        || !operation.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(problem("Invalid operation identity"));
    }
    let operation_lock = app.lock(format!("operation:{operation}"))?;
    let _guard = operation_lock.lock().await;
    if let Some(prior) = app.snapshot()?.operations.get(&operation) {
        if prior.input != input {
            return Err(problem("This operation identity was already used for different input"));
        }
        return Ok(Json(prior.result.clone().unwrap_or_else(|| json!({"error":"Outcome uncertain. Inspect the original conversation or repository before submitting another action.","uncertain":true}))));
    }
    let _resource_guard = mutation_guard(&app, &input).await?;
    app.update(|state| {
        state
            .operations
            .insert(operation.clone(), Operation { input: input.clone(), result: None });
        Ok(())
    })?;
    let result = match dispatch(&app, &input).await {
        Ok(value) => value,
        Err(error) if error.1 => {
            return Ok(Json(json!({"error":error.0,"uncertain":true,"operation":operation})));
        }
        Err(error) => json!({"error":error.0}),
    };
    app.update(|state| {
        state
            .operations
            .get_mut(&operation)
            .ok_or_else(|| problem("Operation record missing"))?
            .result = Some(result.clone());
        Ok(())
    })
    .map_err(|error| crate::error::uncertain(error.0))?;
    Ok(Json(result))
}
async fn operation_review(
    State(app): State<Arc<App>>,
    Json(input): Json<Value>,
) -> Result<Json<Value>> {
    if input["confirmed"] != true {
        return Err(problem("Confirm that you inspected the original target"));
    }
    let id = input["operation"].as_str().ok_or_else(|| problem("Choose the original operation"))?;
    let lock = app.lock(format!("operation:{id}"))?;
    let _guard = lock.lock().await;
    Ok(Json(operations::acknowledge(&app, id)?))
}
async fn mutation_guard(app: &Arc<App>, input: &Value) -> Result<tokio::sync::OwnedMutexGuard<()>> {
    let string = |key: &str| input[key].as_str().unwrap_or("");
    // Serialize related Git effects while unrelated projects and agent controls stay live.
    let resource_key = if ["git", "ignore", "repository"].contains(&string("command")) {
        format!("git:{}", string("project"))
    } else if ["send", "control", "session-settings"].contains(&string("command")) {
        format!("conversation:{}", string("session"))
    } else {
        format!("operation:{}", id()?)
    };
    let resource_lock = app.lock(resource_key)?;
    let resource_guard = resource_lock.lock_owned().await;
    if ["send", "control", "session-settings", "git", "ignore", "repository"]
        .contains(&string("command"))
    {
        let target = if ["send", "control", "session-settings"].contains(&string("command")) {
            "session"
        } else {
            "project"
        };
        let snapshot = app.snapshot()?;
        if let Some((id, _)) = snapshot.operations.iter().find(|(id, r)| {
            id.as_str() != string("operation")
                && r.result.is_none()
                && r.input[target] == input[target]
                && r.input["command"] != "daemon-request"
        }) {
            return Err(problem(format!(
                "Resolve original operation {id} before another change to this target."
            )));
        }
    }
    Ok(resource_guard)
}
async fn dispatch(app: &Arc<App>, input: &Value) -> Result<Value> {
    let string = |key: &str| input[key].as_str().unwrap_or("");
    match string("command") {
        "open-project" => Ok(json!(app.open_project(std::path::Path::new(string("root")))?)),
        "close-project" => {
            app.close_project(string("project"))?;
            Ok(json!({"closed":true}))
        }
        "new-session" => {
            let session = Session {
                settings: crate::sessions::Settings::default(),
                id: id()?,
                project: app.project(string("project"))?.id,
                parent: input["parent"].as_str().map(String::from),
                title: if string("title").is_empty() {
                    "New conversation".into()
                } else {
                    string("title").into()
                },
                closed: false,
            };
            app.update(|state| {
                App::nest(state, &session, session.parent.as_deref())?;
                state.sessions.push(session.clone());
                Ok(())
            })?;
            Ok(json!(session))
        }
        "session" => edit_session(app, input),
        "session-settings" => crate::sessions::configure(app, input).await,
        "open-run" => crate::sessions::open_run(app, string("run")).await,
        "workbench" => crate::consoles::workbench(app, input),
        "repository" => {
            let mut project = app.project(string("project"))?;
            project.repository = files::resolve(&project.root, string("path"))?;
            if !project.repository.is_dir() {
                return Err(problem("Repository location must be a directory"));
            }
            app.update(|state| {
                *state
                    .projects
                    .iter_mut()
                    .find(|p| p.id == project.id)
                    .ok_or_else(|| problem("Project missing"))? = project.clone();
                Ok(())
            })?;
            Ok(json!(project))
        }
        "git" => {
            git::action(
                &app.project(string("project"))?,
                string("action"),
                input["paths"]
                    .as_array()
                    .map(|v| v.iter().filter_map(Value::as_str).map(String::from).collect())
                    .unwrap_or_default(),
                string("message").into(),
                input,
            )
            .await
        }
        "ignore" => git::ignore(&app.project(string("project"))?, string("path"), true).await,
        "attach-file" => files::attachments::stage(app, input),
        "send" => daemon::send(app, input).await,
        "control" => {
            daemon::control(app, string("session"), string("action"), string("operation")).await
        }
        "config" => {
            let text = string("text");
            let preferences = Preferences::parse(text)?;
            save(&app.options.config_file, text.as_bytes())?;
            Ok(json!(preferences))
        }
        "preferences" => {
            let preferences: Preferences = serde_json::from_value(input["preferences"].clone())?;
            let text = toml::to_string_pretty(&preferences).map_err(problem)?;
            Preferences::parse(&text)?;
            save(&app.options.config_file, text.as_bytes())?;
            Ok(json!({"preferences":preferences,"config":text}))
        }
        "console" => crate::consoles::start(app, input),
        "close-console" => {
            app.terminals.lock().map_err(problem)?.remove(string("id"));
            Ok(json!({"closed":true}))
        }
        _ => Err(problem("Unknown command")),
    }
}
fn edit_session(app: &App, input: &Value) -> Result<Value> {
    app.update(|state| {
        let index = state
            .sessions
            .iter()
            .position(|session| session.id == input["session"].as_str().unwrap_or(""))
            .ok_or_else(|| problem("Session missing"))?;
        let mut session = state.sessions[index].clone();
        if let Some(parent) = input.get("parent") {
            let parent = parent.as_str().map(String::from);
            if parent != session.parent {
                session.parent = parent;
                App::nest(state, &session, session.parent.as_deref())?;
            }
        }
        if let Some(title) = input["title"].as_str() {
            if title.trim().is_empty() || title.len() > 256 {
                return Err(problem("Use a session title between 1 and 256 characters"));
            }
            session.title = title.into();
        }
        if let Some(closed) = input["closed"].as_bool() {
            session.closed = closed;
            if !closed {
                let project = state
                    .projects
                    .iter_mut()
                    .find(|p| p.id == session.project)
                    .ok_or_else(|| problem("Project not found"))?;
                project.closed = false;
            }
        }
        // Presentation-only edits remain available after a project is moved or deleted.
        state.sessions[index] = session.clone();
        Ok(json!(session))
    })
}
#[derive(Deserialize)]
struct TerminalQuery {
    #[serde(default)]
    after: u64,
}
async fn terminal_read(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(query): Query<TerminalQuery>,
) -> Result<Json<Value>> {
    let terminal = app
        .terminals
        .lock()
        .map_err(problem)?
        .get(&id)
        .cloned()
        .ok_or_else(|| problem("Console ended; open a new console"))?;
    Ok(Json(terminal.read(query.after)?))
}
async fn terminal_write(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(value): Json<Value>,
) -> Result<Json<Value>> {
    let terminal = app
        .terminals
        .lock()
        .map_err(problem)?
        .get(&id)
        .cloned()
        .ok_or_else(|| problem("Console ended"))?;
    if let Some(text) = value["text"].as_str() {
        terminal.input(text)?;
    }
    if let (Some(cols), Some(rows)) = (value["cols"].as_u64(), value["rows"].as_u64()) {
        terminal.resize(cols.min(400) as u16, rows.min(200) as u16)?;
    }
    Ok(Json(json!({"ok":true})))
}
