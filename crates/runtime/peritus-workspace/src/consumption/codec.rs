//! Canonical encoding and decoding for durable action marker frames.

use std::{fs, path::Path};

use peritus_artifact_store::ArtifactDigest;
use peritus_git::CommitId;
use peritus_types::{ActionId, EventId, RevisionNumber, Sha256Digest, SnapshotId};

use super::{
    ActionConsumptionBinding, ActionPlan, ActionRecord, ActionTerminalRecord, consumption_error,
};
use crate::WorkspaceError;

const MAGIC_V1: &[u8] = b"PERITUS-WORKSPACE-ACTION-V1\0";
const MAGIC_V2: &[u8] = b"PERITUS-WORKSPACE-ACTION-V2\0";
const PLAN_MAGIC: &[u8] = b"PLAN1\0";
const HEADER_BYTES_V1: usize = MAGIC_V1.len() + 16 + 16 + 16 + 8 + 8 + 16 + 32;
const HEADER_BYTES_V2: usize = MAGIC_V2.len() + 16 + 16 + 16 + 8 + 8 + 16 + 32;

pub(super) fn encode_header(
    binding: ActionConsumptionBinding,
    action_id: ActionId,
    action_digest: Sha256Digest,
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(HEADER_BYTES_V2);
    bytes.extend_from_slice(MAGIC_V2);
    bytes.extend_from_slice(binding.workspace_id.as_bytes());
    bytes.extend_from_slice(binding.resource_id.as_bytes());
    bytes.extend_from_slice(binding.environment_id.as_bytes());
    bytes.extend_from_slice(&binding.generation.get().to_be_bytes());
    bytes.extend_from_slice(&binding.revision.get().to_be_bytes());
    bytes.extend_from_slice(action_id.as_bytes());
    bytes.extend_from_slice(action_digest.as_bytes());
    bytes
}

pub(super) fn encode_plan(plan: &ActionPlan) -> Vec<u8> {
    let mut payload = Vec::with_capacity(153);
    payload.push(plan.operation);
    payload.extend_from_slice(plan.snapshot_id.as_bytes());
    payload.extend_from_slice(plan.payload_digest.as_bytes());
    payload.extend_from_slice(&plan.installed_revision.get().to_be_bytes());
    payload.extend_from_slice(plan.dispatch_event.as_bytes());
    payload.extend_from_slice(
        plan.patch_identity.map_or([0; 32], |identity| *identity.as_bytes()).as_slice(),
    );
    payload.extend_from_slice(
        plan.patch_manifest_digest.map_or([0; 32], |digest| *digest.as_bytes()).as_slice(),
    );
    payload.extend_from_slice(
        plan.target_snapshot_id.map_or([0; 16], SnapshotId::into_bytes).as_slice(),
    );
    let mut frame = Vec::with_capacity(PLAN_MAGIC.len() + 8 + payload.len() + 32);
    frame.extend_from_slice(PLAN_MAGIC);
    frame.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    frame.extend_from_slice(&payload);
    frame.extend_from_slice(peritus_codec::sha256(&payload).as_bytes());
    frame
}

