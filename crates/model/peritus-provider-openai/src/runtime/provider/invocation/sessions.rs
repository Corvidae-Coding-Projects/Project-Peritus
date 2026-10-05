//! Native session identity is scoped by the host task/role and immutable provider route.

use peritus_model_protocol::ModelRequest;
use peritus_provider_core::ProviderCoreError;
use peritus_provider_core::{RuntimeSession, RuntimeTurnDirectory};
use std::{
    fs::{self, File},
    io::{BufRead as _, BufReader, Read as _, Write as _},
};

pub(super) struct Session {
    namespace: RuntimeSession,
    thread: Option<String>,
}
impl Session {
    pub(super) fn open(request: &ModelRequest) -> Result<Self, ProviderCoreError> {
        let namespace = RuntimeSession::open(request)?;
        let thread = namespace.root().map(recover_thread).transpose()?.flatten();
        if let Some(root) = namespace.root()
            && thread.is_none()
            && root
                .join("invocation-started")
                .try_exists()
                .map_err(|_| error("cannot inspect native session invocation"))?
        {
            return Err(error("interrupted native session has no recoverable thread identity"));
        }
        Ok(Self { namespace, thread })
    }
    pub(super) fn thread(&self) -> Option<&str> {
        self.thread.as_deref()
    }
    pub(super) fn root(&self) -> Option<&std::path::Path> {
        self.namespace.root()
    }
    pub(super) fn persistent(&self) -> bool {
        self.thread.is_some() || self.namespace.root().is_some()
    }
    pub(super) fn turn_directory(&self) -> Result<RuntimeTurnDirectory, ProviderCoreError> {
        self.namespace.turn_directory()
    }
    pub(super) fn mark_started(&self) -> Result<(), ProviderCoreError> {
        if self.thread.is_none()
            && let Some(root) = self.namespace.root()
        {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(root.join("invocation-started"))
                .map_err(|_| error("cannot record native session invocation"))?;
            file.write_all(b"started")
                .and_then(|()| file.sync_all())
                .map_err(|_| error("cannot synchronize native session invocation"))?;
            #[cfg(unix)]
            File::open(root)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| error("cannot synchronize native session directory"))?;
        }
        Ok(())
    }
    pub(super) fn validate_output(&self) -> Result<(), ProviderCoreError> {
        if let Some(root) = self.namespace.root() {
            recover_thread(root)?.ok_or_else(|| error("native output has no thread identity"))?;
        }
        Ok(())
    }
}

fn recover_thread(root: &std::path::Path) -> Result<Option<String>, ProviderCoreError> {
    let mut thread = None;
    for entry in fs::read_dir(root).map_err(|_| error("cannot inspect native session journals"))? {
        let entry = entry.map_err(|_| error("cannot inspect native session journal"))?;
        if !entry.file_type().map_err(|_| error("cannot inspect native turn type"))?.is_dir() {
            continue;
        }
        let file = match File::open(entry.path().join("native.jsonl")) {
            Ok(file) => file,
            Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(error("cannot read native session journal")),
        };
        // The thread-start event precedes all inference. Retain partial final frames as evidence;
        // they cannot establish a session identity or trigger a global most-recent fallback.
        let mut reader = BufReader::new(file.take(64 * 1024));
        let mut line = String::new();
        while reader
            .read_line(&mut line)
            .map_err(|_| error("cannot read native thread identity"))?
            != 0
        {
            if !line.ends_with('\n') {
                break;
            }
            if let Ok(event) = serde_json::from_str::<serde_json::Value>(&line)
                && event.get("type").and_then(serde_json::Value::as_str) == Some("thread.started")
            {
                let id = event
                    .get("thread_id")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| error("native journal has no thread identity"))?;
                if id.is_empty()
                    || id.len() > 512
                    || id.starts_with('-')
                    || !id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                {
                    return Err(error("native journal has invalid thread identity"));
                }
                if thread.as_deref().is_some_and(|previous| previous != id) {
                    return Err(error("native namespace contains conflicting thread identities"));
                }
                thread = Some(id.to_owned());
                break;
            }
            line.clear();
        }
    }
    Ok(thread)
}

const fn error(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::configuration("codex_runtime_session", detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::runtime_support::{codex_profile, codex_tool_request_with_effort};
    use peritus_model_protocol::ReasoningEffort;
    fn fixture_request(root: &std::path::Path, model: &str) -> ModelRequest {
        let profile = codex_profile(model, true);
        codex_tool_request_with_effort(&profile, "fixture", ReasoningEffort::High)
            .with_local_session_directory(root.to_path_buf())
    }
    #[test]
    fn reopening_recovers_exact_thread_from_interrupted_output_and_isolates_routes() {
        let directory = tempfile::tempdir().expect("root");
        let request = fixture_request(directory.path(), "model-a");
        let session = Session::open(&request).expect("session");
        assert!(Session::open(&request).is_err(), "one native owner per lineage");
        let turn = session.turn_directory().expect("turn");
        fs::write(turn.path().join("native.jsonl"), b"{\"type\":\"thread.started\",\"thread_id\":\"12345678-1234-1234-1234-123456789abc\"}\n{\"incomplete\":").expect("interrupted native output");
        drop(session);
        let resumed = Session::open(&request).expect("reopen");
        assert_eq!(resumed.thread(), Some("12345678-1234-1234-1234-123456789abc"));
        let other = fixture_request(directory.path(), "model-b");
        assert!(Session::open(&other).expect("other model").thread().is_none());
        let role = tempfile::tempdir().expect("other role");
        assert!(
            Session::open(&fixture_request(role.path(), "model-a"))
                .expect("other role")
                .thread()
                .is_none()
        );
    }
    #[test]
    fn conflicting_threads_fail_closed_instead_of_selecting_most_recent() {
        let directory = tempfile::tempdir().expect("root");
        let request = fixture_request(directory.path(), "model-a");
        let session = Session::open(&request).expect("session");
        for id in ["thread-a", "thread-b"] {
            let turn = session.turn_directory().expect("turn");
            fs::write(
                turn.path().join("native.jsonl"),
                format!("{{\"type\":\"thread.started\",\"thread_id\":\"{id}\"}}\n"),
            )
            .expect("journal");
        }
        drop(session);
        assert!(Session::open(&request).is_err());
    }
    #[test]
    fn lost_thread_identity_cannot_silently_start_a_fresh_context() {
        let directory = tempfile::tempdir().expect("root");
        let request = fixture_request(directory.path(), "model-a");
        let session = Session::open(&request).expect("initial session");
        session.mark_started().expect("durable launch intent");
        assert!(session.validate_output().is_err());
        drop(session);
        assert!(Session::open(&request).is_err(), "ambiguous start must retain its failed lineage");
    }
}
