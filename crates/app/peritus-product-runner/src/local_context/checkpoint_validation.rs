//! Shared semantic validation for recovered and read-only inspected checkpoints.

use super::{
    error,
    memory::environment,
    record::{
        ArchiveKind, ArchivedObservation, CHECKPOINT_SCHEMA_VERSION, CheckpointManifest,
        LEGACY_CHECKPOINT_SCHEMA_VERSION, TranscriptManifest, ViewValidation, decode,
    },
};
use peritus_agent::DeveloperLoopError;
use peritus_context::working::{ObservationId, WorkingEntryStatus, WorkingLimits, WorkingState};
use peritus_model_protocol::ProtocolLimits;

pub(super) fn validate_schema_lineage(
    manifest: &CheckpointManifest,
    read: impl Fn([u8; 32]) -> Result<Vec<u8>, DeveloperLoopError>,
) -> Result<(), DeveloperLoopError> {
    if manifest.schema_version != LEGACY_CHECKPOINT_SCHEMA_VERSION {
        return Ok(());
    }
    let mut current = manifest.clone();
    loop {
        if current.generation == 1 {
            return if current.previous.is_none() {
                Ok(())
            } else {
                Err(error("legacy checkpoint predecessor mismatch"))
            };
        }
        let digest =
            current.previous.ok_or_else(|| error("legacy checkpoint predecessor is missing"))?;
        let prior: CheckpointManifest = decode(&read(digest)?)?;
        if prior.schema_version != LEGACY_CHECKPOINT_SCHEMA_VERSION {
            return Err(error("checkpoint schema downgrade"));
        }
        if prior.scope != current.scope
            || prior.generation.checked_add(1) != Some(current.generation)
        {
            return Err(error("legacy checkpoint predecessor mismatch"));
        }
        current = prior;
    }
}

pub(super) fn validate_checkpoint(
    schema_version: u16,
    state: &WorkingState,
    sources: &[ArchivedObservation],
    transcript: &TranscriptManifest,
    validation: &ViewValidation,
    limits: WorkingLimits,
) -> Result<(), DeveloperLoopError> {
    let archive_bytes = sources.iter().try_fold(0_u64, |total, source| {
        total
            .checked_add(source.artifact.bytes)
            .ok_or_else(|| error("checkpoint archive accounting overflow"))
    })?;
    let entries =
        state.entries(state.binding()).map_err(|_| error("checkpoint state binding mismatch"))?;
    let stale_entries =
        entries.iter().filter(|entry| entry.status() == WorkingEntryStatus::Stale).count();
    let selected_are_canonical =
        validation.selected_observations.windows(2).all(|pair| pair[0] < pair[1]);
    let selected_exist = validation
        .selected_observations
        .iter()
        .all(|sequence| *sequence > 0 && *sequence <= sources.len() as u64);
    if validation.state_revision != state.revision()
        || validation.through_observation != state.through_observation()
        || validation.estimated_input_tokens > validation.max_input_tokens
        || validation.max_input_tokens == 0
        || validation.input_tokens_saved
            != validation.uncompacted_input_tokens.saturating_sub(validation.estimated_input_tokens)
        || validation.archive_bytes != archive_bytes
        || validation.stale_entries != stale_entries
        || validation.omitted_entries > entries.len()
        || validation.pending_operations != transcript.pending.len()
        || !selected_are_canonical
        || !selected_exist
        || match schema_version {
            LEGACY_CHECKPOINT_SCHEMA_VERSION => validation.tool_policy.is_some(),
            CHECKPOINT_SCHEMA_VERSION => validation.tool_policy.is_none(),
            _ => true,
        }
    {
        return Err(error("checkpoint validation does not bind its exact state"));
    }
    validate_index(state, sources, transcript, limits)
}