pub(super) fn decode_plan(bytes: &[u8]) -> Result<Option<(ActionPlan, usize)>, WorkspaceError> {
    if !bytes.starts_with(PLAN_MAGIC) {
        return Ok(None);
    }
    let prefix = PLAN_MAGIC.len();
    if bytes.len() < prefix + 8 {
        return Ok(None);
    }
    let length = u64::from_le_bytes(
        bytes[prefix..prefix + 8]
            .try_into()
            .map_err(|_| consumption_error("action plan length is malformed"))?,
    );
    if length != 153 {
        return Err(consumption_error("action plan has an invalid length"));
    }
    let payload_start = prefix + 8;
    let payload_end = payload_start + 153;
    let frame_end = payload_end + 32;
    if bytes.len() < frame_end {
        return Ok(None);
    }
    let payload = &bytes[payload_start..payload_end];
    if peritus_codec::sha256(payload).as_bytes() != &bytes[payload_end..frame_end] {
        return Err(consumption_error("action plan checksum does not match"));
    }
    let snapshot_id = SnapshotId::new(take_array::<16>(payload, &mut 1_usize))
        .map_err(|_| consumption_error("action plan snapshot identity is invalid"))?;
    let mut digest_bytes = [0_u8; 32];
    digest_bytes.copy_from_slice(&payload[17..49]);
    let revision = u64::from_be_bytes(
        payload[49..57]
            .try_into()
            .map_err(|_| consumption_error("action plan revision is malformed"))?,
    );
    let installed_revision = RevisionNumber::new(revision)
        .map_err(|_| consumption_error("action plan revision is invalid"))?;
    let dispatch_event = EventId::new(
        payload[57..73]
            .try_into()
            .map_err(|_| consumption_error("action plan event is malformed"))?,
    )
    .map_err(|_| consumption_error("action plan event is invalid"))?;
    let patch_bytes: [u8; 32] = payload[73..105]
        .try_into()
        .map_err(|_| consumption_error("action plan patch identity is malformed"))?;
    let patch_digest_bytes: [u8; 32] = payload[105..137]
        .try_into()
        .map_err(|_| consumption_error("action plan patch digest is malformed"))?;
    let target_bytes: [u8; 16] = payload[137..153]
        .try_into()
        .map_err(|_| consumption_error("action plan target is malformed"))?;
    let patch_identity = (patch_bytes != [0; 32])
        .then(|| peritus_patch::PatchIdentity::from_digest(Sha256Digest::new(patch_bytes)));
    let patch_manifest_digest =
        (patch_digest_bytes != [0; 32]).then(|| Sha256Digest::new(patch_digest_bytes));
    let target_snapshot_id = (target_bytes != [0; 16])
        .then(|| SnapshotId::new(target_bytes))
        .transpose()
        .map_err(|_| consumption_error("action plan target snapshot is invalid"))?;
    if (payload[0] == 3
        && (patch_identity.is_none()
            || patch_manifest_digest.is_none()
            || target_snapshot_id.is_some()))
        || (payload[0] == 4
            && (patch_identity.is_some()
                || patch_manifest_digest.is_some()
                || target_snapshot_id.is_none()))
        || !matches!(payload[0], 3 | 4)
    {
        return Err(consumption_error("action plan fields do not match its operation"));
    }
    Ok(Some((
        ActionPlan {
            operation: payload[0],
            snapshot_id,
            payload_digest: Sha256Digest::new(digest_bytes),
            installed_revision,
            dispatch_event,
            patch_identity,
            patch_manifest_digest,
            target_snapshot_id,
        },
        frame_end,
    )))
}

pub(super) fn decode_record(
    binding: ActionConsumptionBinding,
    bytes: &[u8],
) -> Result<(ActionId, ActionRecord, u64), WorkspaceError> {
    let (magic, legacy, header_bytes) = if bytes.starts_with(MAGIC_V2) {
        (MAGIC_V2, false, HEADER_BYTES_V2)
    } else if bytes.starts_with(MAGIC_V1) {
        (MAGIC_V1, true, HEADER_BYTES_V1)
    } else {
        return Err(consumption_error("action marker has an unsupported format"));
    };
    if bytes.len() < header_bytes {
        return Err(consumption_error("action marker header is incomplete"));
    }
    let mut offset = magic.len();
    let workspace = take_array::<16>(bytes, &mut offset);
    let resource = take_array::<16>(bytes, &mut offset);
    let environment = take_array::<16>(bytes, &mut offset);
    let generation = u64::from_be_bytes(take_array::<8>(bytes, &mut offset));
    let revision = u64::from_be_bytes(take_array::<8>(bytes, &mut offset));
    let action = take_array::<16>(bytes, &mut offset);
    let digest = take_array::<32>(bytes, &mut offset);
    if workspace != binding.workspace_id.into_bytes()
        || resource != binding.resource_id.into_bytes()
        || environment != binding.environment_id.into_bytes()
        || generation != binding.generation.get()
        || revision != binding.revision.get()
    {
        return Err(consumption_error("action marker differs from current workspace state"));
    }
    let action_id = ActionId::new(action)
        .map_err(|_| consumption_error("action marker contains an invalid action identity"))?;
    let mut record = ActionRecord {
        action_digest: Sha256Digest::new(digest),
        plan: None,
        terminal: None,
        legacy,
    };
    let mut consumed = header_bytes;
    if let Some((plan, frame_bytes)) = decode_plan(&bytes[header_bytes..])? {
        record.plan = Some(plan);
        consumed += frame_bytes;
    }
    if record.plan.is_some() && bytes[consumed..].starts_with(PLAN_MAGIC) {
        return Err(consumption_error("action marker contains multiple plans"));
    }
    if !bytes[consumed..].starts_with(PLAN_MAGIC)
        && let Some((terminal, frame_bytes)) = decode_terminal(&bytes[consumed..])?
    {
        record.terminal = Some(terminal);
        consumed = consumed
            .checked_add(frame_bytes)
            .ok_or_else(|| consumption_error("action marker length overflowed"))?;
        if decode_plan(&bytes[consumed..])?.is_some()
            || decode_terminal(&bytes[consumed..])?.is_some()
        {
            return Err(consumption_error("action marker contains multiple terminal results"));
        }
    }
    let consumed = u64::try_from(consumed)
        .map_err(|_| consumption_error("action marker exceeds this platform"))?;
    Ok((action_id, record, consumed))
}

