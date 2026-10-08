//! Persist exact prepared view and reconstruction artifacts before atomic publication.

use super::super::{
    checkpoint_validation, error,
    record::{
        CHECKPOINT_SCHEMA_VERSION, CONTEXT_UPDATE_ENTRY_PAGE_SCHEMA_VERSION,
        CONTEXT_UPDATE_PAGE_SCHEMA_VERSION, CONTEXT_UPDATE_REDUCER_SCHEMA_VERSION,
        CONTEXT_UPDATE_SCHEMA_VERSION, CheckpointManifest, ContextUpdateEntryPage,
        ContextUpdateEntryIdentity, ContextUpdateEntryStatus, ContextUpdateRecord,
        ContextUpdateReducer, ContextUpdateReducerPage, ContextUpdateRoot,
        ContextUpdateTranscriptPage, MemoryRecord, RootContextUpdate, TranscriptManifest, decode,
        encode,
    },
    storage::{LocalStore, StoredArtifact},
    view_binding,
};
use super::LocalMemory;
use peritus_agent::{DeveloperLoopError, estimate_developer_request_tokens};
use peritus_codec::sha256;
use peritus_context::working::{
    EncodedWorkingStatePart, ReusableWorkingStateHistory, WorkingEntryStatus, WorkingEvent,
    WorkingState, WorkingStateArtifact, WorkingStateWriteError, apply_working_event,
    encode_paged_working_state_reusing_with, encode_working_event,
};
use peritus_model_protocol::{Message, ProtocolLimits, encode_messages};
use peritus_types::Sha256Digest;
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::io::Write as _;
use std::path::Path;

const CONTEXT_UPDATE_PAGE_ENTRIES: usize = 255;

pub(super) fn reconcile_checkpoint_trace(
    path: &Path,
    manifest: &CheckpointManifest,
    artifact: StoredArtifact,
    receipt: Sha256Digest,
    validation: &super::super::record::ViewValidation,
) -> Result<(), DeveloperLoopError> {
    let payload = serde_json::to_vec(&Value::from_iter([
        ("schema_version", Value::from(manifest.schema_version)),
        ("scope", Value::from(manifest.scope)),
        ("generation", Value::from(manifest.generation)),
        ("manifest_sha256", Value::from(artifact.digest.into_bytes())),
        ("trace_receipt", Value::from(receipt.into_bytes())),
        ("manifest_bytes", Value::from(artifact.bytes)),
        ("view_sha256", Value::from(manifest.view.digest.into_bytes())),
        ("state_revision", Value::from(validation.state_revision)),
        (
            "estimated_input_tokens",
            Value::from(validation.estimated_input_tokens),
        ),
        (
            "validation",
            serde_json::to_value(validation)
                .map_err(|_| error("encode checkpoint validation"))?,
        ),
    ]))
    .map_err(|_| error("encode checkpoint trace"))?;
    crate::trace::local_memory::checkpoint_once(path, receipt, &payload)
}