pub(super) fn validate_index(
    state: &WorkingState,
    sources: &[ArchivedObservation],
    transcript: &TranscriptManifest,
    limits: WorkingLimits,
) -> Result<(), DeveloperLoopError> {
    if sources.len() as u64 != state.through_observation() || sources.len() > limits.observations()
    {
        return Err(error("source index size mismatch"));
    }
    for (index, source) in sources.iter().enumerate() {
        if source.sequence != index as u64 + 1
            || source.invocation == 0
            || source.invocation > transcript.invocation
        {
            return Err(error("invalid source index order"));
        }
        let locator = state
            .observation(
                state.binding(),
                ObservationId::new(source.sequence).map_err(|_| error("invalid source id"))?,
            )
            .map_err(|_| error("unresolved indexed observation"))?;
        source.validate_locator(locator)?;
    }
    let mut invocation = 0;
    let mut tool_sequence = 0_u64;
    for source in sources {
        if source.invocation < invocation {
            return Err(error("source invocation order regressed"));
        }
        if source.invocation != invocation {
            invocation = source.invocation;
            tool_sequence = 0;
        }
        if let Some(sequence) = source.tool_sequence {
            tool_sequence =
                tool_sequence.checked_add(1).ok_or_else(|| error("tool sequence overflow"))?;
            if sequence != tool_sequence {
                return Err(error("noncontiguous archived tool sequence"));
            }
        }
    }
    validate_transcript(state, sources, transcript, transcript.invocation, limits)
}

pub(super) fn validate_transcript(
    state: &WorkingState,
    sources: &[ArchivedObservation],
    transcript: &TranscriptManifest,
    expected_invocation: u64,
    limits: WorkingLimits,
) -> Result<(), DeveloperLoopError> {
    if transcript.invocation != expected_invocation
        || transcript.request_prefix.len() > 256
        || transcript.message_ids.len() > ProtocolLimits::PRODUCTION.max_messages()
        || transcript.current_inputs.len() > limits.entries()
        || transcript.pending.len() > limits.entries()
        || transcript.files.len() > limits.entries()
        || transcript.facts_through > state.through_observation()
    {
        return Err(error("transcript manifest bounds or identity mismatch"));
    }
    for ids in [&transcript.message_ids, &transcript.current_inputs] {
        if ids.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(error("noncanonical transcript source order"));
        }
        for id in ids {
            let source = archived(sources, *id)?;
            if source.invocation != transcript.invocation || source.kind == ArchiveKind::ToolOutput
            {
                return Err(error("invalid transcript message source"));
            }
        }
    }
    for id in &transcript.current_inputs {
        if !transcript.message_ids.contains(id)
            || !matches!(archived(sources, *id)?.kind, ArchiveKind::Policy | ArchiveKind::User)
        {
            return Err(error("invocation input is not a pinned instruction source"));
        }
    }
    if transcript.files.windows(2).any(|pair| pair[0] >= pair[1])
        || transcript.files.iter().any(|path| path.is_empty() || path.len() > 4096)
        || transcript.pending.windows(2).any(|pair| pair[0].key >= pair[1].key)
    {
        return Err(error("noncanonical pending or file projection"));
    }
    for pending in &transcript.pending {
        let source = archived(sources, pending.source)?;
        if pending.invocation != source.invocation {
            return Err(error("pending projection invocation mismatch"));
        }
        let expected_key = if let Some(handle) = &pending.handle {
            if source.kind != ArchiveKind::ToolOutput
                || source.call.as_ref() != Some(&pending.call)
                || handle.is_empty()
                || handle.len() > 256
            {
                return Err(error("pending operation source mismatch"));
            }
            environment::key(format!("operation:{handle}").as_bytes())?
        } else {
            if source.kind != ArchiveKind::Assistant {
                return Err(error("pending proposal source mismatch"));
            }
            environment::key(
                format!("proposal:{}/{}", pending.invocation, pending.call.id).as_bytes(),
            )?
        };
        if expected_key.into_bytes() != pending.key {
            return Err(error("pending identity does not bind its source"));
        }
    }
    Ok(())
}

fn archived(
    sources: &[ArchivedObservation],
    sequence: u64,
) -> Result<&ArchivedObservation, DeveloperLoopError> {
    let index = usize::try_from(
        sequence.checked_sub(1).ok_or_else(|| error("invalid observation handle"))?,
    )
    .map_err(|_| error("observation handle overflow"))?;
    sources
        .get(index)
        .filter(|source| source.sequence == sequence)
        .ok_or_else(|| error("observation is unavailable"))
}
