//! A stable Claude UUID and its durable submission authority are retained across reconstruction.

use peritus_model_protocol::ModelRequest;
use peritus_provider_core::{ProviderCoreError, RuntimeSession, RuntimeTurnDirectory};
use std::{fmt::Write as _, fs::OpenOptions, io::Write as _, path::Path};

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
        let authority = root.join("invocation-started");
        let resumed = match std::fs::read(&authority) {
            Ok(record) => {
                validate_authority(&record, &id)?;
                true
            }
            Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => return Err(error("cannot inspect native session invocation")),
        };
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
            && let (Some(root), Some(id)) = (self.namespace.root(), self.id.as_deref())
        {
            write_new(&root.join("invocation-started"), &authority_record(id))?;
        }
        Ok(())
    }
    /// Withdraws a provisional first-submission marker after a proven pre-spawn failure.
    /// Existing native sessions retain their authority because a later connect failure does not
    /// erase an already accepted thread.
    pub(super) fn release_unaccepted_start(&self) -> Result<(), ProviderCoreError> {
        if self.resumed {
            return Ok(());
        }
        let (Some(root), Some(id)) = (self.namespace.root(), self.id.as_deref()) else {
            return Ok(());
        };
        let authority = root.join("invocation-started");
        let record = std::fs::read(&authority)
            .map_err(|_| ambiguous("cannot inspect unaccepted native submission marker"))?;
        if record != authority_record(id) {
            return Err(ambiguous("unaccepted native submission marker changed identity"));
        }
        std::fs::remove_file(&authority)
            .map_err(|_| ambiguous("cannot withdraw unaccepted native submission marker"))?;
        #[cfg(unix)]
        std::fs::File::open(root)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| ambiguous("cannot synchronize withdrawn native submission marker"))?;
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

fn authority_record(id: &str) -> Vec<u8> {
    format!("peritus-claude-native-session/v1\n{id}\n").into_bytes()
}

fn validate_authority(record: &[u8], id: &str) -> Result<(), ProviderCoreError> {
    // Version-zero installations wrote only this marker. It proves possible submission under the
    // exclusively owned route but cannot prove non-acceptance, so it remains resumable.
    if record == b"started" || record == authority_record(id) {
        return Ok(());
    }
    Err(error("stored native session authority is invalid or changed identity"))
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
    std::fs::File::open(path.parent().ok_or_else(|| error("native record has no parent"))?)
        .and_then(|file| file.sync_all())
        .map_err(|_| error("cannot synchronize native session directory"))?;
    Ok(())
}
const fn error(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::configuration("claude_runtime_session", detail)
}

const fn ambiguous(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::transport("claude_runtime_session", detail)
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
