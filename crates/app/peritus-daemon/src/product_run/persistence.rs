//! Durable product-run snapshots and restart recovery.

use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

use peritus_app_protocol::{
    ProductConversationMessage, ProductConversationRole, ProductProviderSelection, ProductRunPhase,
    ProductRunRequest, ProductRunSnapshot,
};
use peritus_provider_core::CancellationToken;
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};
use serde::Deserialize;
use serde::Serialize;

use super::{ProductRunServiceError, RunRecord, SharedConversation, filesystem, invalid};
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

#[derive(Serialize, Deserialize)]
struct PersistedRecord {
    run_id: String,
    workspace_id: String,
    writer: String,
    reviewer: String,
    fixer: String,
    phase: u16,
    cycle: u32,
    task: String,
    status: String,
    diff: String,
    gates: String,
    review: String,
    summary: String,
    #[serde(default)]
    messages: Vec<PersistedMessage>,
}

#[derive(Serialize, Deserialize)]
struct PersistedMessage {
    role: u16,
    content: String,
}

pub(super) fn persist_record(
    directory: &Path,
    record: &RunRecord,
) -> Result<(), ProductRunServiceError> {
    let persisted = PersistedRecord::from_record(record)?;
    let bytes =
        serde_json::to_vec_pretty(&persisted).map_err(|_| ProductRunServiceError::Unavailable)?;
    let path = directory.join(format!("{}.json", persisted.run_id));
    let temporary = path.with_extension("json.new");
    fs::write(&temporary, bytes).map_err(|_| ProductRunServiceError::Unavailable)?;
    fs::rename(temporary, path).map_err(|_| ProductRunServiceError::Unavailable)
}

pub(super) fn load_records(directory: &Path) -> Result<BTreeMap<RunId, RunRecord>, DaemonError> {
    let mut records = BTreeMap::new();
    for entry in fs::read_dir(directory).map_err(filesystem)? {
        let entry = entry.map_err(filesystem)?;
        if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let bytes = fs::read(entry.path()).map_err(filesystem)?;
        let persisted: PersistedRecord = serde_json::from_slice(&bytes).map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::CorruptState,
                DaemonRecovery::Reconcile,
                "load product run",
                "product-run state is malformed",
                error,
            )
        })?;
        let record = persisted
            .into_record()
            .map_err(|_| invalid("product-run state contains invalid values"))?;
        records.insert(record.request.run_id(), record);
    }
    Ok(records)
}

impl PersistedRecord {
    fn from_record(record: &RunRecord) -> Result<Self, ProductRunServiceError> {
        let snapshot = &record.snapshot;
        let providers = snapshot.providers();
        let messages = record
            .conversation
            .messages()?
            .into_iter()
            .map(|message| PersistedMessage {
                role: message.role().tag(),
                content: message.content().to_owned(),
            })
            .collect();
        Ok(Self {
            run_id: hex(snapshot.run_id().as_bytes()),
            workspace_id: hex(snapshot.workspace_id().as_bytes()),
            writer: hex(providers.writer().as_bytes()),
            reviewer: hex(providers.reviewer().as_bytes()),
            fixer: hex(providers.fixer().as_bytes()),
            phase: snapshot.phase().tag(),
            cycle: snapshot.cycle(),
            task: snapshot.task().to_owned(),
            status: snapshot.status().to_owned(),
            diff: snapshot.diff().to_owned(),
            gates: snapshot.gates().to_owned(),
            review: snapshot.review().to_owned(),
            summary: snapshot.summary().to_owned(),
            messages,
        })
    }

    fn into_record(self) -> Result<RunRecord, ProductRunServiceError> {
        let run_id =
            RunId::new(unhex(&self.run_id)?).map_err(|_| ProductRunServiceError::InvalidMessage)?;
        let workspace_id = WorkspaceId::new(unhex(&self.workspace_id)?)
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        let providers = ProductProviderSelection::new(
            profile(&self.writer)?,
            profile(&self.reviewer)?,
            profile(&self.fixer)?,
        );
        let request = ProductRunRequest::new(run_id, workspace_id, providers, self.task.clone())
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        let loaded_phase =
            ProductRunPhase::from_tag(self.phase).ok_or(ProductRunServiceError::InvalidMessage)?;
        let (phase, status) = if loaded_phase.terminal() {
            (loaded_phase, self.status)
        } else {
            (
                ProductRunPhase::RecoveryRequired,
                "Daemon restart interrupted this run; retry is available".to_owned(),
            )
        };
        let mut messages = self
            .messages
            .into_iter()
            .map(|message| {
                let role = ProductConversationRole::from_tag(message.role)
                    .ok_or(ProductRunServiceError::InvalidMessage)?;
                ProductConversationMessage::new(role, message.content)
                    .map_err(|_| ProductRunServiceError::InvalidMessage)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if messages.is_empty() {
            messages.push(
                ProductConversationMessage::new(ProductConversationRole::User, self.task.clone())
                    .map_err(|_| ProductRunServiceError::InvalidMessage)?,
            );
            if loaded_phase.terminal() && !self.summary.trim().is_empty() {
                messages.push(
                    ProductConversationMessage::new(
                        ProductConversationRole::Agent,
                        format!("{}: {}", status, self.summary),
                    )
                    .map_err(|_| ProductRunServiceError::InvalidMessage)?,
                );
            }
        }
        let conversation = SharedConversation::new(run_id, messages)?;
        let snapshot = ProductRunSnapshot::new(
            run_id,
            workspace_id,
            providers,
            phase,
            self.cycle,
            self.task,
            status,
            self.diff,
            self.gates,
            self.review,
            self.summary,
        )
        .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        Ok(RunRecord {
            request,
            snapshot,
            cancelled: Arc::new(AtomicBool::new(false)),
            provider_cancellation: CancellationToken::new(),
            conversation,
        })
    }
}

fn profile(value: &str) -> Result<ProviderProfileId, ProductRunServiceError> {
    ProviderProfileId::new(unhex(value)?).map_err(|_| ProductRunServiceError::InvalidMessage)
}

fn hex(bytes: &[u8; 16]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        use core::fmt::Write as _;
        let _ = write!(text, "{byte:02x}");
        text
    })
}

fn unhex(value: &str) -> Result<[u8; 16], ProductRunServiceError> {
    if value.len() != 32 {
        return Err(ProductRunServiceError::InvalidMessage);
    }
    let mut bytes = [0_u8; 16];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(pair).map_err(|_| ProductRunServiceError::InvalidMessage)?;
        bytes[index] =
            u8::from_str_radix(text, 16).map_err(|_| ProductRunServiceError::InvalidMessage)?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_run_without_messages_gains_a_resumable_conversation() {
        let json = r#"{
            "run_id":"01010101010101010101010101010101",
            "workspace_id":"02020202020202020202020202020202",
            "writer":"03030303030303030303030303030303",
            "reviewer":"04040404040404040404040404040404",
            "fixer":"05050505050505050505050505050505",
            "phase":8,
            "cycle":1,
            "task":"build tetris",
            "status":"parse model file plan failed",
            "diff":"",
            "gates":"",
            "review":"",
            "summary":"invalid escape"
        }"#;
        let persisted: PersistedRecord = serde_json::from_str(json).expect("legacy record");
        let record = persisted.into_record().expect("migrated record");
        let conversation = record.conversation.snapshot().expect("conversation");

        assert_eq!(record.snapshot.phase(), ProductRunPhase::Failed);
        assert_eq!(conversation.messages().len(), 2);
        assert_eq!(conversation.messages()[0].content(), "build tetris");
        assert!(conversation.messages()[1].content().contains("invalid escape"));
    }
}
