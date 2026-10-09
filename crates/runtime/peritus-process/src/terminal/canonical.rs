//! Versioned canonical encoding of complete durable terminal results.

use peritus_types::{ProcessId, Sha256Digest};
use sha2::{Digest, Sha256};

use crate::{
    ErrorCode, EscalationRecord, OsExitObservation, OutputArtifact, OutputSummary, ProcessError,
    ProcessInstant, ProcessOperation, ProcessResourceObservation, RecoveryClass, StopTrigger,
    StreamAccounting, TerminalResult,
};

mod reader;
mod tags;

use reader::Reader;
use tags::{
    completeness_tag, decode_completeness, decode_disposition, decode_fidelity, decode_reason,
    decode_recovery, decode_resource, decode_stream, disposition_tag, fidelity_tag, reason_tag,
    recovery_tag, resource_tag, stream_tag,
};

const MAGIC: &[u8] = b"PERITUS-PROCESS-TERMINAL-V2\0";
const MAX_ITEMS: usize = 64;
const MAX_SIGNAL_NAME_BYTES: usize = 128;

pub(crate) fn terminal_digest(result: &TerminalResult) -> Result<Sha256Digest, ProcessError> {
    let bytes = encode_terminal(result)?;
    Ok(Sha256Digest::new(Sha256::digest(bytes).into()))
}

pub(crate) fn encode_terminal(result: &TerminalResult) -> Result<Vec<u8>, ProcessError> {
    validate_terminal(result)?;
    let mut bytes = Vec::with_capacity(512);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(result.process_id().as_bytes());
    digest(&mut bytes, result.plan_digest());
    bytes.push(disposition_tag(result.disposition()));
    encode_exit(&mut bytes, result.os_exit())?;
    encode_trigger(&mut bytes, result.first_trigger());
    let escalation = result.escalation();
    boolean(&mut bytes, escalation.graceful_attempted());
    boolean(&mut bytes, escalation.forced());
    boolean(&mut bytes, escalation.tree_quiescent());
    optional_u64(&mut bytes, result.started_at().map(ProcessInstant::millis));
    u64_value(&mut bytes, result.ended_at().millis());
    encode_output(&mut bytes, result.output())?;
    encode_resources(&mut bytes, result.resources())?;
    encode_artifacts(&mut bytes, result.artifacts())?;
    boolean(&mut bytes, result.tree_cleanup_complete());
    boolean(&mut bytes, result.support_tasks_joined());
    boolean(&mut bytes, result.artifact_publication_complete());
    bytes.push(recovery_tag(result.recovery()));
    Ok(bytes)
}

pub(crate) fn decode_terminal(bytes: &[u8]) -> Result<TerminalResult, ProcessError> {
    if !bytes.starts_with(MAGIC) {
        return Err(corrupt("terminal result has invalid framing"));
    }
    let mut reader = Reader::new(&bytes[MAGIC.len()..]);
    let process_id = reader.id(ProcessId::new)?;
    let plan_digest = reader.digest()?;
    let disposition = decode_disposition(reader.u8()?)?;
    let os_exit = decode_exit(&mut reader)?;
    let first_trigger = decode_trigger(&mut reader)?;
    let escalation = EscalationRecord::new(reader.boolean()?, reader.boolean()?, reader.boolean()?);
    let started_at = reader.optional_u64()?.map(ProcessInstant::from_millis);
    let ended_at = ProcessInstant::from_millis(reader.u64()?);
    let output = decode_output(&mut reader)?;
    let resources = decode_resources(&mut reader)?;
    let artifacts = decode_artifacts(&mut reader)?;
    let tree_cleanup_complete = reader.boolean()?;
    let support_tasks_joined = reader.boolean()?;
    let publication_complete = reader.boolean()?;
    let recovery = decode_recovery(reader.u8()?)?;
    if !reader.is_empty() {
        return Err(corrupt("terminal result has trailing bytes"));
    }
    let mut result = TerminalResult::new(
        process_id,
        plan_digest,
        disposition,
        os_exit,
        first_trigger,
        escalation,
        started_at,
        ended_at,
        output,
        resources,
        tree_cleanup_complete,
        support_tasks_joined,
        recovery,
    );
    for artifact in artifacts {
        result.add_artifact(artifact);
    }
    if publication_complete {
        result.mark_artifacts_complete();
    } else {
        result.mark_artifact_failure();
    }
    validate_terminal(&result)?;
    if encode_terminal(&result)? != bytes {
        return Err(corrupt("terminal result uses a noncanonical field order"));
    }
    Ok(result)
}

