//! Independent terminal output, exact retained input, resize and interrupt routes.

use crate::{
    body,
    error::{Result, problem, uncertain},
    state::App,
    terminal::receipt::{self, InputReceipt, InputState},
};
use axum::{Json, extract::{Path, Query, Request, State}};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::sync::Arc;

#[derive(Deserialize)]
pub(super) struct TerminalQuery {
    workspace: String,
    #[serde(default)]
    after: u64,
    #[serde(default)]
    recover_input: bool,
}

#[derive(Deserialize)]
pub(super) struct InputQuery {
    workspace: String,
    input: String,
    digest: String,
    #[serde(default)]
    form: bool,
}

pub(super) async fn terminal_read(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(query): Query<TerminalQuery>,
) -> Result<Json<Value>> {
    let value = tokio::task::spawn_blocking(move || {
        app.terminals
            .get_for_workspace(&query.workspace, &id)?
            .read(query.after, query.recover_input)
    })
    .await
    .map_err(problem)??;
    Ok(Json(value))
}

pub(super) async fn terminal_input(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(query): Query<InputQuery>,
    request: Request,
) -> Result<Json<Value>> {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    if !receipt::valid_input(&query.input) || !receipt::valid_digest(&query.digest) {
        return Err(problem("Terminal input identity or digest is invalid"));
    }
    let lock = app.lock(format!("terminal-input:{id}:{}", query.input))?;
    let _locked = lock.lock().await;
    let terminal = app.terminals.get_for_workspace(&query.workspace, &id)?;
    let console = terminal.directory().to_owned();

    let retained = tokio::task::spawn_blocking({
        let console = console.clone();
        let input = query.input.clone();
        move || InputReceipt::read(&console, &input)
    })
    .await
    .map_err(problem)??;

    let purpose = tokio::task::spawn_blocking({
        let console = console.clone();
        let input = query.input.clone();
        move || receipt::read_purpose(&console, &input)
    })
    .await
    .map_err(problem)??;
    if retained.as_ref().is_some_and(|value| value.digest != query.digest)
        || purpose.is_some_and(|form| form != query.form)
    {
        return Err(problem(
            "Terminal input identity was reused with another body or input purpose",
        ));
    }
    if retained.is_some() && purpose.is_none() {
        let console = console.clone();
        let input = query.input.clone();
        let form = query.form;
        tokio::task::spawn_blocking(move || receipt::publish_purpose(&console, &input, form))
            .await
            .map_err(problem)??;
    }
    let connection = match retained.as_ref().map(|value| value.state) {
        Some(InputState::Settled) => {
            return Ok(Json(json!({"ok":true,"input":query.input,"state":"settled"})));
        }
        Some(InputState::Unknown) => {
            return Err(uncertain(
                "Terminal input settlement is unknown; the exact retained body will not be sent again",
            ));
        }
        Some(InputState::Pending) => terminal.input_connection(&query.input, &query.digest)?,
        None => {
            // Reject incompatible legacy owners before accepting and retaining a new body.
            let connection = terminal.input_connection(&query.input, &query.digest)?;
            retain_body(&console, &query.input, &query.digest, request).await?;
            let console = console.clone();
            let input = query.input.clone();
            let form = query.form;
            tokio::task::spawn_blocking(move || {
                receipt::publish_purpose(&console, &input, form)
            })
            .await
            .map_err(problem)??;
            connection
        }
    };
    let mut owner = tokio::net::TcpStream::connect(connection.endpoint)
        .await
        .map_err(uncertain)?;
    owner.write_all(&connection.header).await.map_err(uncertain)?;
    owner.shutdown().await.map_err(uncertain)?;
    let mut acknowledgement = [0_u8; 1];
    let result = owner.read_exact(&mut acknowledgement).await;
    let receipt = tokio::task::spawn_blocking({
        let console = console.clone();
        let input = query.input.clone();
        move || InputReceipt::read(&console, &input)
    })
    .await
    .map_err(problem)??
    .ok_or_else(|| uncertain("Terminal input receipt disappeared during settlement"))?;
    if receipt.digest != query.digest {
        return Err(problem("Terminal input receipt changed during settlement"));
    }
    if receipt.state == InputState::Settled {
        return Ok(Json(json!({"ok":true,"input":query.input,"state":"settled"})));
    }
    if receipt.state == InputState::Unknown
        || result.is_ok() && acknowledgement == [crate::terminal::OWNER_UNKNOWN]
    {
        return Err(uncertain(
            "Terminal input may have reached the PTY; its exact retained body will not be sent again",
        ));
    }
    result.map_err(uncertain)?;
    Err(uncertain("Console owner did not settle the exact retained terminal input"))
}

pub(super) async fn terminal_input_review(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(query): Query<InputQuery>,
) -> Result<Json<Value>> {
    if !receipt::valid_input(&query.input) || !receipt::valid_digest(&query.digest) {
        return Err(problem("Terminal input identity or digest is invalid"));
    }
    let lock = app.lock(format!("terminal-input:{id}:{}", query.input))?;
    let _locked = lock.lock().await;
    tokio::task::spawn_blocking(move || {
        let terminal = app.terminals.get_for_workspace(&query.workspace, &id)?;
        receipt::review_unknown(terminal.directory(), &query.input, &query.digest)
    }).await.map_err(problem)??;
    Ok(Json(json!({"reviewed":true,"state":"unknown"})))
}

