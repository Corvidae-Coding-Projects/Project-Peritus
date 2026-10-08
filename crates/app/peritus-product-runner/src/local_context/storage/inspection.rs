//! Read-only inspection of one exact published checkpoint while its writer may still be active.

use super::super::{
    checkpoint_validation, error,
    memory::checkpoint_index::read_complete_checkpoint_indexes,
    record::{
        ArchivedObservation, CHECKPOINT_SCHEMA_VERSION, CheckpointManifest,
        INDEXED_CHECKPOINT_SCHEMA_VERSION, LEGACY_CHECKPOINT_SCHEMA_VERSION,
        PAGED_CHECKPOINT_SCHEMA_VERSION, SNAPSHOT_CHECKPOINT_SCHEMA_VERSION,
        TranscriptManifest, ViewValidation, decode,
    },
    tools::hex,
    view_binding,
};
use super::{STATE_KEY, STATE_NAMESPACE, StoredArtifact, identity::StorageIdentity};
use peritus_agent::DeveloperLoopError;
use peritus_artifact_store::{ArtifactDigest, ArtifactReadChunk, ArtifactReadHandle, ArtifactStore, StoreConfig};
use peritus_context::working::{
    ObservationId, ObservationSource, WorkingBinding, WorkingLimits, decode_working_state,
    WorkingStateArtifact, WorkingStateReadError, decode_paged_working_state_from,
    decode_working_state_core,
};
use peritus_journal::JournalReader;
use peritus_model_protocol::{ContentBlock, Message, ProtocolLimits, decode_messages};
use peritus_types::Sha256Digest;
use serde_json::Value;
use std::path::Path;

pub(in crate::local_context) fn inspect(
    root: &Path,
    binding: WorkingBinding,
) -> Result<String, DeveloperLoopError> {
    let mut inspection = open(root, binding)?;
    let mut archive = Vec::new();
    while let Some(chunk) = inspection.read_view(archive.len() as u64, 64 * 1024)? {
        archive.try_reserve(chunk.bytes().len()).map_err(|_| error("allocate exact inspection archive"))?;
        archive.extend_from_slice(chunk.bytes());
    }
    Ok(Value::from_iter([
        ("schema_version", Value::from(1)),
        ("description", Value::from("Exact last published model-visible view. If uncovered_tail is true, the next view has not yet been published; this command does not assemble or recover it.")),
        ("uncovered_tail", Value::from(inspection.uncovered_tail)),
        ("checkpoint", serde_json::to_value(&inspection.manifest).map_err(|_| error("encode inspection manifest"))?),
        ("validation", serde_json::to_value(&inspection.validation).map_err(|_| error("encode inspection validation"))?),
        ("source_index", serde_json::to_value(&inspection.sources).map_err(|_| error("encode inspection sources"))?),
        ("readable_messages", Value::from(readable_messages(&inspection.messages, "retained in canonical_view_archive_hex"))),
        ("canonical_view_archive_hex", Value::from(hex(&archive))),
    ]).to_string())
}

/// One immutable, verified inspection frontier. Paging never follows a newer checkpoint.
pub struct LocalContextInspection {
    manifest: CheckpointManifest,
    validation: ViewValidation,
    sources: Vec<ArchivedObservation>,
    messages: Vec<Message>,
    view: ArtifactReadHandle,
    uncovered_tail: bool,
}

impl LocalContextInspection {
    /// Returns the exact checkpoint identity and counts for this retained frontier.
    ///
    /// # Errors
    /// Returns a JSON encoding failure; no context state is mutated.
    pub fn summary(&self) -> Result<String, DeveloperLoopError> {
        Ok(Value::from_iter([
            ("schema_version", Value::from(2)),
            ("uncovered_tail", Value::from(self.uncovered_tail)),
            ("checkpoint", serde_json::to_value(&self.manifest).map_err(|_| error("encode inspection manifest"))?),
            ("validation", serde_json::to_value(&self.validation).map_err(|_| error("encode inspection validation"))?),
            ("source_count", Value::from(self.sources.len())),
            ("message_count", Value::from(self.messages.len())),
            ("canonical_view_bytes", Value::from(self.manifest.view.bytes)),
        ]).to_string())
    }