impl LocalMemory {
    pub(in crate::local_context) fn publish(
        &mut self,
        messages: &[Message],
    ) -> Result<(), DeveloperLoopError> {
        let prepared =
            self.prepared.as_ref().ok_or_else(|| error("no prepared view to publish"))?;
        let profile = self.profile.as_ref().ok_or_else(|| error("prepared view lacks profile"))?;
        let tool_policy = view_binding::tool_policy(&self.tools)?;
        let render_policy = sha256(&encode(&self.config)?).into_bytes();
        if prepared.messages != messages
            || prepared.validation.state_revision != self.state.revision()
            || prepared.through_event != self.store.sequence()
            || prepared.validation.model_revision != self.model_revision
            || prepared.validation.profile != profile.profile_id().into_bytes()
            || prepared.validation.profile_revision != profile.revision()
            || prepared.validation.max_input_tokens != profile.limits().max_input_tokens()
            || prepared.validation.estimated_input_tokens
                != estimate_developer_request_tokens(messages, &self.tools)
            || prepared.validation.local_compactor_failures != self.local_compactor_failures
            || prepared.validation.retrieval_calls != self.retrieval_calls
            || prepared.validation.tool_policy != Some(tool_policy)
            || prepared.validation.segment_continuation != self.segment_continuation
            || prepared.policy.into_bytes() != render_policy
        {
            return Err(error("stale or changed prepared view"));
        }
        checkpoint_validation::validate_checkpoint(
            CHECKPOINT_SCHEMA_VERSION,
            &self.state,
            &self.sources,
            &self.transcript,
            &prepared.validation,
            self.limits,
        )?;
        let validation = prepared.validation.clone();
        let prepared_messages = prepared.messages.clone();
        let (source_index, transcript_manifest) = self.publish_checkpoint_index()?;
        let through_event = self.store.sequence();
        let (working_state, working_history) = self.store_working_snapshot(source_index)?;
        let view_bytes = encode_messages(messages, ProtocolLimits::PRODUCTION)?;
        let view = self.store.store(&view_bytes)?;
        let validation_artifact = self.store.store(&encode(&validation)?)?;
        let previous = self
            .last_checkpoint
            .as_ref()
            .map(|manifest| encode(manifest).map(|bytes| sha256(&bytes).into_bytes()))
            .transpose()?;
        let generation =
            self.store.generation().checked_add(1).ok_or_else(|| error("generation overflow"))?;
        let scope = self.store.scope_digest().into_bytes();
        let view_binding = view_binding::checkpoint(
            scope,
            generation,
            through_event,
            render_policy,
            &view_bytes,
            &validation,
        )?;
        let manifest = CheckpointManifest {
            schema_version: CHECKPOINT_SCHEMA_VERSION,
            scope,
            generation,
            previous,
            through_event,
            working_state,
            transcript_manifest,
            source_index,
            view,
            render_policy,
            validation: validation_artifact,
            view_binding: Some(view_binding),
        };
        let bytes = encode(&manifest)?;
        let artifact = self.store.store_bundle(
            &bytes,
            &[working_state, transcript_manifest, view, validation_artifact],
        )?;
        let event = encode(&MemoryRecord::Checkpoint { manifest: artifact })?;
        let roots = [artifact.digest];
        let previous_owner = self.last_checkpoint_owner;
        let receipt =
            self.store.append(&event, &roots, Some((self.store.generation(), bytes)))?;
        self.last_checkpoint = Some(manifest.clone());
        self.last_checkpoint_owner = Some(receipt.owner());
        self.working_history = Some(working_history);
        self.last_view = prepared_messages;
        self.prepared = None;
        if let Some(owner) = previous_owner
            && let Err(retire) = self.store.retire_checkpoint_owner(owner)
        {
            use std::io::Write as _;
            let _ = writeln!(
                std::io::stderr().lock(),
                "obsolete local checkpoint reference retirement deferred: {retire}"
            );
        }
        if let Err(trace) =
            reconcile_checkpoint_trace(
                &self.trace_path,
                &manifest,
                artifact,
                receipt.owner(),
                &validation,
            )
        {
            use std::io::Write as _;
            let _ = writeln!(
                std::io::stderr().lock(),
                "local checkpoint trace observation deferred after durable commit: {trace}"
            );
        }
        Ok(())
    }

    pub(super) fn commit_context_update_root(
        &mut self,
        base_model_revision: u64,
        events: &[WorkingEvent],
        transcript: &TranscriptManifest,
    ) -> Result<(), DeveloperLoopError> {
        let (source_index, _) = self.publish_checkpoint_index()?;
        let reducer_head =
            store_context_reducer_pages(&self.store, &self.state, events, source_index)?;
        let reducer_count = u64::try_from(events.len())
            .map_err(|_| error("context update reducer count overflow"))?;
        let transcript_before = transcript_digest(&self.transcript)?;
        let transcript_after = transcript_digest(transcript)?;
        let (transcript_head, transcript_change_count) =
            store_context_transcript_pages(
                &self.store,
                &self.transcript,
                transcript,
            )?;
        let root = ContextUpdateRoot {
            schema_version: CONTEXT_UPDATE_SCHEMA_VERSION,
            base_model_revision,
            source_index,
            reducer_head,
            reducer_count,
            transcript_before,
            transcript_after,
            transcript_head,
            transcript_change_count,
        };
        let root_bytes = encode(&root)?;
        let mut children = Vec::with_capacity(3);
        children.push(source_index);
        children.push(reducer_head);
        if let Some(transcript_head) = transcript_head {
            children.push(transcript_head);
        }
        let root_artifact = self.store.store_bundle(&root_bytes, &children)?;
        let record = MemoryRecord::ContextUpdate(ContextUpdateRecord::Root(
            RootContextUpdate {
                schema_version: CONTEXT_UPDATE_SCHEMA_VERSION,
                root: root_artifact,
            },
        ));
        self.commit(&record, &[root_artifact.digest])
    }