fn validate_terminal(result: &TerminalResult) -> Result<(), ProcessError> {
    if result.started_at().is_some_and(|started| started.millis() > result.ended_at().millis())
        || result.first_trigger().is_some_and(|trigger| trigger.sequence() == 0)
        || result.escalation().tree_quiescent() != result.tree_cleanup_complete()
    {
        return Err(corrupt("terminal lifecycle observations are inconsistent"));
    }
    validate_output(result.output())?;
    validate_resources(result.resources())?;
    validate_artifacts(result.output(), result.artifacts(), result.artifact_publication_complete())
}

fn validate_output(output: &OutputSummary) -> Result<(), ProcessError> {
    if output.streams().len() > MAX_ITEMS {
        return Err(corrupt("terminal output stream count exceeds its bound"));
    }
    let mut seen = [false; 3];
    for stream in output.streams() {
        let index = usize::from(stream_tag(stream.stream()) - 1);
        if seen[index]
            || stream.retained() > stream.observed()
            || stream.dropped() != stream.observed() - stream.retained()
        {
            return Err(corrupt("terminal output accounting is noncanonical"));
        }
        seen[index] = true;
    }
    Ok(())
}

fn validate_resources(resources: &[ProcessResourceObservation]) -> Result<(), ProcessError> {
    if resources.len() > MAX_ITEMS {
        return Err(corrupt("terminal resource observation count exceeds its bound"));
    }
    for (index, resource) in resources.iter().enumerate() {
        if resources[..index].iter().any(|other| other.dimension() == resource.dimension()) {
            return Err(corrupt("terminal resource observations contain a duplicate dimension"));
        }
    }
    Ok(())
}

fn validate_artifacts(
    output: &OutputSummary,
    artifacts: &[OutputArtifact],
    publication_complete: bool,
) -> Result<(), ProcessError> {
    if artifacts.len() > MAX_ITEMS {
        return Err(corrupt("terminal artifact count exceeds its bound"));
    }
    for (index, artifact) in artifacts.iter().enumerate() {
        let Some(accounting) =
            output.streams().iter().find(|item| item.stream() == artifact.stream())
        else {
            return Err(corrupt("terminal artifact has no corresponding output stream"));
        };
        let range_matches = artifact.end_offset() == artifact.size();
        let retained_matches = artifact.size() == accounting.retained();
        if artifacts[..index].iter().any(|other| other.stream() == artifact.stream())
            || artifact.start_offset() != 0
            || !range_matches
            || !retained_matches
            || artifact.completeness() != accounting.completeness()
        {
            return Err(corrupt("terminal artifact differs from retained output accounting"));
        }
    }
    if publication_complete
        && output
            .streams()
            .iter()
            .filter(|stream| stream.retained() > 0)
            .any(|stream| !artifacts.iter().any(|artifact| artifact.stream() == stream.stream()))
    {
        return Err(corrupt("terminal publication is complete without every retained stream"));
    }
    Ok(())
}

fn encode_output(bytes: &mut Vec<u8>, output: &OutputSummary) -> Result<(), ProcessError> {
    count(bytes, output.streams().len())?;
    let mut streams = output.streams().to_vec();
    streams.sort_by_key(|stream| stream_tag(stream.stream()));
    for stream in streams {
        bytes.push(stream_tag(stream.stream()));
        u64_value(bytes, stream.observed());
        u64_value(bytes, stream.retained());
        u64_value(bytes, stream.dropped());
        bytes.push(completeness_tag(stream.completeness()));
    }
    u64_value(bytes, output.event_records_dropped());
    Ok(())
}

fn decode_output(reader: &mut Reader<'_>) -> Result<OutputSummary, ProcessError> {
    let count = reader.count(MAX_ITEMS)?;
    let mut streams = Vec::with_capacity(count);
    for _ in 0..count {
        streams.push(
            StreamAccounting::from_persisted(
                decode_stream(reader.u8()?)?,
                reader.u64()?,
                reader.u64()?,
                reader.u64()?,
                decode_completeness(reader.u8()?)?,
            )
            .ok_or_else(|| corrupt("terminal output accounting is inconsistent"))?,
        );
    }
    Ok(OutputSummary::new(streams, reader.u64()?))
}

fn encode_resources(
    bytes: &mut Vec<u8>,
    resources: &[ProcessResourceObservation],
) -> Result<(), ProcessError> {
    count(bytes, resources.len())?;
    let mut ordered = resources.to_vec();
    ordered.sort_by_key(|resource| resource_tag(resource.dimension()));
    for resource in ordered {
        bytes.push(resource_tag(resource.dimension()));
        u64_value(bytes, resource.value());
        u64_value(bytes, resource.ceiling());
        bytes.push(fidelity_tag(resource.fidelity()));
    }
    Ok(())
}

fn decode_resources(
    reader: &mut Reader<'_>,
) -> Result<Vec<ProcessResourceObservation>, ProcessError> {
    let count = reader.count(MAX_ITEMS)?;
    let mut resources = Vec::with_capacity(count);
    for _ in 0..count {
        resources.push(ProcessResourceObservation::new(
            decode_resource(reader.u8()?)?,
            reader.u64()?,
            reader.u64()?,
            decode_fidelity(reader.u8()?)?,
        ));
    }
    Ok(resources)
}

