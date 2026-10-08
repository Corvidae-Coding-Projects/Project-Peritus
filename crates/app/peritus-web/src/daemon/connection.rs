//! Reconnect a durable gateway IPC session without replacing its authenticated owner.
use super::{App, Client, Result, bytes, hex, problem};
use peritus_app_protocol::WellKnownProtocolFeature;
use peritus_types::SessionId;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    workspace: String,
    endpoint: PathBuf,
    session: String,
}

pub(super) async fn connect(
    app: &App, endpoint: &Path, required: &[WellKnownProtocolFeature],
    original_session: Option<SessionId>,
) -> Result<Client> {
    let workspace = app.snapshot()?.identity;
    let mut hash = Sha256::new();
    hash.update(b"peritus/web/native-session/v1\0");
    hash.update(workspace.as_bytes());
    hash.update([0]);
    hash.update(endpoint.as_os_str().as_encoded_bytes());
    let key = hex(&hash.finalize());
    let lock = app.lock(format!("native-session:{key}"))?;
    let _guard = lock.lock().await;
    let root = app.options.state_file.parent().ok_or_else(|| problem("Workspace state has no directory"))?;
    let path = root.join("native-sessions").join(format!("{key}.json"));
    let read_path = path.clone();
    let retained = tokio::task::spawn_blocking(move || -> Result<Option<Binding>> {
        match std::fs::read(read_path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }).await.map_err(problem)??;
    if retained.as_ref().is_some_and(|binding| binding.workspace != workspace || binding.endpoint != endpoint) {
        return Err(problem("The retained IPC session belongs to another gateway or endpoint"));
    }
    let retained_session = retained.as_ref().map(|binding| {
        SessionId::new(bytes(&binding.session)?).map_err(|error| problem(format!("{error:?}")))
    }).transpose()?;
    if original_session.is_some()
        && retained_session.is_some()
        && original_session != retained_session
    {
        return Err(problem(
            "The retained endpoint sidecar belongs to another native session",
        ));
    }
    let requested = original_session.or(retained_session);
    // A rejected/revoked original session stays visible. It never silently becomes a new owner.
    let client = Client::connect(endpoint.as_os_str(), requested, None, required).await.map_err(problem)?;
    if retained.is_none() {
        let binding = Binding {
            workspace, endpoint: endpoint.to_owned(), session: hex(client.context().session_id().as_bytes()),
        };
        let payload = serde_json::to_vec(&binding)?;
        tokio::task::spawn_blocking(move || crate::state::save(&path, &payload))
            .await.map_err(problem)??;
    }
    Ok(client)
}
