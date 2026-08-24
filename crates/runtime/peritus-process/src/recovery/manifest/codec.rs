//! Versioned checksum-protected process-manifest codec.
use peritus_types::{
    AcceptanceSpecId, ActionId, ActorId, AttemptId, EnvironmentId, Generation, HarnessId, PolicyId,
    ProcessId, ProjectId, ProviderProfileId, ResourceId, RevisionNumber, RevisionTuple, RunId,
    SessionId, Sha256Digest, TurnId, WorkspaceId,
};
use sha2::{Digest, Sha256};

use crate::{
    CancellationReason, ErrorCode, ExecutionIdentity, LifecyclePhase, OsExitObservation,
    ProcessError, ProcessOperation, RecoveryClass, StopTrigger, WorkspaceAccess,
    platform::ProcessTreeIdentity,
};

use super::{ExecutionManifest, LeaseOwnership};

mod reader;
mod terminal_payload;
use reader::Reader;
use terminal_payload::{decode_terminal_payload, encode_terminal_payload, terminal_binding_valid};

const MAGIC: &[u8] = b"PERITUS-PROCESS-MANIFEST-V2\0";
const MAX_MANIFEST_BYTES: usize = 16 * 1_024;

pub(super) fn encode(manifest: &ExecutionManifest) -> Result<Vec<u8>, ProcessError> {
    let mut bytes = Vec::with_capacity(768);
    bytes.extend_from_slice(MAGIC);
    encode_identity(&mut bytes, &manifest.identity);
    for digest_value in [
        manifest.action_digest,
        manifest.plan_digest,
        manifest.sandbox_digest,
        manifest.backend_digest,
        manifest.support_digest,
        manifest.preparation_digest,
    ] {
        digest(&mut bytes, digest_value);
    }
    bytes.push(match manifest.access {
        WorkspaceAccess::ReadOnly => 1,
        WorkspaceAccess::Writable => 2,
    });
    encode_lease(&mut bytes, manifest.lease);
    bytes.push(phase_tag(manifest.phase));
    encode_tree(&mut bytes, manifest.tree);
    encode_trigger(&mut bytes, manifest.trigger);
    encode_exit(&mut bytes, manifest.exit.as_ref())?;
    u64_value(&mut bytes, manifest.observed_output);
    u64_value(&mut bytes, manifest.retained_output);
    u64_value(&mut bytes, manifest.dropped_output);
    bytes.push(u8::from(manifest.tree_quiescent));
    bytes.push(u8::from(manifest.support_tasks_joined));
    optional_digest(&mut bytes, manifest.terminal_digest);
    encode_terminal_payload(&mut bytes, manifest.terminal.as_ref())?;
    if bytes.len() + Sha256Digest::LENGTH > MAX_MANIFEST_BYTES {
        return Err(corrupt("process manifest exceeds its canonical bound"));
    }
    let checksum: [u8; 32] = Sha256::digest(&bytes).into();
    bytes.extend_from_slice(&checksum);
    Ok(bytes)
}

pub(super) fn decode(bytes: &[u8]) -> Result<ExecutionManifest, ProcessError> {
    if bytes.len() < MAGIC.len() + Sha256Digest::LENGTH
        || bytes.len() > MAX_MANIFEST_BYTES
        || !bytes.starts_with(MAGIC)
    {
        return Err(corrupt("process manifest has invalid framing"));
    }
    let payload_end = bytes.len() - Sha256Digest::LENGTH;
    let expected: [u8; 32] = Sha256::digest(&bytes[..payload_end]).into();
    if bytes[payload_end..] != expected {
        return Err(corrupt("process manifest checksum differs"));
    }
    let mut reader = Reader::new(&bytes[MAGIC.len()..payload_end]);
    let manifest = ExecutionManifest {
        identity: decode_identity(&mut reader)?,
        action_digest: reader.digest()?,
        plan_digest: reader.digest()?,
        sandbox_digest: reader.digest()?,
        backend_digest: reader.digest()?,
        support_digest: reader.digest()?,
        preparation_digest: reader.digest()?,
        access: decode_access(reader.u8()?)?,
        lease: decode_lease(&mut reader)?,
        phase: decode_phase(reader.u8()?)?,
        tree: decode_tree(&mut reader)?,
        trigger: decode_trigger(&mut reader)?,
        exit: decode_exit(&mut reader)?,
        observed_output: reader.u64()?,
        retained_output: reader.u64()?,
        dropped_output: reader.u64()?,
        tree_quiescent: reader.boolean()?,
        support_tasks_joined: reader.boolean()?,
        terminal_digest: reader.optional_digest()?,
        terminal: decode_terminal_payload(&mut reader)?,
    };
    if !reader.is_empty()
        || manifest.retained_output > manifest.observed_output
        || manifest.dropped_output != manifest.observed_output - manifest.retained_output
        || !terminal_binding_valid(&manifest)?
    {
        return Err(corrupt("process manifest fields are noncanonical or inconsistent"));
    }
    Ok(manifest)
}