    fn store_working_snapshot(
        &self,
        source_index: StoredArtifact,
    ) -> Result<(StoredArtifact, ReusableWorkingStateHistory), DeveloperLoopError> {
        store_working_snapshot_reusing(
            &self.store,
            &self.state,
            source_index,
            self.working_history.as_ref(),
        )
    }

    pub(super) fn reconcile_last_checkpoint_trace(&self) -> Result<(), DeveloperLoopError> {
        let (Some(manifest), Some(receipt)) =
            (self.last_checkpoint.as_ref(), self.last_checkpoint_owner)
        else {
            return Ok(());
        };
        let manifest_bytes = encode(manifest)?;
        let artifact = StoredArtifact {
            digest: sha256(&manifest_bytes),
            bytes: u64::try_from(manifest_bytes.len())
                .map_err(|_| error("checkpoint manifest size overflow"))?,
        };
        let validation = decode(&self.store.read(manifest.validation)?)?;
        reconcile_checkpoint_trace(
            &self.trace_path,
            manifest,
            artifact,
            receipt,
            &validation,
        )
    }
}

pub(super) fn store_working_snapshot(
    store: &LocalStore,
    state: &WorkingState,
    source_index: StoredArtifact,
) -> Result<StoredArtifact, DeveloperLoopError> {
    store_working_snapshot_reusing(store, state, source_index, None)
        .map(|(artifact, _)| artifact)
}

fn store_working_snapshot_reusing(
    store: &LocalStore,
    state: &WorkingState,
    source_index: StoredArtifact,
    history: Option<&ReusableWorkingStateHistory>,
) -> Result<(StoredArtifact, ReusableWorkingStateHistory), DeveloperLoopError> {
    let source = WorkingStateArtifact::new(
        source_index.digest.into_bytes(),
        source_index.bytes,
    )
    .map_err(|_| error("invalid source-index checkpoint reference"))?;
    let mut data = Vec::new();
    data.try_reserve_exact(255)
        .map_err(|_| error("allocate working checkpoint page index"))?;
    let mut descriptor_tail = None;
    let mut covered = 0_usize;
    let (root, history) = encode_paged_working_state_reusing_with(
        state,
        source,
        history,
        |part| {
        match part {
            EncodedWorkingStatePart::Data(page) => {
                let stored = store.store(page.bytes())?;
                if stored != stored_artifact(page.artifact()) {
                    return Err(error(
                        "stored working page differs from its canonical descriptor",
                    ));
                }
                data.push(stored);
            }
            EncodedWorkingStatePart::Reused(page) => {
                data.push(stored_artifact(page.artifact()));
            }
            EncodedWorkingStatePart::Descriptor(page) => {
                let first = usize::try_from(page.first())
                    .map_err(|_| error("working descriptor page offset overflow"))?;
                let count = usize::try_from(page.count())
                    .map_err(|_| error("working descriptor page count overflow"))?;
                let end = first
                    .checked_add(count)
                    .ok_or_else(|| error("working descriptor page range overflow"))?;
                if first != covered || count != data.len() {
                    return Err(error(
                        "working descriptor pages do not cover canonical data pages",
                    ));
                }
                let mut children = Vec::new();
                children
                    .try_reserve_exact(count.saturating_add(1))
                    .map_err(|_| error("allocate working descriptor child index"))?;
                if let Some(previous) = descriptor_tail {
                    children.push(previous);
                }
                children.extend_from_slice(&data);
                let stored = store.store_bundle(page.bytes(), &children)?;
                if stored != stored_artifact(page.artifact()) {
                    return Err(error(
                        "stored descriptor page differs from its canonical index",
                    ));
                }
                descriptor_tail = Some(stored);
                covered = end;
                data.clear();
            }
        }
        Ok(())
    },
    )
    .map_err(|failure| match failure {
        WorkingStateWriteError::Artifact(failure) => failure,
        WorkingStateWriteError::Codec(_) => error("encode paged working checkpoint"),
    })?;
    if !data.is_empty() {
        return Err(error("working descriptor chain is incomplete"));
    }

    let mut children = Vec::with_capacity(2);
    children.push(source_index);
    if let Some(tail) = descriptor_tail {
        children.push(tail);
    }
    let artifact = store.store_bundle(&root, &children)?;
    Ok((artifact, history))
}