    /// Reads a physical page of exact source records from this checkpoint.
    ///
    /// # Errors
    /// Rejects invalid offsets or page allocations, and returns encoding failures.
    pub fn source_page(&self, offset: usize, maximum: usize) -> Result<String, DeveloperLoopError> {
        let end = page_end(self.sources.len(), offset, maximum, 256)?;
        Ok(Value::from_iter([
            ("generation", Value::from(self.manifest.generation)),
            ("canonical_view_sha256", Value::from(hex(self.manifest.view.digest.as_bytes()))),
            ("offset", Value::from(offset)),
            ("total", Value::from(self.sources.len())),
            ("next_offset", if end < self.sources.len() { Value::from(end) } else { Value::Null }),
            ("sources", serde_json::to_value(&self.sources[offset..end]).map_err(|_| error("encode inspection source page"))?),
        ]).to_string())
    }

    /// Reads a physical page of complete message protocol records.
    ///
    /// # Errors
    /// Rejects invalid offsets or page allocations. Nontext bytes remain available in `read_view`.
    pub fn message_page(&self, offset: usize, maximum: usize) -> Result<String, DeveloperLoopError> {
        let end = page_end(self.messages.len(), offset, maximum, 16)?;
        Ok(Value::from_iter([
            ("generation", Value::from(self.manifest.generation)),
            ("canonical_view_sha256", Value::from(hex(self.manifest.view.digest.as_bytes()))),
            ("offset", Value::from(offset)),
            ("total", Value::from(self.messages.len())),
            ("next_offset", if end < self.messages.len() { Value::from(end) } else { Value::Null }),
            ("messages", Value::from(readable_messages(&self.messages[offset..end], "retained in read_view canonical archive bytes"))),
        ]).to_string())
    }

    /// Reads exact canonical archive bytes, without a hex-encoded duplicate in each page.
    ///
    /// # Errors
    /// Rejects an invalid range/allocation, missing bytes, or mutation after verification.
    pub fn read_view(&mut self, offset: u64, maximum: usize) -> Result<Option<ArtifactReadChunk>, DeveloperLoopError> {
        if maximum == 0 || maximum > 64 * 1024 {
            return Err(error("inspection view page is outside the physical allocation envelope"));
        }
        self.view.read_chunk_at(offset, maximum).map_err(|_| error("exact inspection view range is unavailable or changed"))
    }
}

fn page_end(total: usize, offset: usize, maximum: usize, allocation: usize) -> Result<usize, DeveloperLoopError> {
    if offset > total || maximum == 0 || maximum > allocation {
        return Err(error("invalid exact inspection page range"));
    }
    Ok(offset.saturating_add(maximum).min(total))
}