const fn decode_access(tag: u8) -> Result<WorkspaceAccess, ProcessError> {
    match tag {
        1 => Ok(WorkspaceAccess::ReadOnly),
        2 => Ok(WorkspaceAccess::Writable),
        _ => Err(corrupt("process manifest has an unknown access tag")),
    }
}

fn encode_identity(bytes: &mut Vec<u8>, identity: &ExecutionIdentity) {
    bytes.extend_from_slice(identity.project_id().as_bytes());
    bytes.extend_from_slice(identity.session_id().as_bytes());
    bytes.extend_from_slice(identity.run_id().as_bytes());
    bytes.extend_from_slice(identity.attempt_id().as_bytes());
    bytes.extend_from_slice(identity.turn_id().as_bytes());
    bytes.extend_from_slice(identity.action_id().as_bytes());
    bytes.extend_from_slice(identity.process_id().as_bytes());
    bytes.extend_from_slice(identity.workspace_id().as_bytes());
    bytes.extend_from_slice(identity.resource_id().as_bytes());
    bytes.extend_from_slice(identity.environment_id().as_bytes());
    bytes.extend_from_slice(identity.actor_id().as_bytes());
    encode_revision(bytes, identity.revision());
}

fn decode_identity(reader: &mut Reader<'_>) -> Result<ExecutionIdentity, ProcessError> {
    Ok(ExecutionIdentity::new(
        reader.id(ProjectId::new)?,
        reader.id(SessionId::new)?,
        reader.id(RunId::new)?,
        reader.id(AttemptId::new)?,
        reader.id(TurnId::new)?,
        reader.id(ActionId::new)?,
        reader.id(ProcessId::new)?,
        reader.id(WorkspaceId::new)?,
        reader.id(ResourceId::new)?,
        reader.id(EnvironmentId::new)?,
        reader.id(ActorId::new)?,
        decode_revision(reader)?,
    ))
}

fn encode_revision(bytes: &mut Vec<u8>, revision: RevisionTuple) {
    bytes.extend_from_slice(revision.acceptance_spec_id().as_bytes());
    bytes.extend_from_slice(revision.harness_id().as_bytes());
    bytes.extend_from_slice(revision.workspace_id().as_bytes());
    u64_value(bytes, revision.workspace_generation().get());
    u64_value(bytes, revision.workspace_revision().get());
    bytes.extend_from_slice(revision.policy_id().as_bytes());
    bytes.extend_from_slice(revision.provider_profile_id().as_bytes());
}

fn decode_revision(reader: &mut Reader<'_>) -> Result<RevisionTuple, ProcessError> {
    let acceptance = reader.id(AcceptanceSpecId::new)?;
    let harness = reader.id(HarnessId::new)?;
    let workspace = reader.id(WorkspaceId::new)?;
    let generation = Generation::new(reader.u64()?)
        .map_err(|_| corrupt("manifest contains a zero workspace generation"))?;
    let revision = RevisionNumber::new(reader.u64()?)
        .map_err(|_| corrupt("manifest contains a zero workspace revision"))?;
    let policy = reader.id(PolicyId::new)?;
    let provider = reader.id(ProviderProfileId::new)?;
    Ok(RevisionTuple::new(acceptance, harness, workspace, generation, revision, policy, provider))
}

