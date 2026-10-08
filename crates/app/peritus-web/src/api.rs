//! Authenticated same-origin HTTP routes; all mutation sources share one dispatcher.

mod file_response;
mod text_response;
mod terminal_response;
mod mutation;
mod recovery;

use crate::{
    body,
    config::Preferences,
    daemon,
    error::{Result, problem, uncertain},
    files, git, operations,
    state::{App, OperationOwner, Publication, Session, id, save},
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Query, Request, State},
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
        .route("/api/operation-review", post(recovery::review))
        .route("/api/operation-retry", post(recovery::retry))
        .route("/api/operation-cancel", post(recovery::cancel))
        .route("/api/file", put(files::edit::save))
        .route("/api/text", get(text_response::text))
        .route("/api/raw", get(file_response::raw))
        .route("/api/terminal/{id}", get(terminal_response::terminal_read))
        .route("/api/terminal/{id}/input", post(terminal_response::terminal_input))
        .route("/api/terminal/{id}/input-review", post(terminal_response::terminal_input_review))
        .route("/api/terminal/{id}/interrupt", post(terminal_response::terminal_interrupt))
        .route("/api/terminal/{id}/resize", post(terminal_response::terminal_resize))
        .fallback_service(
            ServeDir::new(&app.options.assets)
                .not_found_service(ServeFile::new(app.options.assets.join("index.html"))),
        )
        .layer(DefaultBodyLimit::disable())
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
    let pending = operations::pending_page(&app, None, None)?;
    let consoles = crate::consoles::list(&app).await?;
    let mut response = Json(json!({"identity":workspace.identity,"workspace":{"projects":workspace.projects,"sessions":workspace.sessions},"consoles":consoles,"pendingOperations":pending["operations"],"pendingOperationsCursor":pending["cursor"],"pendingOperationsSnapshot":pending["snapshot"],"preferences":parsed.as_ref().ok(),"config":config,"configError":parsed.err().map(|e|e.0),"configPath":app.options.config_file,"token":app.token})).into_response();
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
    workspace: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    project: String,
    #[serde(default)]
    session: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    directory: String,
    #[serde(default)]
    offset: u64,
    #[serde(default)]
    profile: String,
    #[serde(default)]
    operation: String,
    #[serde(default)]
    filter: String,
    #[serde(default)]
    snapshot: String,
    #[serde(default)]
    cursor: String,
    #[serde(default)]
    candidate: String,
    #[serde(default)]
    run: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    revision: String,
    #[serde(default)]
    stream: String,
}
async fn query(State(app): State<Arc<App>>, Query(args): Query<QueryArgs>) -> Result<Json<Value>> {
    if args.workspace!=app.snapshot()?.identity {
        return Err(problem("This observation belongs to another gateway workspace. Refresh the owning workspace before inspecting it."));
    }
    let value = match args.kind.as_str() {
        "files" => {
            let root = app.project(&args.project)?.root;
            let state_file = app.options.state_file.clone();
            let workspace = args.workspace.clone();
            tokio::task::spawn_blocking(move || {
                files::list_page(
                    &state_file,
                    &workspace,
                    &root,
                    &args.path,
                    &args.directory,
                    args.offset,
                    &args.filter,
                    &args.cursor,
                    &args.snapshot,
                )
            }).await.map_err(problem)??
        }
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
        "git" => git::status(&app, &app.project(&args.project)?).await?,
        "git-inventory" => git::inventory(
            &app,
            &app.project(&args.project)?,
            (!args.cursor.is_empty()).then_some(args.cursor.as_str()),
            (!args.snapshot.is_empty()).then_some(args.snapshot.as_str()),
        )
        .await?,
        "git-output" => git::output(&app, &args.operation, &args.stream, args.offset).await?,
        "ignore" => git::ignore(&app.project(&args.project)?, &args.path, false).await?,
        "daemon" => daemon::status(&app).await?,
        "facts" => daemon::ready_facts(&app, &app.project(&args.project)?).await?,
        "conversation" => daemon::conversation(&app, &args.session).await?,
        "models" => daemon::models(&app, &args.profile).await?,
        "runs" => daemon::runs(
            &app,
            (!args.cursor.is_empty()).then_some(args.cursor.as_str()),
        )
        .await?,
        "improvements" => {
            daemon::improvements::list(&app, &args.project, &args.cursor).await?
        }
        "improvement-evidence" => {
            daemon::improvements::evidence(
                &app,
                &args.project,
                &args.candidate,
                &args.revision,
                &args.cursor,
            )
            .await?
        }
        "improvement-text" => {
            daemon::improvements::text(
                &app,
                &args.project,
                &args.candidate,
                &args.run,
                &args.source,
                args.offset,
            )
            .await?
        }
        "consoles" => crate::consoles::list(&app).await?,
        "pending-operations" => operations::pending_page(
            &app,
            (!args.cursor.is_empty()).then_some(args.cursor.as_str()),
            (!args.snapshot.is_empty()).then_some(args.snapshot.as_str()),
        )?,
        "operation" => operations::observe(&app, &args.operation).await?,
        _ => return Err(problem("Unknown query")),
    };
    Ok(Json(value))
}
async fn action(State(app): State<Arc<App>>, body::JsonInput(input): body::JsonInput) -> Result<Json<Value>> {
    mutation::workspace(&app, &input)?;
    let operation = input["operation"]
        .as_str()
        .ok_or_else(|| problem("An operation identity is required"))?
        .to_owned();
    operations::validate_identity(&operation)?;
    let operation_lock = app.lock(format!("operation:{operation}"))?;
    let _guard = operation_lock.lock().await;
    let owner = app.own_operation(&operation).await?;
    if let Some(prior) = owner.get()? {
        if prior.input != input {
            return Err(problem("This operation identity was already used for different input"));
        }
        return Ok(Json(prior.result.unwrap_or_else(|| json!({"error":"Outcome uncertain. Inspect the original conversation or repository before submitting another action.","uncertain":true}))));
    }
    let _resource_guard = mutation::guard(&app, &input).await?;
    if owner.insert(input.clone())? == Publication::Existing {
        let prior = owner.get()?.ok_or_else(|| problem("Operation record missing"))?;
        return Ok(Json(prior.result.unwrap_or_else(|| json!({"error":"Outcome uncertain. Inspect the original conversation or repository before submitting another action.","uncertain":true}))));
    }
    let result = match dispatch(&app, &input, &owner).await {
        Ok(value) => value,
        Err(error) if error.1 => {
            return Ok(Json(json!({"error":error.0,"uncertain":true,"operation":operation})));
        }
        Err(error) => json!({"error":error.0}),
    };
    owner.settle(result.clone()).map_err(|error| uncertain(error.0))?;
    Ok(Json(result))
}
async fn dispatch(app: &Arc<App>, input: &Value, owner: &OperationOwner) -> Result<Value> {
    let string = |key: &str| input[key].as_str().unwrap_or("");
    match string("command") {
        "improvements" => daemon::improvements::action(app, input).await,
        "open-project" => Ok(json!(app.open_project(std::path::Path::new(string("root")))?)),
        "close-project" => {
            app.close_project(string("project"))?;
            Ok(json!({"closed":true}))
        }
        "new-session" => {
            let parent = input["parent"].as_str().map(String::from);
            let native = parent
                .as_deref()
                .map(|parent| app.session(parent).map(|session| session.native))
                .transpose()?
                .flatten();
            let session = Session {
                settings: crate::sessions::Settings::default(),
                id: id()?,
                conversation: id()?,
                run: id()?,
                native,
                project: app.project(string("project"))?.id,
                parent,
                title: crate::sessions::title(if string("title").is_empty() {
                    "New conversation"
                } else { string("title") })?.as_str().to_owned(),
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
        "open-workbench" => {
            crate::sessions::open_workbench(
                app,
                string("conversation"),
                string("run"),
                string("target"),
            )
            .await
        }
        "workbench" => crate::consoles::workbench(app, input, owner).await,
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
                app,
                owner,
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
        "attach-file" => files::attachments::stage(app, input).await,
        "send" => daemon::send_owned(app, input, owner).await,
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
        "console" => crate::consoles::start(app, input, owner).await,
        "close-console" => crate::consoles::close(app, input).await,
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
                if let Some(parent) = session.parent.as_deref() {
                    let inherited = state
                        .sessions
                        .iter()
                        .find(|candidate| candidate.id == parent)
                        .ok_or_else(|| problem("Parent session not found"))?
                        .native
                        .clone();
                    if session
                        .native
                        .as_ref()
                        .zip(inherited.as_ref())
                        .is_some_and(|(owner, parent)| owner != parent)
                    {
                        return Err(problem(
                            "Nested sessions cannot belong to different native owners",
                        ));
                    }
                    if session.native.is_none() {
                        session.native = inherited;
                    }
                }
            }
        }
        if let Some(title) = input["title"].as_str() {
            crate::sessions::title(title)?.as_str().clone_into(&mut session.title);
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