pub(in crate::local_context) fn open(
    root: &Path,
    binding: WorkingBinding,
) -> Result<LocalContextInspection, DeveloperLoopError> {
    let identity = StorageIdentity::new(binding)?;
    let database = root.join("journal.sqlite3");
    let journal = JournalReader::open(&database, identity.store)
        .map_err(|_| error("open exact read-only context journal"))?;
    let row = journal
        .state_record(STATE_NAMESPACE, STATE_KEY)
        .map_err(|_| error("read exact checkpoint root"))?
        .ok_or_else(|| error("no published model-visible view exists"))?;
    let manifest: CheckpointManifest = decode(row.bytes())?;
    if !matches!(
        manifest.schema_version,
        LEGACY_CHECKPOINT_SCHEMA_VERSION
            | SNAPSHOT_CHECKPOINT_SCHEMA_VERSION
            | INDEXED_CHECKPOINT_SCHEMA_VERSION
            | PAGED_CHECKPOINT_SCHEMA_VERSION
            | CHECKPOINT_SCHEMA_VERSION
    ) || manifest.scope != identity.scope.into_bytes()
        || manifest.generation != row.revision()
    {
        return Err(error("inspection scope or generation mismatch"));
    }
    let config = StoreConfig::for_available_space_without_artifact_limit(root.join("artifacts"))
        .and_then(|config| config.with_database_path(&database))
        .map_err(|_| error("invalid inspection artifact configuration"))?;
    let read = |artifact: StoredArtifact| -> Result<Vec<u8>, DeveloperLoopError> {
        let bytes = ArtifactStore::read_existing(
            &config,
            ArtifactDigest::from_sha256(artifact.digest),
            artifact.bytes,
        )
        .map_err(|_| error("checkpoint artifact missing or corrupt"))?;
        if bytes.len() as u64 != artifact.bytes {
            return Err(error("checkpoint artifact size mismatch"));
        }
        Ok(bytes)
    };
    // Acquire the owned canonical-view file before inspecting the remaining frontier.
    // The handle retains exact bytes across catalog retirement; do not reopen it after reading.
    let mut view = ArtifactStore::open_existing(&config, ArtifactDigest::from_sha256(manifest.view.digest))
        .map_err(|_| error("open immutable inspection view"))?;
    if view.metadata().size() != manifest.view.bytes {
        return Err(error("inspection view length mismatch"));
    }
    if manifest.schema_version == LEGACY_CHECKPOINT_SCHEMA_VERSION {
        checkpoint_validation::validate_schema_lineage(&manifest, |digest| {
            ArtifactStore::read_existing(
                &config,
                ArtifactDigest::from_sha256(Sha256Digest::new(digest)),
                i64::MAX as u64,
            )
            .map_err(|_| error("checkpoint predecessor missing or corrupt"))
        })?;
    }
    let validation_bytes = read(manifest.validation)?;
    let validation: ViewValidation = decode(&validation_bytes)?;
    let limits = WorkingLimits::standard();
    let (sources, transcript, state) = if matches!(
        manifest.schema_version,
        INDEXED_CHECKPOINT_SCHEMA_VERSION
            | PAGED_CHECKPOINT_SCHEMA_VERSION
            | CHECKPOINT_SCHEMA_VERSION
    ) {
        let (sources, transcript, _) = read_complete_checkpoint_indexes(
            manifest.schema_version,
            manifest.source_index,
            manifest.transcript_manifest,
            &read,
        )?;
        let observations = observation_sources(&sources)?;
        let working = read(manifest.working_state)?;
        let state = if matches!(
            manifest.schema_version,
            PAGED_CHECKPOINT_SCHEMA_VERSION | CHECKPOINT_SCHEMA_VERSION
        ) {
            let source_index = WorkingStateArtifact::new(
                manifest.source_index.digest.into_bytes(), manifest.source_index.bytes,
            ).map_err(|_| error("invalid inspection paged source-index reference"))?;
            decode_paged_working_state_from(
                &working, source_index, &observations, binding, limits,
                |artifact| read(StoredArtifact {
                    digest: Sha256Digest::new(artifact.digest()), bytes: artifact.bytes(),
                }),
            ).map_err(|failure| match failure {
                WorkingStateReadError::Artifact(failure) => failure,
                WorkingStateReadError::Codec(_) => error("invalid paged inspection working state"),
            })?
        } else {
            decode_working_state_core(&working, &observations, binding, limits)
                .map_err(|_| error("invalid incremental inspection working state"))?
        };
        (sources, transcript, state)
    } else {
        let sources: Vec<ArchivedObservation> = decode(&read(manifest.source_index)?)?;
        let transcript: TranscriptManifest = decode(&read(manifest.transcript_manifest)?)?;
        let state = decode_working_state(&read(manifest.working_state)?, binding, limits)
            .map_err(|_| error("invalid inspection working state"))?;
        (sources, transcript, state)
    };
    checkpoint_validation::validate_checkpoint(
        manifest.schema_version,
        &state,
        &sources,
        &transcript,
        &validation,
        limits,
    )?;
    let mut archive = Vec::new();
    while let Some(chunk) = view.read_chunk_at(
        u64::try_from(archive.len()).map_err(|_| error("inspection archive offset overflow"))?,
        64 * 1024,
    ).map_err(|_| error("read owned inspection view"))? {
        archive.try_reserve(chunk.bytes().len())
            .map_err(|_| error("inspection view allocation unavailable"))?;
        archive.extend_from_slice(chunk.bytes());
    }
    if u64::try_from(archive.len()).map_err(|_| error("inspection archive size overflow"))? != manifest.view.bytes {
        return Err(error("inspection archive is incomplete"));
    }
    view_binding::verify(&manifest, &archive, &validation)?;
    let messages = decode_messages(&archive, ProtocolLimits::PRODUCTION)?;
    let head = journal
        .head(identity.aggregate)
        .map_err(|_| error("read context inspection head"))?
        .ok_or_else(|| error("missing checkpoint producing journal"))?;
    let uncovered_tail = head.sequence().get() > manifest.through_event.saturating_add(1);
    Ok(LocalContextInspection { manifest, validation, sources, messages, view, uncovered_tail })
}

fn observation_sources(
    sources: &[ArchivedObservation],
) -> Result<Vec<ObservationSource>, DeveloperLoopError> {
    sources
        .iter()
        .map(|source| {
            ObservationSource::new(
                ObservationId::new(source.sequence)
                    .map_err(|_| error("invalid checkpoint observation sequence"))?,
                source.artifact.digest,
                source.artifact.bytes,
                0,
                source.artifact.bytes,
                source.kind.source_kind(),
            )
            .map_err(|_| error("invalid checkpoint observation source"))
        })
        .collect()
}

fn readable_messages(messages: &[Message], archive_location: &str) -> Vec<Value> {
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
                                        Value::from(archive_location),
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