fn stored_artifact(artifact: WorkingStateArtifact) -> StoredArtifact {
    StoredArtifact {
        digest: Sha256Digest::new(artifact.digest()),
        bytes: artifact.bytes(),
    }
}

fn store_context_reducer_pages(
    store: &LocalStore,
    initial: &WorkingState,
    events: &[WorkingEvent],
    source_index: StoredArtifact,
) -> Result<StoredArtifact, DeveloperLoopError> {
    if events.is_empty() {
        return Err(error("context update has no reducer page"));
    }
    let mut current = initial.clone();
    let mut reducer_artifacts = Vec::new();
    reducer_artifacts
        .try_reserve_exact(events.len())
        .map_err(|_| error("allocate context update reducer index"))?;
    for event in events {
        let successor = apply_working_event(&current, event)
            .map_err(|_| error("context update reducer rejected during storage"))?;
        let (descriptor, children) = match event {
            WorkingEvent::Refresh { base_revision, .. } => {
                let state = store_working_snapshot(store, &successor, source_index)?;
                (
                    ContextUpdateReducer::Refresh {
                        schema_version: CONTEXT_UPDATE_REDUCER_SCHEMA_VERSION,
                        base_revision: *base_revision,
                        state,
                    },
                    vec![state],
                )
            }
            WorkingEvent::Delta(delta) => {
                if let Ok(bytes) = encode_working_event(event) {
                    let event = store.store(&bytes)?;
                    (
                        ContextUpdateReducer::Event {
                            schema_version: CONTEXT_UPDATE_REDUCER_SCHEMA_VERSION,
                            event,
                        },
                        vec![event],
                    )
                } else {
                    let state = store_working_snapshot(store, &successor, source_index)?;
                    let entry_head = store_context_entry_pages(store, delta.entries())?;
                    let entry_count = u64::try_from(delta.entries().len())
                        .map_err(|_| error("context update delta entry count overflow"))?;
                    (
                        ContextUpdateReducer::DeltaSnapshot {
                            schema_version: CONTEXT_UPDATE_REDUCER_SCHEMA_VERSION,
                            base_revision: delta.base_revision(),
                            state,
                            entry_head,
                            entry_count,
                        },
                        vec![state, entry_head],
                    )
                }
            }
            WorkingEvent::Observation { .. } | WorkingEvent::Protocol(_) => {
                return Err(error("invalid context update reducer sequence"));
            }
        };
        reducer_artifacts.push(store.store_bundle(&encode(&descriptor)?, &children)?);
        current = successor;
    }

    let mut next = None;
    let mut first = reducer_artifacts.len();
    for reducers in reducer_artifacts.rchunks(CONTEXT_UPDATE_PAGE_ENTRIES) {
        first = first
            .checked_sub(reducers.len())
            .ok_or_else(|| error("context update reducer page underflow"))?;
        let page = ContextUpdateReducerPage {
            schema_version: CONTEXT_UPDATE_PAGE_SCHEMA_VERSION,
            next,
            first_reducer: u64::try_from(first)
                .map_err(|_| error("context update reducer offset overflow"))?,
            reducers: reducers.to_vec(),
        };
        let bytes = encode(&page)?;
        let mut children = Vec::new();
        children
            .try_reserve_exact(page.reducers.len().saturating_add(1))
            .map_err(|_| error("allocate context update reducer child index"))?;
        if let Some(next) = next {
            children.push(next);
        }
        children.extend_from_slice(&page.reducers);
        next = Some(store.store_bundle(&bytes, &children)?);
    }
    next.ok_or_else(|| error("context update has no reducer page"))
}