async fn retain_body(
    console: &std::path::Path,
    input: &str,
    expected_digest: &str,
    request: Request,
) -> Result<()> {
    use futures_util::StreamExt as _;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let directory = receipt::input_directory(console, input)?;
    tokio::fs::create_dir_all(&directory).await?;
    let body_path = receipt::body_path(console, input)?;
    if tokio::fs::try_exists(&body_path).await? {
        let mut retained = tokio::fs::File::open(&body_path).await?;
        let mut digest = Sha256::new();
        let mut bytes = 0_u64;
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let count = retained.read(&mut buffer).await?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
            bytes = bytes
                .checked_add(u64::try_from(count).map_err(problem)?)
                .ok_or_else(|| problem("Terminal input length overflowed"))?;
        }
        let observed = crate::state::hex(&digest.finalize());
        if observed != expected_digest {
            return Err(problem(
                "Retained terminal input body conflicts with its declared digest",
            ));
        }
        let receipt = InputReceipt::pending(input.to_owned(), observed, bytes)?;
        tokio::task::spawn_blocking({
            let console = console.to_owned();
            move || receipt.publish(&console)
        })
        .await
        .map_err(problem)??;
        return Ok(());
    }
    let temporary = directory.join(format!(".body-{}.tmp", crate::state::id()?));
    let mut output = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .await?;
    let mut body = request.into_body().into_data_stream();
    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(uncertain)?;
        output.write_all(&chunk).await.map_err(uncertain)?;
        digest.update(&chunk);
        bytes = bytes
            .checked_add(u64::try_from(chunk.len()).map_err(problem)?)
            .ok_or_else(|| problem("Terminal input length overflowed"))?;
    }
    output.sync_all().await.map_err(uncertain)?;
    drop(output);
    let observed = crate::state::hex(&digest.finalize());
    if observed != expected_digest {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(problem("Terminal input body does not match its declared digest"));
    }
    tokio::fs::rename(&temporary, &body_path).await.map_err(uncertain)?;
    let receipt = InputReceipt::pending(input.to_owned(), observed, bytes)?;
    tokio::task::spawn_blocking({
        let console = console.to_owned();
        move || receipt.publish(&console)
    })
    .await
    .map_err(problem)??;
    Ok(())
}

pub(super) async fn terminal_resize(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    body::JsonInput(value): body::JsonInput,
) -> Result<Json<Value>> {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let workspace = value["workspace"]
        .as_str()
        .ok_or_else(|| problem("Gateway workspace identity is required"))?
        .to_owned();
    let cols = value["cols"].as_u64().ok_or_else(|| problem("PTY columns are required"))?;
    let rows = value["rows"].as_u64().ok_or_else(|| problem("PTY rows are required"))?;
    let pixels_wide = value["pixelWidth"].as_u64().unwrap_or(0);
    let pixels_high = value["pixelHeight"].as_u64().unwrap_or(0);
    let payload = crate::terminal::resize_payload(cols, rows, pixels_wide, pixels_high)?;
    let connection = tokio::task::spawn_blocking(move || {
        app.terminals
            .get_for_workspace(&workspace, &id)?
            .connection(crate::terminal::Operation::Resize)
    })
    .await
    .map_err(problem)??;
    let mut owner = tokio::net::TcpStream::connect(connection.endpoint).await.map_err(problem)?;
    owner.write_all(&connection.header).await.map_err(problem)?;
    owner.write_all(&payload).await.map_err(problem)?;
    owner.shutdown().await.map_err(problem)?;
    let mut acknowledgement = [0_u8; 1];
    owner.read_exact(&mut acknowledgement).await.map_err(problem)?;
    if acknowledgement != [crate::terminal::OWNER_ACK] {
        return Err(problem("Console owner did not confirm the PTY resize"));
    }
    Ok(Json(json!({"ok":true})))
}

pub(super) async fn terminal_interrupt(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    body::JsonInput(value): body::JsonInput,
) -> Result<Json<Value>> {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let workspace = value["workspace"]
        .as_str()
        .ok_or_else(|| problem("Gateway workspace identity is required"))?
        .to_owned();
    let connection = tokio::task::spawn_blocking(move || {
        app.terminals
            .get_for_workspace(&workspace, &id)?
            .connection(crate::terminal::Operation::Interrupt)
    })
    .await
    .map_err(problem)??;
    let mut owner = tokio::net::TcpStream::connect(connection.endpoint)
        .await
        .map_err(uncertain)?;
    owner.write_all(&connection.header).await.map_err(uncertain)?;
    owner.shutdown().await.map_err(uncertain)?;
    let mut acknowledgement = [0_u8; 1];
    owner.read_exact(&mut acknowledgement).await.map_err(uncertain)?;
    if acknowledgement != [crate::terminal::OWNER_ACK] {
        return Err(uncertain("Console owner did not confirm the interrupt"));
    }
    Ok(Json(json!({"ok":true})))
}