fn encode_lease(bytes: &mut Vec<u8>, lease: Option<LeaseOwnership>) {
    let Some(lease) = lease else {
        bytes.push(0);
        return;
    };
    bytes.push(1);
    bytes.extend_from_slice(lease.workspace_id.as_bytes());
    bytes.extend_from_slice(lease.resource_id.as_bytes());
    bytes.extend_from_slice(lease.environment_id.as_bytes());
    bytes.extend_from_slice(lease.actor_id.as_bytes());
    bytes.extend_from_slice(lease.session_id.as_bytes());
    u64_value(bytes, lease.generation.get());
    u64_value(bytes, lease.claim_version.get());
}

fn decode_lease(reader: &mut Reader<'_>) -> Result<Option<LeaseOwnership>, ProcessError> {
    match reader.u8()? {
        0 => Ok(None),
        1 => Ok(Some(LeaseOwnership {
            workspace_id: reader.id(WorkspaceId::new)?,
            resource_id: reader.id(ResourceId::new)?,
            environment_id: reader.id(EnvironmentId::new)?,
            actor_id: reader.id(ActorId::new)?,
            session_id: reader.id(SessionId::new)?,
            generation: Generation::new(reader.u64()?)
                .map_err(|_| corrupt("manifest contains a zero lease generation"))?,
            claim_version: RevisionNumber::new(reader.u64()?)
                .map_err(|_| corrupt("manifest contains a zero lease version"))?,
        })),
        _ => Err(corrupt("process manifest has an invalid optional lease tag")),
    }
}

fn encode_tree(bytes: &mut Vec<u8>, tree: Option<ProcessTreeIdentity>) {
    let Some(tree) = tree else {
        bytes.push(0);
        return;
    };
    bytes.push(1);
    u32_value(bytes, tree.root_pid());
    optional_u64(bytes, tree.start_token());
    match tree.process_group() {
        Some(group) => {
            bytes.push(1);
            u32_value(bytes, group);
        }
        None => bytes.push(0),
    }
    bytes.push(u8::from(tree.complete_containment()));
}

fn decode_tree(reader: &mut Reader<'_>) -> Result<Option<ProcessTreeIdentity>, ProcessError> {
    match reader.u8()? {
        0 => Ok(None),
        1 => {
            let root = reader.u32()?;
            if root == 0 {
                return Err(corrupt("manifest contains a zero process identifier"));
            }
            let start = reader.optional_u64()?;
            let group = match reader.u8()? {
                0 => None,
                1 => Some(reader.u32()?),
                _ => return Err(corrupt("manifest has an invalid process group tag")),
            };
            Ok(Some(ProcessTreeIdentity::new(root, start, group, reader.boolean()?)))
        }
        _ => Err(corrupt("manifest has an invalid optional process tree tag")),
    }
}

fn encode_trigger(bytes: &mut Vec<u8>, trigger: Option<StopTrigger>) {
    let Some(trigger) = trigger else {
        bytes.push(0);
        return;
    };
    bytes.push(1);
    u64_value(bytes, trigger.sequence());
    bytes.push(reason_tag(trigger.reason()));
}

fn decode_trigger(reader: &mut Reader<'_>) -> Result<Option<StopTrigger>, ProcessError> {
    match reader.u8()? {
        0 => Ok(None),
        1 => {
            let sequence = reader.u64()?;
            if sequence == 0 {
                return Err(corrupt("manifest contains a zero trigger sequence"));
            }
            Ok(Some(StopTrigger::new(sequence, decode_reason(reader.u8()?)?)))
        }
        _ => Err(corrupt("manifest has an invalid optional trigger tag")),
    }
}