fn store_context_entry_pages(
    store: &LocalStore,
    entries: &[peritus_context::working::WorkingEntry],
) -> Result<StoredArtifact, DeveloperLoopError> {
    let mut next = None;
    let mut first = entries.len();
    for entries in entries.rchunks(CONTEXT_UPDATE_PAGE_ENTRIES) {
        first = first
            .checked_sub(entries.len())
            .ok_or_else(|| error("context update entry page underflow"))?;
        let page = ContextUpdateEntryPage {
            schema_version: CONTEXT_UPDATE_ENTRY_PAGE_SCHEMA_VERSION,
            next,
            first_entry: u64::try_from(first)
                .map_err(|_| error("context update entry offset overflow"))?,
            entries: entries
                .iter()
                .map(|entry| {
                    let status = match entry.status() {
                        WorkingEntryStatus::Open => ContextUpdateEntryStatus::Open,
                        WorkingEntryStatus::Contradicted => {
                            ContextUpdateEntryStatus::Contradicted
                        }
                        WorkingEntryStatus::Resolved => ContextUpdateEntryStatus::Resolved,
                        WorkingEntryStatus::Stale | WorkingEntryStatus::Superseded => {
                            return Err(error("context update delta contains a derived status"));
                        }
                    };
                    Ok(ContextUpdateEntryIdentity {
                        id: entry.id().into_bytes(),
                        status,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        let bytes = encode(&page)?;
        next = Some(if let Some(child) = next {
            store.store_bundle(&bytes, &[child])?
        } else {
            store.store_bundle(&bytes, &[])?
        });
    }
    next.ok_or_else(|| error("context update delta has no entry page"))
}

fn store_context_transcript_pages(
    store: &LocalStore,
    before: &TranscriptManifest,
    after: &TranscriptManifest,
) -> Result<(Option<StoredArtifact>, u64), DeveloperLoopError> {
    let mut pages = plan_context_transcript_pages(before, after)?;
    let change_count = if let Some(page) = pages.last() {
        let page_count = page
            .files_removed
            .len()
            .checked_add(page.files_added.len())
            .and_then(|count| u64::try_from(count).ok())
            .ok_or_else(|| error("context update transcript change count overflow"))?;
        page.first_change
            .checked_add(page_count)
            .ok_or_else(|| error("context update transcript change count overflow"))?
    } else {
        0
    };
    let mut next = None;
    for page in pages.iter_mut().rev() {
        page.next = next;
        let bytes = encode(page)?;
        next = Some(if let Some(child) = page.next {
            store.store_bundle(&bytes, &[child])?
        } else {
            store.store_bundle(&bytes, &[])?
        });
    }
    Ok((next, change_count))
}

fn plan_context_transcript_pages(
    before: &TranscriptManifest,
    after: &TranscriptManifest,
) -> Result<Vec<ContextUpdateTranscriptPage>, DeveloperLoopError> {
    let mut expected = before.clone();
    expected.files.clone_from(&after.files);
    if &expected != after {
        return Err(error("context update changed a host-owned transcript field"));
    }
    let mut current = before.clone();
    let mut before_index = 0_usize;
    let mut after_index = 0_usize;
    let mut first_change = 0_u64;
    let mut pages = Vec::new();
    while before_index < before.files.len() || after_index < after.files.len() {
        let mut files_removed = Vec::new();
        let mut files_added = Vec::new();
        while files_removed
            .len()
            .checked_add(files_added.len())
            .is_some_and(|count| count < CONTEXT_UPDATE_PAGE_ENTRIES)
            && (before_index < before.files.len() || after_index < after.files.len())
        {
            match (before.files.get(before_index), after.files.get(after_index)) {
                (Some(old), Some(new)) if old == new => {
                    before_index += 1;
                    after_index += 1;
                }
                (Some(old), Some(new)) if old < new => {
                    files_removed.push(old.clone());
                    before_index += 1;
                }
                (Some(_), Some(new)) => {
                    files_added.push(new.clone());
                    after_index += 1;
                }
                (Some(old), None) => {
                    files_removed.push(old.clone());
                    before_index += 1;
                }
                (None, Some(new)) => {
                    files_added.push(new.clone());
                    after_index += 1;
                }
                (None, None) => break,
            }
        }
        if files_removed.is_empty() && files_added.is_empty() {
            continue;
        }
        let before_digest = transcript_digest(&current)?;
        apply_context_file_changes(&mut current, &files_removed, &files_added)?;
        let after_digest = transcript_digest(&current)?;
        let count = files_removed
            .len()
            .checked_add(files_added.len())
            .and_then(|count| u64::try_from(count).ok())
            .ok_or_else(|| error("context update transcript change count overflow"))?;
        pages.push(ContextUpdateTranscriptPage {
            schema_version: CONTEXT_UPDATE_PAGE_SCHEMA_VERSION,
            next: None,
            first_change,
            before: before_digest,
            after: after_digest,
            files_removed,
            files_added,
        });
        first_change = first_change
            .checked_add(count)
            .ok_or_else(|| error("context update transcript change count overflow"))?;
    }
    if &current != after {
        return Err(error("context update transcript page plan is incomplete"));
    }
    Ok(pages)
}

pub(super) fn apply_context_transcript_page(
    transcript: &mut TranscriptManifest,
    page: &ContextUpdateTranscriptPage,
    expected_first_change: u64,
) -> Result<u64, DeveloperLoopError> {
    let count = page
        .files_removed
        .len()
        .checked_add(page.files_added.len())
        .ok_or_else(|| error("context update transcript change count overflow"))?;
    if page.schema_version != CONTEXT_UPDATE_PAGE_SCHEMA_VERSION
        || page.first_change != expected_first_change
        || count == 0
        || count > CONTEXT_UPDATE_PAGE_ENTRIES
        || transcript_digest(transcript)? != page.before
    {
        return Err(error("invalid context update transcript page"));
    }
    apply_context_file_changes(transcript, &page.files_removed, &page.files_added)?;
    if transcript_digest(transcript)? != page.after {
        return Err(error("context update transcript page digest mismatch"));
    }
    expected_first_change
        .checked_add(
            u64::try_from(count)
                .map_err(|_| error("context update transcript change count overflow"))?,
        )
        .ok_or_else(|| error("context update transcript change count overflow"))
}

fn apply_context_file_changes(
    transcript: &mut TranscriptManifest,
    files_removed: &[String],
    files_added: &[String],
) -> Result<(), DeveloperLoopError> {
    if files_removed.windows(2).any(|pair| pair[0] >= pair[1])
        || files_added.windows(2).any(|pair| pair[0] >= pair[1])
        || files_removed.iter().any(String::is_empty)
        || files_added.iter().any(String::is_empty)
        || files_removed
            .iter()
            .any(|path| files_added.binary_search(path).is_ok())
        || files_removed
            .iter()
            .any(|path| transcript.files.binary_search(path).is_err())
        || files_added
            .iter()
            .any(|path| transcript.files.binary_search(path).is_ok())
    {
        return Err(error("invalid context update transcript file delta"));
    }
    transcript
        .files
        .retain(|path| files_removed.binary_search(path).is_err());
    transcript.files.extend_from_slice(files_added);
    transcript.files.sort();
    Ok(())
}

pub(super) fn transcript_digest(
    transcript: &TranscriptManifest,
) -> Result<[u8; 32], DeveloperLoopError> {
    struct DigestWriter(Sha256);
    impl std::io::Write for DigestWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = DigestWriter(Sha256::new());
    serde_json::to_writer(&mut writer, transcript)
        .map_err(|_| error("hash context update transcript"))?;
    writer
        .flush()
        .map_err(|_| error("hash context update transcript"))?;
    Ok(writer.0.finalize().into())
}
