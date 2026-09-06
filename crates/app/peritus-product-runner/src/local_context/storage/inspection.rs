//! Read-only inspection of one exact published checkpoint while its writer may still be active.

use super::super::{
    error,
    record::{ArchivedObservation, CheckpointManifest, TranscriptManifest, ViewValidation, decode},
    tools::hex,
};
use super::{
    MAX_ARTIFACT_BYTES, STATE_KEY, STATE_NAMESPACE, StoredArtifact, identity::StorageIdentity,
};
use peritus_agent::DeveloperLoopError;
use peritus_artifact_store::{ArtifactDigest, ArtifactStore, StoreConfig};
use peritus_context::working::{
    ObservationId, WorkingBinding, WorkingLimits, decode_working_state,
};
use peritus_journal::JournalReader;
use peritus_model_protocol::{ContentBlock, Message, ProtocolLimits, decode_messages};
use serde_json::Value;
use std::path::Path;

pub(in crate::local_context) fn inspect(
    root: &Path,
    binding: WorkingBinding,
) -> Result<String, DeveloperLoopError> {
    let identity = StorageIdentity::new(binding)?;
    let database = root.join("journal.sqlite3");
    let journal = JournalReader::open(&database, identity.store)
        .map_err(|_| error("open exact read-only context journal"))?;
    let row = journal
        .state_record(STATE_NAMESPACE, STATE_KEY)
        .map_err(|_| error("read exact checkpoint root"))?
        .ok_or_else(|| error("no published model-visible view exists"))?;
    let manifest: CheckpointManifest = decode(row.bytes())?;
    if manifest.schema_version != 1
        || manifest.scope != identity.scope.into_bytes()
        || manifest.generation != row.revision()
    {
        return Err(error("inspection scope or generation mismatch"));
    }
    let config = StoreConfig::new(root.join("artifacts"), MAX_ARTIFACT_BYTES, 1024 * 1024 * 1024)
        .and_then(|config| config.with_database_path(&database))
        .map_err(|_| error("invalid inspection artifact configuration"))?;
    let read = |artifact: StoredArtifact| -> Result<Vec<u8>, DeveloperLoopError> {
        let bytes = ArtifactStore::read_existing(
            &config,
            ArtifactDigest::from_sha256(artifact.digest),
            MAX_ARTIFACT_BYTES,
        )
        .map_err(|_| error("checkpoint artifact missing or corrupt"))?;
        if bytes.len() as u64 != artifact.bytes {
            return Err(error("checkpoint artifact size mismatch"));
        }
        Ok(bytes)
    };
    let validation: ViewValidation = decode(&read(manifest.validation)?)?;
    let sources: Vec<ArchivedObservation> = decode(&read(manifest.source_index)?)?;
    let state =
        decode_working_state(&read(manifest.working_state)?, binding, WorkingLimits::standard())
            .map_err(|_| error("invalid inspection working state"))?;
    let transcript: TranscriptManifest = decode(&read(manifest.transcript_manifest)?)?;
    if validation.state_revision != state.revision()
        || validation.through_observation != state.through_observation()
        || sources.len() as u64 != state.through_observation()
        || validation.estimated_input_tokens > validation.max_input_tokens
        || validation.max_input_tokens == 0
    {
        return Err(error("inspection validation does not bind exact state"));
    }
    for (index, source) in sources.iter().enumerate() {
        if source.sequence != index as u64 + 1 || source.invocation > transcript.invocation {
            return Err(error("invalid inspection source index"));
        }
        let locator = state
            .observation(
                state.binding(),
                ObservationId::new(source.sequence)
                    .map_err(|_| error("invalid inspection source"))?,
            )
            .map_err(|_| error("unresolved inspection source"))?;
        source.validate_locator(locator)?;
    }
    let archive = read(manifest.view)?;
    let messages = decode_messages(&archive, ProtocolLimits::PRODUCTION)?;
    let readable = readable_messages(&messages);
    let head = journal
        .head(identity.aggregate)
        .map_err(|_| error("read context inspection head"))?
        .ok_or_else(|| error("missing checkpoint producing journal"))?;
    let output = Value::from_iter([
        ("schema_version", Value::from(1)),
        ("description", Value::from("Exact last published model-visible view. If uncovered_tail is true, the next view has not yet been published; this command does not assemble or recover it.")),
        ("uncovered_tail", Value::from(head.sequence().get() > manifest.through_event.saturating_add(1))),
        ("checkpoint", serde_json::to_value(&manifest).map_err(|_| error("encode inspection manifest"))?),
        ("validation", serde_json::to_value(&validation).map_err(|_| error("encode inspection validation"))?),
        ("source_index", serde_json::to_value(&sources).map_err(|_| error("encode inspection sources"))?),
        ("readable_messages", Value::from(readable)),
        ("canonical_view_archive_hex", Value::from(hex(&archive))),
    ]).to_string();
    if output.len() > 32 * 1024 * 1024 {
        return Err(error("exact inspection exceeds output capacity"));
    }
    Ok(output)
}

fn readable_messages(messages: &[Message]) -> Vec<Value> {
    messages
        .iter()
        .map(|message| {
            Value::from_iter([
                ("role", Value::from(format!("{:?}", message.role()))),
                (
                    "content",
                    Value::from(
                        message
                            .content()
                            .iter()
                            .map(|block| match block {
                                ContentBlock::Text(text) => Value::from_iter([
                                    ("kind", Value::from("text")),
                                    ("text", Value::from(text.expose_for_wire())),
                                ]),
                                ContentBlock::ToolCall(call) => Value::from_iter([
                                    ("kind", Value::from("tool_call")),
                                    ("id", Value::from(call.id().expose_for_wire())),
                                    ("name", Value::from(call.name().as_str())),
                                    ("arguments", Value::from(call.arguments().to_wire_string())),
                                ]),
                                ContentBlock::ToolResult(result) => Value::from_iter([
                                    ("kind", Value::from("tool_result")),
                                    ("id", Value::from(result.call_id().expose_for_wire())),
                                    ("output", Value::from(result.output().to_wire_string())),
                                    ("is_error", Value::from(result.is_error())),
                                ]),
                                _ => Value::from_iter([
                                    ("kind", Value::from("non_text")),
                                    (
                                        "exact_bytes",
                                        Value::from("retained in canonical_view_archive_hex"),
                                    ),
                                ]),
                            })
                            .collect::<Vec<_>>(),
                    ),
                ),
            ])
        })
        .collect::<Vec<_>>()
}
