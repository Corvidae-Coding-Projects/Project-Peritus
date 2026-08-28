//! Durable external-benchmark conversation turns and per-turn trace identity.

use std::{
    fmt::Write as _,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use peritus_product_runner::ConversationView;
use serde::{Deserialize, Serialize};

use crate::BenchmarkError;

const SCHEMA_VERSION: u32 = 1;
const MAX_TURNS: usize = 64;
const MAX_STATE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone)]
pub struct BenchmarkSession {
    evidence_dir: PathBuf,
    stored: StoredSession,
    current_turn: usize,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredSession {
    schema_version: u32,
    session_id: String,
    task_id: String,
    turns: Vec<StoredTurn>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredTurn {
    key: String,
    prompt: String,
}

impl BenchmarkSession {
    pub fn open(
        evidence_dir: &Path,
        session_id: &str,
        task_id: &str,
        prompt_file: &Path,
        prompt: String,
    ) -> Result<Self, BenchmarkError> {
        let path = evidence_dir.join("conversation.json");
        let mut stored = if path.exists() {
            read(&path)?
        } else {
            StoredSession {
                schema_version: SCHEMA_VERSION,
                session_id: session_id.to_owned(),
                task_id: task_id.to_owned(),
                turns: Vec::new(),
            }
        };
        validate(&stored, session_id, task_id)?;
        let key = prompt_file
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or_else(|| BenchmarkError::Arguments("prompt filename is not UTF-8".to_owned()))?
            .to_owned();
        let current_turn = match stored.turns.iter().position(|turn| turn.key == key) {
            Some(index) if stored.turns[index].prompt == prompt => index,
            Some(_) => {
                return Err(BenchmarkError::Workspace(
                    "benchmark prompt changed for an existing session turn".to_owned(),
                ));
            }
            None => {
                if stored.turns.len() >= MAX_TURNS {
                    return Err(BenchmarkError::Workspace(
                        "benchmark conversation exceeds 64 turns".to_owned(),
                    ));
                }
                stored.turns.push(StoredTurn { key, prompt });
                stored.turns.len() - 1
            }
        };
        publish(&path, &stored)?;
        Ok(Self { evidence_dir: evidence_dir.to_owned(), stored, current_turn })
    }

    pub fn current_trace_path(&self) -> PathBuf {
        self.trace_path(self.current_turn)
    }

    pub fn trace_inputs(&self) -> Vec<(PathBuf, String)> {
        self.stored
            .turns
            .iter()
            .enumerate()
            .map(|(index, _)| (self.trace_path(index), self.render_through(index)))
            .collect()
    }

    pub const fn turn_number(&self) -> usize {
        self.current_turn + 1
    }

    fn trace_path(&self, index: usize) -> PathBuf {
        self.evidence_dir.join(format!("developer-round-{:04}.trace", index + 1))
    }

    fn render_through(&self, last: usize) -> String {
        let mut rendered = String::new();
        for (index, turn) in self.stored.turns.iter().take(last + 1).enumerate() {
            if !rendered.is_empty() {
                rendered.push_str("\n\n");
            }
            let _ = write!(rendered, "User round {}:\n{}", index + 1, turn.prompt);
        }
        rendered
    }
}

impl ConversationView for BenchmarkSession {
    fn revision(&self) -> u64 {
        u64::try_from(self.turn_number()).unwrap_or(u64::MAX)
    }

    fn render(&self) -> String {
        self.render_through(self.current_turn)
    }
}

fn read(path: &Path) -> Result<StoredSession, BenchmarkError> {
    let bytes = fs::read(path)
        .map_err(|error| BenchmarkError::filesystem("read benchmark conversation", path, error))?;
    if bytes.len() > MAX_STATE_BYTES {
        return Err(BenchmarkError::Workspace(
            "benchmark conversation state exceeds 32 MiB".to_owned(),
        ));
    }
    serde_json::from_slice(&bytes).map_err(BenchmarkError::from)
}

fn validate(stored: &StoredSession, session_id: &str, task_id: &str) -> Result<(), BenchmarkError> {
    if stored.schema_version != SCHEMA_VERSION
        || stored.session_id != session_id
        || stored.task_id != task_id
        || stored.turns.len() > MAX_TURNS
    {
        return Err(BenchmarkError::Workspace(
            "benchmark conversation identity or schema does not match".to_owned(),
        ));
    }
    Ok(())
}

fn publish(path: &Path, stored: &StoredSession) -> Result<(), BenchmarkError> {
    let mut bytes = serde_json::to_vec_pretty(stored)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_STATE_BYTES {
        return Err(BenchmarkError::Workspace(
            "benchmark conversation state exceeds 32 MiB".to_owned(),
        ));
    }
    let temporary = path.with_extension("json.new");
    let mut file = fs::File::create(&temporary).map_err(|error| {
        BenchmarkError::filesystem("create benchmark conversation", &temporary, error)
    })?;
    file.write_all(&bytes).map_err(|error| {
        BenchmarkError::filesystem("write benchmark conversation", &temporary, error)
    })?;
    file.sync_all().map_err(|error| {
        BenchmarkError::filesystem("sync benchmark conversation", &temporary, error)
    })?;
    fs::rename(&temporary, path)
        .map_err(|error| BenchmarkError::filesystem("publish benchmark conversation", path, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separate_process_turns_restore_exact_user_conversation() {
        let root = tempfile::tempdir().expect("temporary evidence");
        let first = BenchmarkSession::open(
            root.path(),
            "session",
            "task",
            Path::new("prompt-round1.txt"),
            "remember blue orchard".to_owned(),
        )
        .expect("first turn");
        assert_eq!(first.revision(), 1);

        let second = BenchmarkSession::open(
            root.path(),
            "session",
            "task",
            Path::new("prompt-round2.txt"),
            "recall the prior phrase".to_owned(),
        )
        .expect("second turn");

        assert_eq!(second.revision(), 2);
        assert!(second.render().contains("remember blue orchard"));
        assert!(second.render().contains("recall the prior phrase"));
        assert_eq!(second.trace_inputs().len(), 2);
        assert!(second.current_trace_path().ends_with("developer-round-0002.trace"));
    }
}