pub(super) fn read_action_bytes(path: &Path) -> Result<Vec<u8>, WorkspaceError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| consumption_error("action marker cannot be inspected"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(consumption_error("action marker is not a regular file"));
    }
    fs::read(path).map_err(|_| consumption_error("action marker cannot be read"))
}

pub(super) fn encode_terminal(terminal: &ActionTerminalRecord) -> Result<Vec<u8>, WorkspaceError> {
    let mut payload = Vec::new();
    match terminal {
        ActionTerminalRecord::Applied { patch_identity, installed_manifest } => {
            payload.push(1);
            payload.extend_from_slice(patch_identity.as_bytes());
            let digest = peritus_codec::sha256(installed_manifest);
            payload.extend_from_slice(digest.as_bytes());
            let length = u64::try_from(installed_manifest.len())
                .map_err(|_| consumption_error("installed patch manifest exceeds this platform"))?;
            payload.extend_from_slice(&length.to_le_bytes());
            payload.extend_from_slice(installed_manifest);
        }
        ActionTerminalRecord::RolledBack => payload.push(2),
        ActionTerminalRecord::Candidate {
            patch_identity,
            detail_digest,
            artifact_digest,
            artifact_size,
            snapshot_manifest,
            workspace_manifest,
        } => {
            payload.push(3);
            payload.extend_from_slice(patch_identity.as_bytes());
            payload.extend_from_slice(detail_digest.as_bytes());
            payload.extend_from_slice(artifact_digest.sha256().as_bytes());
            payload.extend_from_slice(&artifact_size.to_le_bytes());
            put_bytes(&mut payload, snapshot_manifest)?;
            put_bytes(&mut payload, workspace_manifest)?;
        }
        ActionTerminalRecord::WorkspaceRollback {
            restored_from,
            detail_digest,
            artifact_digest,
            artifact_size,
            snapshot_manifest,
            workspace_manifest,
        } => {
            payload.push(4);
            put_bytes(&mut payload, restored_from.to_string().as_bytes())?;
            payload.extend_from_slice(detail_digest.as_bytes());
            payload.extend_from_slice(artifact_digest.sha256().as_bytes());
            payload.extend_from_slice(&artifact_size.to_le_bytes());
            put_bytes(&mut payload, snapshot_manifest)?;
            put_bytes(&mut payload, workspace_manifest)?;
        }
    }
    let length = u64::try_from(payload.len())
        .map_err(|_| consumption_error("action result exceeds this platform"))?;
    let mut frame = Vec::with_capacity(payload.len().saturating_add(40));
    frame.extend_from_slice(&length.to_le_bytes());
    frame.extend_from_slice(&payload);
    frame.extend_from_slice(peritus_codec::sha256(&payload).as_bytes());
    Ok(frame)
}