fn encode_exit(bytes: &mut Vec<u8>, exit: Option<&OsExitObservation>) -> Result<(), ProcessError> {
    match exit {
        None => bytes.push(0),
        Some(OsExitObservation::Code(code)) => {
            bytes.push(1);
            i32_value(bytes, *code);
        }
        Some(OsExitObservation::Signal(signal)) => {
            bytes.push(2);
            i32_value(bytes, *signal);
        }
        Some(OsExitObservation::SignalName(name)) => {
            if name.len() > 128 {
                return Err(corrupt("platform signal name exceeds its bound"));
            }
            bytes.push(3);
            u16_value(
                bytes,
                u16::try_from(name.len()).map_err(|_| corrupt("signal name is too long"))?,
            );
            bytes.extend_from_slice(name.as_bytes());
        }
        Some(OsExitObservation::PlatformException(code)) => {
            bytes.push(4);
            u32_value(bytes, *code);
        }
        Some(OsExitObservation::Unavailable) => bytes.push(5),
    }
    Ok(())
}

fn decode_exit(reader: &mut Reader<'_>) -> Result<Option<OsExitObservation>, ProcessError> {
    Ok(match reader.u8()? {
        0 => None,
        1 => Some(OsExitObservation::Code(reader.i32()?)),
        2 => Some(OsExitObservation::Signal(reader.i32()?)),
        3 => Some(OsExitObservation::SignalName(reader.string(128)?)),
        4 => Some(OsExitObservation::PlatformException(reader.u32()?)),
        5 => Some(OsExitObservation::Unavailable),
        _ => return Err(corrupt("manifest has an unknown exit observation tag")),
    })
}

const fn phase_tag(phase: LifecyclePhase) -> u8 {
    match phase {
        LifecyclePhase::Authorized => 1,
        LifecyclePhase::Starting => 2,
        LifecyclePhase::Running => 3,
        LifecyclePhase::Stopping => 4,
        LifecyclePhase::Exited => 5,
        LifecyclePhase::Closed => 6,
        LifecyclePhase::Terminal => 7,
    }
}

const fn decode_phase(tag: u8) -> Result<LifecyclePhase, ProcessError> {
    match tag {
        1 => Ok(LifecyclePhase::Authorized),
        2 => Ok(LifecyclePhase::Starting),
        3 => Ok(LifecyclePhase::Running),
        4 => Ok(LifecyclePhase::Stopping),
        5 => Ok(LifecyclePhase::Exited),
        6 => Ok(LifecyclePhase::Closed),
        7 => Ok(LifecyclePhase::Terminal),
        _ => Err(corrupt("manifest has an unknown lifecycle phase")),
    }
}

const fn reason_tag(reason: CancellationReason) -> u8 {
    match reason {
        CancellationReason::User => 1,
        CancellationReason::Deadline => 2,
        CancellationReason::OutputLimit => 3,
        CancellationReason::ResourceLimit => 4,
        CancellationReason::LeaseFence => 5,
        CancellationReason::SupervisorShutdown => 6,
        CancellationReason::BackendFailure => 7,
    }
}

const fn decode_reason(tag: u8) -> Result<CancellationReason, ProcessError> {
    match tag {
        1 => Ok(CancellationReason::User),
        2 => Ok(CancellationReason::Deadline),
        3 => Ok(CancellationReason::OutputLimit),
        4 => Ok(CancellationReason::ResourceLimit),
        5 => Ok(CancellationReason::LeaseFence),
        6 => Ok(CancellationReason::SupervisorShutdown),
        7 => Ok(CancellationReason::BackendFailure),
        _ => Err(corrupt("manifest has an unknown cancellation reason")),
    }
}

fn digest(bytes: &mut Vec<u8>, value: Sha256Digest) {
    bytes.extend_from_slice(value.as_bytes());
}
fn optional_digest(bytes: &mut Vec<u8>, value: Option<Sha256Digest>) {
    match value {
        Some(value) => {
            bytes.push(1);
            digest(bytes, value);
        }
        None => bytes.push(0),
    }
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
fn u16_value(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}
fn u32_value(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}
fn i32_value(bytes: &mut Vec<u8>, value: i32) {
    bytes.extend_from_slice(&value.to_be_bytes());
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