fn encode_artifacts(bytes: &mut Vec<u8>, artifacts: &[OutputArtifact]) -> Result<(), ProcessError> {
    count(bytes, artifacts.len())?;
    let mut ordered = artifacts.to_vec();
    ordered.sort_by_key(|artifact| stream_tag(artifact.stream()));
    for artifact in ordered {
        bytes.push(stream_tag(artifact.stream()));
        digest(bytes, artifact.digest());
        u64_value(bytes, artifact.size());
        u64_value(bytes, artifact.start_offset());
        u64_value(bytes, artifact.end_offset());
        bytes.push(completeness_tag(artifact.completeness()));
    }
    Ok(())
}

fn decode_artifacts(reader: &mut Reader<'_>) -> Result<Vec<OutputArtifact>, ProcessError> {
    let count = reader.count(MAX_ITEMS)?;
    let mut artifacts = Vec::with_capacity(count);
    for _ in 0..count {
        artifacts.push(OutputArtifact::new(
            decode_stream(reader.u8()?)?,
            reader.digest()?,
            reader.u64()?,
            reader.u64()?,
            reader.u64()?,
            decode_completeness(reader.u8()?)?,
        ));
    }
    Ok(artifacts)
}

fn encode_exit(bytes: &mut Vec<u8>, exit: &OsExitObservation) -> Result<(), ProcessError> {
    match exit {
        OsExitObservation::Code(value) => {
            bytes.push(1);
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        OsExitObservation::Signal(value) => {
            bytes.push(2);
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        OsExitObservation::SignalName(value) => {
            if value.len() > MAX_SIGNAL_NAME_BYTES {
                return Err(corrupt("terminal signal name exceeds its bound"));
            }
            bytes.push(3);
            length(bytes, value.len(), MAX_SIGNAL_NAME_BYTES)?;
            bytes.extend_from_slice(value.as_bytes());
        }
        OsExitObservation::PlatformException(value) => {
            bytes.push(4);
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        OsExitObservation::Unavailable => bytes.push(5),
    }
    Ok(())
}

fn decode_exit(reader: &mut Reader<'_>) -> Result<OsExitObservation, ProcessError> {
    match reader.u8()? {
        1 => Ok(OsExitObservation::Code(reader.i32()?)),
        2 => Ok(OsExitObservation::Signal(reader.i32()?)),
        3 => Ok(OsExitObservation::SignalName(reader.string(MAX_SIGNAL_NAME_BYTES)?)),
        4 => Ok(OsExitObservation::PlatformException(reader.u32()?)),
        5 => Ok(OsExitObservation::Unavailable),
        _ => Err(corrupt("terminal result has an unknown exit tag")),
    }
}

fn encode_trigger(bytes: &mut Vec<u8>, trigger: Option<StopTrigger>) {
    match trigger {
        Some(trigger) => {
            bytes.push(1);
            u64_value(bytes, trigger.sequence());
            bytes.push(reason_tag(trigger.reason()));
        }
        None => bytes.push(0),
    }
}

fn decode_trigger(reader: &mut Reader<'_>) -> Result<Option<StopTrigger>, ProcessError> {
    match reader.u8()? {
        0 => Ok(None),
        1 => {
            let sequence = reader.u64()?;
            if sequence == 0 {
                return Err(corrupt("terminal result contains a zero trigger sequence"));
            }
            Ok(Some(StopTrigger::new(sequence, decode_reason(reader.u8()?)?)))
        }
        _ => Err(corrupt("terminal result has an invalid trigger tag")),
    }
}

fn count(bytes: &mut Vec<u8>, value: usize) -> Result<(), ProcessError> {
    length(bytes, value, MAX_ITEMS)
}

fn length(bytes: &mut Vec<u8>, value: usize, limit: usize) -> Result<(), ProcessError> {
    if value > limit {
        return Err(corrupt("terminal collection exceeds its canonical bound"));
    }
    let value = u16::try_from(value)
        .map_err(|_| corrupt("terminal collection length is not representable"))?;
    bytes.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

fn boolean(bytes: &mut Vec<u8>, value: bool) {
    bytes.push(u8::from(value));
}

fn optional_u64(bytes: &mut Vec<u8>, value: Option<u64>) {
    match value {
        Some(value) => {
            bytes.push(1);
            u64_value(bytes, value);
        }
        None => bytes.push(0),
    }
}

fn digest(bytes: &mut Vec<u8>, value: Sha256Digest) {
    bytes.extend_from_slice(value.as_bytes());
}

fn u64_value(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

pub(super) const fn corrupt(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::CorruptRecovery,
        ProcessOperation::Reconcile,
        RecoveryClass::Quarantine,
        detail,
    )
}