#[allow(
    clippy::too_many_lines,
    reason = "the durable receipt decoder validates each versioned terminal variant"
)]
pub(super) fn decode_terminal(
    bytes: &[u8],
) -> Result<Option<(ActionTerminalRecord, usize)>, WorkspaceError> {
    if bytes.len() < 8 {
        return Ok(None);
    }
    let length = u64::from_le_bytes(
        bytes[..8]
            .try_into()
            .map_err(|_| consumption_error("action result length is malformed"))?,
    );
    let length = usize::try_from(length)
        .map_err(|_| consumption_error("action result exceeds this platform"))?;
    let payload_end = 8_usize
        .checked_add(length)
        .ok_or_else(|| consumption_error("action result length overflowed"))?;
    let frame_end = payload_end
        .checked_add(32)
        .ok_or_else(|| consumption_error("action result length overflowed"))?;
    if frame_end > bytes.len() {
        return Ok(None);
    }
    let payload = &bytes[8..payload_end];
    if peritus_codec::sha256(payload).as_bytes() != &bytes[payload_end..frame_end] {
        return Err(consumption_error("action result checksum does not match"));
    }
    let terminal = match payload.first().copied() {
        Some(1) if payload.len() >= 73 => {
            let identity = peritus_patch::PatchIdentity::from_digest(Sha256Digest::new(
                payload[1..33]
                    .try_into()
                    .map_err(|_| consumption_error("patch identity is malformed"))?,
            ));
            let manifest_digest = &payload[33..65];
            let manifest_length = u64::from_le_bytes(
                payload[65..73]
                    .try_into()
                    .map_err(|_| consumption_error("patch manifest length is malformed"))?,
            );
            let manifest_length = usize::try_from(manifest_length)
                .map_err(|_| consumption_error("patch manifest exceeds this platform"))?;
            let end = 73_usize
                .checked_add(manifest_length)
                .ok_or_else(|| consumption_error("patch manifest length overflowed"))?;
            if end != payload.len()
                || peritus_codec::sha256(&payload[73..end]).as_bytes() != manifest_digest
            {
                return Err(consumption_error("installed patch manifest is malformed"));
            }
            ActionTerminalRecord::Applied {
                patch_identity: identity,
                installed_manifest: payload[73..end].to_vec(),
            }
        }
        Some(2) if payload.len() == 1 => ActionTerminalRecord::RolledBack,
        Some(3) => {
            let mut cursor = 1_usize;
            let patch_identity =
                peritus_patch::PatchIdentity::from_digest(Sha256Digest::new(take_receipt_array::<
                    32,
                >(
                    payload,
                    &mut cursor,
                )?));
            let detail_digest = Sha256Digest::new(take_receipt_array::<32>(payload, &mut cursor)?);
            let artifact_digest = ArtifactDigest::from_sha256(Sha256Digest::new(
                take_receipt_array::<32>(payload, &mut cursor)?,
            ));
            let artifact_size = u64::from_le_bytes(take_receipt_array::<8>(payload, &mut cursor)?);
            let snapshot_manifest = take_bytes(payload, &mut cursor)?.to_vec();
            let workspace_manifest = take_bytes(payload, &mut cursor)?.to_vec();
            if cursor != payload.len() {
                return Err(consumption_error("candidate terminal receipt has trailing data"));
            }
            ActionTerminalRecord::Candidate {
                patch_identity,
                detail_digest,
                artifact_digest,
                artifact_size,
                snapshot_manifest,
                workspace_manifest,
            }
        }
        Some(4) => {
            let mut cursor = 1_usize;
            let restored_from = std::str::from_utf8(take_bytes(payload, &mut cursor)?)
                .ok()
                .and_then(|value| {
                    let format = if value.len() == 40 {
                        peritus_git::ObjectFormat::Sha1
                    } else {
                        peritus_git::ObjectFormat::Sha256
                    };
                    CommitId::parse(format, value, peritus_git::Operation::ReopenSnapshot).ok()
                })
                .ok_or_else(|| consumption_error("rollback terminal commit is malformed"))?;
            let detail_digest = Sha256Digest::new(take_receipt_array::<32>(payload, &mut cursor)?);
            let artifact_digest = ArtifactDigest::from_sha256(Sha256Digest::new(
                take_receipt_array::<32>(payload, &mut cursor)?,
            ));
            let artifact_size = u64::from_le_bytes(take_receipt_array::<8>(payload, &mut cursor)?);
            let snapshot_manifest = take_bytes(payload, &mut cursor)?.to_vec();
            let workspace_manifest = take_bytes(payload, &mut cursor)?.to_vec();
            if cursor != payload.len() {
                return Err(consumption_error("rollback terminal receipt has trailing data"));
            }
            ActionTerminalRecord::WorkspaceRollback {
                restored_from,
                detail_digest,
                artifact_digest,
                artifact_size,
                snapshot_manifest,
                workspace_manifest,
            }
        }
        _ => return Err(consumption_error("action result state is unsupported")),
    };
    Ok(Some((terminal, frame_end)))
}

fn take_array<const N: usize>(bytes: &[u8], offset: &mut usize) -> [u8; N] {
    let end = *offset + N;
    let mut result = [0_u8; N];
    result.copy_from_slice(&bytes[*offset..end]);
    *offset = end;
    result
}

fn take_receipt_array<const N: usize>(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<[u8; N], WorkspaceError> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| consumption_error("action result offset overflowed"))?;
    if end > bytes.len() {
        return Err(consumption_error("action result is truncated"));
    }
    let mut result = [0_u8; N];
    result.copy_from_slice(&bytes[*offset..end]);
    *offset = end;
    Ok(result)
}

pub(super) fn put_bytes(target: &mut Vec<u8>, value: &[u8]) -> Result<(), WorkspaceError> {
    let length = u64::try_from(value.len())
        .map_err(|_| consumption_error("action receipt field exceeds this platform"))?;
    target.extend_from_slice(&length.to_le_bytes());
    target.extend_from_slice(value);
    Ok(())
}

fn take_bytes<'a>(bytes: &'a [u8], offset: &mut usize) -> Result<&'a [u8], WorkspaceError> {
    let length = u64::from_le_bytes(take_receipt_array::<8>(bytes, offset)?);
    let length = usize::try_from(length)
        .map_err(|_| consumption_error("action receipt field exceeds this platform"))?;
    let end = offset
        .checked_add(length)
        .ok_or_else(|| consumption_error("action receipt field length overflowed"))?;
    if end > bytes.len() {
        return Err(consumption_error("action receipt field is truncated"));
    }
    let value = &bytes[*offset..end];
    *offset = end;
    Ok(value)
}
