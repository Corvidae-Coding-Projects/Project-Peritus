//! A stable Claude UUID is recorded before inference and always resumed exactly.

use peritus_model_protocol::ModelRequest;
use peritus_provider_core::{ProviderCoreError, RuntimeSession, RuntimeTurnDirectory};
use std::{
    fmt::Write as _,
    fs::{File, OpenOptions},
    io::Write as _,
    path::Path,
};

pub(super) struct Session {
    namespace: RuntimeSession,
    id: Option<String>,
    resumed: bool,
}
impl Session {
    pub(super) fn open(request: &ModelRequest) -> Result<Self, ProviderCoreError> {
        let namespace = RuntimeSession::open(request)?;
        let Some(root) = namespace.root() else {
            return Ok(Self { namespace, id: None, resumed: false });
        };
        let identity = root.join("session-id");
        let id = match std::fs::read_to_string(&identity) {
            Ok(id) => id,
            Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => {
                let mut bytes = [0; 16];
                getrandom::fill(&mut bytes)
                    .map_err(|_| error("cannot allocate native session identity"))?;
                bytes[6] = (bytes[6] & 0x0f) | 0x40;
                bytes[8] = (bytes[8] & 0x3f) | 0x80;
                let mut id = String::new();
                for (index, byte) in bytes.into_iter().enumerate() {
                    if [4, 6, 8, 10].contains(&index) {
                        id.push('-');
                    }
                    write!(id, "{byte:02x}")
                        .map_err(|_| error("cannot encode native session identity"))?;
                }
                write_new(&identity, id.as_bytes())?;
                id
            }
            Err(_) => return Err(error("cannot read native session identity")),
        };
        if id.len() != 36
            || !id.bytes().enumerate().all(|(index, byte)| {
                if [8, 13, 18, 23].contains(&index) {
                    byte == b'-'
                } else {
                    byte.is_ascii_hexdigit()
                }
            })
        {
            return Err(error("stored native session identity is invalid"));
        }
        let resumed = root
            .join("invocation-started")
            .try_exists()
            .map_err(|_| error("cannot inspect native session invocation"))?;
        Ok(Self { namespace, id: Some(id), resumed })
    }
    pub(super) fn root(&self) -> Option<&Path> {
        self.namespace.root()
    }
    pub(super) fn turn_directory(&self) -> Result<RuntimeTurnDirectory, ProviderCoreError> {
        self.namespace.turn_directory()
    }
    pub(super) fn arguments(&self, arguments: &mut Vec<String>) {
        if let Some(id) = &self.id {
            arguments.extend([
                if self.resumed { "--resume" } else { "--session-id" }.to_owned(),
                id.clone(),
                "--system-prompt-snapshot".to_owned(),
                "off".to_owned(),
            ]);
        }
    }
    pub(super) fn mark_started(&self) -> Result<(), ProviderCoreError> {
        if !self.resumed
            && let Some(root) = self.namespace.root()
        {
            write_new(&root.join("invocation-started"), b"started")?;
        }
        Ok(())
    }
    pub(super) fn validate_output(&self, output: &[u8]) -> Result<(), ProviderCoreError> {
        let Some(expected) = &self.id else { return Ok(()) };
        let value: serde_json::Value = serde_json::from_slice(output)
            .map_err(|_| error("native session output is incomplete; exact identity retained"))?;
        if value.get("session_id").and_then(serde_json::Value::as_str) != Some(expected) {
            return Err(error("native runtime returned a different or absent session identity"));
        }
        Ok(())
    }
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), ProviderCoreError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| error("cannot create native session record"))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| error("cannot synchronize native session record"))?;
    #[cfg(unix)]
    File::open(path.parent().ok_or_else(|| error("native record has no parent"))?)
        .and_then(|file| file.sync_all())
        .map_err(|_| error("cannot synchronize native session directory"))?;
    Ok(())
}
const fn error(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::configuration("claude_runtime_session", detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_invocation_resumes_exact_recorded_uuid_and_rejects_other_output() {
        let directory = tempfile::tempdir().unwrap();
        let profile = crate::test_support::runtime_profile();
        let request = crate::test_support::runtime_request(&profile, false)
            .with_local_session_directory(directory.path().to_path_buf());
        let session = Session::open(&request).unwrap();
        assert!(Session::open(&request).is_err());
        let mut first = vec![];
        session.arguments(&mut first);
        assert_eq!(first[0], "--session-id");
        let id = first[1].clone();
        let root = session.root().unwrap().to_path_buf();
        session.mark_started().unwrap();
        drop(session);
        let resumed = Session::open(&request).unwrap();
        assert_eq!(resumed.root(), Some(root.as_path()));
        let mut next = vec![];
        resumed.arguments(&mut next);
        assert_eq!(next[0], "--resume");
        assert_eq!(next[1], id);
        assert!(!next.iter().any(|value| value == "--continue" || value == "--fork-session"));
        resumed.validate_output(format!(r#"{{"session_id":"{id}"}}"#).as_bytes()).unwrap();
        assert!(resumed.validate_output(br#"{"session_id":"unrelated"}"#).is_err());
        assert!(resumed.validate_output(br#"{"partial":"#).is_err());
    }
}
