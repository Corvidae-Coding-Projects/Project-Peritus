//! Atomic sequence-addressed paging for complete F0 checkpoints.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits, decode_message, encode_message};
use peritus_journal::{DurableStateRecord, SqliteJournal, StateInstall};
use peritus_types::Sha256Digest;

use crate::{
    CampaignState, EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery,
    EvolutionStorageLimits, ProductionHarnessState,
    wire::{
        CampaignCheckpointFrame, CampaignStateFrame, PointerCheckpointFrame, PointerStateFrame,
    },
};

use super::{CAMPAIGN_STATE_NAMESPACE, POINTER_STATE_NAMESPACE};

const CAMPAIGN_ROOT_DOMAIN: &[u8] = b"peritus.f0.campaign-paged-checkpoint.v1\0";
const POINTER_ROOT_DOMAIN: &[u8] = b"peritus.f0.pointer-paged-checkpoint.v1\0";
const CAMPAIGN_PAGE_KEY_DOMAIN: &[u8] = b"peritus.f0.campaign-state-page.v1\0";
const POINTER_PAGE_KEY_DOMAIN: &[u8] = b"peritus.f0.pointer-state-page.v1\0";

#[derive(Clone, Copy)]
enum CheckpointKind {
    Campaign,
    Pointer,
}

impl CheckpointKind {
    const fn namespace(self) -> u16 {
        match self {
            Self::Campaign => CAMPAIGN_STATE_NAMESPACE,
            Self::Pointer => POINTER_STATE_NAMESPACE,
        }
    }

    const fn root_domain(self) -> &'static [u8] {
        match self {
            Self::Campaign => CAMPAIGN_ROOT_DOMAIN,
            Self::Pointer => POINTER_ROOT_DOMAIN,
        }
    }

    const fn page_key_domain(self) -> &'static [u8] {
        match self {
            Self::Campaign => CAMPAIGN_PAGE_KEY_DOMAIN,
            Self::Pointer => POINTER_PAGE_KEY_DOMAIN,
        }
    }
}

#[derive(Clone, Copy)]
struct Root {
    sequence: u64,
    state_digest: Sha256Digest,
    total_bytes: usize,
    page_bytes: usize,
    page_count: usize,
    complete_digest: Sha256Digest,
}

pub(super) enum CampaignCheckpoint {
    Reference(CampaignCheckpointFrame),
    Legacy(CampaignState),
}

impl CampaignCheckpoint {
    pub(super) const fn campaign_id(&self) -> crate::EvolutionCampaignId {
        match self {
            Self::Reference(frame) => frame.campaign_id(),
            Self::Legacy(state) => state.campaign_id(),
        }
    }

    pub(super) const fn sequence(&self) -> u64 {
        match self {
            Self::Reference(frame) => frame.sequence(),
            Self::Legacy(state) => state.sequence(),
        }
    }

    pub(super) const fn last_event_id(&self) -> peritus_types::EventId {
        match self {
            Self::Reference(frame) => frame.last_event_id(),
            Self::Legacy(state) => state.last_event(),
        }
    }

    pub(super) const fn state_digest(&self) -> Sha256Digest {
        match self {
            Self::Reference(frame) => frame.state_digest(),
            Self::Legacy(state) => state.state_digest(),
        }
    }

    pub(super) fn matches_state(&self, state: &CampaignState) -> bool {
        match self {
            Self::Reference(frame) => frame.matches_state(state),
            Self::Legacy(observed) => observed == state,
        }
    }
}

pub(super) enum PointerCheckpoint {
    Reference(PointerCheckpointFrame),
    Legacy(ProductionHarnessState),
}

impl PointerCheckpoint {
    pub(super) const fn project_id(&self) -> peritus_types::ProjectId {
        match self {
            Self::Reference(frame) => frame.project_id(),
            Self::Legacy(state) => state.project_id(),
        }
    }

    pub(super) const fn generation(&self) -> u64 {
        match self {
            Self::Reference(frame) => frame.generation(),
            Self::Legacy(state) => state.generation(),
        }
    }

    pub(super) const fn sequence(&self) -> u64 {
        match self {
            Self::Reference(frame) => frame.sequence(),
            Self::Legacy(state) => state.sequence(),
        }
    }

    pub(super) const fn last_event_id(&self) -> peritus_types::EventId {
        match self {
            Self::Reference(frame) => frame.last_event_id(),
            Self::Legacy(state) => state.last_event(),
        }
    }

    pub(super) const fn state_digest(&self) -> Sha256Digest {
        match self {
            Self::Reference(frame) => frame.state_digest(),
            Self::Legacy(state) => state.state_digest(),
        }
    }

    pub(super) const fn policy_digest(&self) -> Sha256Digest {
        match self {
            Self::Reference(frame) => frame.policy_digest(),
            Self::Legacy(state) => state.policy().digest(),
        }
    }

    pub(super) fn revision_digest(&self) -> Sha256Digest {
        match self {
            Self::Reference(frame) => frame.revision_digest(),
            Self::Legacy(state) => super::pointer::pointer_event_revision_digest(state),
        }
    }

    pub(super) fn legacy_state(&self) -> Option<&ProductionHarnessState> {
        match self {
            Self::Reference(_) => None,
            Self::Legacy(state) => Some(state),
        }
    }

    pub(super) fn matches_state(&self, state: &ProductionHarnessState) -> bool {
        match self {
            Self::Reference(frame) => frame.matches_state(state),
            Self::Legacy(observed) => observed == state,
        }
    }
}

pub(super) fn campaign_installs(
    root_key: &[u8],
    expected_root_revision: Option<u64>,
    state: &CampaignState,
    storage: EvolutionStorageLimits,
) -> Result<Vec<StateInstall>, EvolutionError> {
    let bytes = campaign_state_bytes(state)?;
    installs(
        CheckpointKind::Campaign,
        root_key,
        expected_root_revision,
        state.sequence(),
        state.campaign_id().as_bytes(),
        state.state_digest(),
        &bytes,
        storage,
    )
}

pub(super) fn pointer_installs(
    root_key: &[u8],
    expected_root_revision: Option<u64>,
    state: &ProductionHarnessState,
    storage: EvolutionStorageLimits,
) -> Result<Vec<StateInstall>, EvolutionError> {
    let bytes = pointer_state_bytes(state)?;
    installs(
        CheckpointKind::Pointer,
        root_key,
        expected_root_revision,
        state.sequence(),
        state.project_id().as_bytes(),
        state.state_digest(),
        &bytes,
        storage,
    )
}

pub(super) fn campaign_compatible_installs(
    journal: &SqliteJournal,
    root_key: &[u8],
    expected_root_revision: Option<u64>,
    state: &CampaignState,
    storage: EvolutionStorageLimits,
) -> Result<Vec<StateInstall>, EvolutionError> {
    let bytes = campaign_state_bytes(state)?;
    compatible_installs(
        journal,
        CheckpointKind::Campaign,
        root_key,
        expected_root_revision,
        state.sequence(),
        state.campaign_id().as_bytes(),
        state.state_digest(),
        &bytes,
        storage,
        |historical| {
            decode_campaign_bytes(historical).map(|checkpoint| checkpoint.matches_state(state))
        },
    )
}

pub(super) fn pointer_compatible_installs(
    journal: &SqliteJournal,
    root_key: &[u8],
    expected_root_revision: Option<u64>,
    state: &ProductionHarnessState,
    storage: EvolutionStorageLimits,
) -> Result<Vec<StateInstall>, EvolutionError> {
    let bytes = pointer_state_bytes(state)?;
    compatible_installs(
        journal,
        CheckpointKind::Pointer,
        root_key,
        expected_root_revision,
        state.sequence(),
        state.project_id().as_bytes(),
        state.state_digest(),
        &bytes,
        storage,
        |historical| {
            decode_pointer_bytes(historical).map(|checkpoint| checkpoint.matches_state(state))
        },
    )
}

pub(super) fn decode_campaign(
    journal: &SqliteJournal,
    record: &DurableStateRecord,
    campaign_id: crate::EvolutionCampaignId,
) -> Result<CampaignCheckpoint, EvolutionError> {
    let (bytes, root) = load_bytes(
        journal,
        CheckpointKind::Campaign,
        record,
        campaign_id.as_bytes(),
    )?;
    let frame = decode_campaign_bytes(&bytes)?;
    if frame.campaign_id() != campaign_id
        || frame.sequence() != record.revision()
        || root.is_some_and(|root| root.state_digest != frame.state_digest())
    {
        return Err(recovery("campaign checkpoint root differs from its complete state"));
    }
    Ok(frame)
}

pub(super) fn decode_pointer(
    journal: &SqliteJournal,
    record: &DurableStateRecord,
    project_id: peritus_types::ProjectId,
) -> Result<PointerCheckpoint, EvolutionError> {
    let (bytes, root) = load_bytes(
        journal,
        CheckpointKind::Pointer,
        record,
        project_id.as_bytes(),
    )?;
    let frame = decode_pointer_bytes(&bytes)?;
    if frame.project_id() != project_id
        || frame.sequence() != record.revision()
        || root.is_some_and(|root| root.state_digest != frame.state_digest())
    {
        return Err(recovery("pointer checkpoint root differs from its complete state"));
    }
    Ok(frame)
}

fn campaign_state_bytes(state: &CampaignState) -> Result<Vec<u8>, EvolutionError> {
    encode_message(
        &CampaignCheckpointFrame::from_state(state),
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)
}

fn pointer_state_bytes(state: &ProductionHarnessState) -> Result<Vec<u8>, EvolutionError> {
    encode_message(
        &PointerCheckpointFrame::from_state(state),
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)
}

#[allow(clippy::too_many_arguments)]
fn compatible_installs(
    journal: &SqliteJournal,
    kind: CheckpointKind,
    root_key: &[u8],
    expected_root_revision: Option<u64>,
    revision: u64,
    owner: &[u8; 16],
    state_digest: Sha256Digest,
    bytes: &[u8],
    requested: EvolutionStorageLimits,
    matches_state: impl FnOnce(&[u8]) -> Result<bool, EvolutionError>,
) -> Result<Vec<StateInstall>, EvolutionError> {
    let historical = journal
        .state_record_revision(kind.namespace(), root_key, revision)
        .map_err(journal_error)?;
    if let Some(record) = historical {
        let (historical_bytes, root) = load_bytes(journal, kind, &record, owner)?;
        if root.is_some_and(|root| root.state_digest != state_digest)
            || !matches_state(&historical_bytes)?
        {
            return Err(recovery("historical checkpoint differs from the successor"));
        }
        if let Some(root) = root {
            let storage = EvolutionStorageLimits::new(root.page_bytes)?;
            return installs(
                kind,
                root_key,
                expected_root_revision,
                revision,
                owner,
                state_digest,
                &historical_bytes,
                storage,
            );
        }
        return Ok(vec![StateInstall::new(
            kind.namespace(),
            root_key.to_vec(),
            expected_root_revision,
            revision,
            historical_bytes,
        )
        .map_err(journal_error)?]);
    }
    installs(
        kind,
        root_key,
        expected_root_revision,
        revision,
        owner,
        state_digest,
        bytes,
        requested,
    )
}

fn decode_campaign_bytes(bytes: &[u8]) -> Result<CampaignCheckpoint, EvolutionError> {
    if let Ok(frame) =
        decode_message::<CampaignCheckpointFrame>(bytes, CodecLimits::PRODUCTION)
    {
        return Ok(CampaignCheckpoint::Reference(frame));
    }
    decode_message::<CampaignStateFrame>(bytes, CodecLimits::PRODUCTION)
        .map(CampaignStateFrame::into_state)
        .map(CampaignCheckpoint::Legacy)
        .map_err(codec)
}

fn decode_pointer_bytes(bytes: &[u8]) -> Result<PointerCheckpoint, EvolutionError> {
    if let Ok(frame) = decode_message::<PointerCheckpointFrame>(bytes, CodecLimits::PRODUCTION) {
        return Ok(PointerCheckpoint::Reference(frame));
    }
    decode_message::<PointerStateFrame>(bytes, CodecLimits::PRODUCTION)
        .map(PointerStateFrame::into_state)
        .map(PointerCheckpoint::Legacy)
        .map_err(codec)
}

#[allow(clippy::too_many_arguments)]
fn installs(
    kind: CheckpointKind,
    root_key: &[u8],
    expected_root_revision: Option<u64>,
    revision: u64,
    owner: &[u8; 16],
    state_digest: Sha256Digest,
    bytes: &[u8],
    storage: EvolutionStorageLimits,
) -> Result<Vec<StateInstall>, EvolutionError> {
    let page_bytes = storage.state_page_bytes();
    let page_count = bytes.len().div_ceil(page_bytes);
    if bytes.is_empty() || page_count == 0 {
        return Err(recovery("canonical checkpoint state is empty"));
    }
    let mut values = Vec::new();
    values
        .try_reserve(page_count.checked_add(1).ok_or_else(paging)?)
        .map_err(|_| paging())?;
    for (ordinal, page) in bytes.chunks(page_bytes).enumerate() {
        values.push(
            StateInstall::new(
                kind.namespace(),
                page_key(
                    kind,
                    owner,
                    revision,
                    u64::try_from(ordinal).map_err(|_| paging())?,
                ),
                None,
                1,
                page.to_vec(),
            )
            .map_err(journal_error)?,
        );
    }
    let root = root_bytes(
        kind,
        owner,
        revision,
        state_digest,
        bytes,
        page_bytes,
        page_count,
    )?;
    values.push(
        StateInstall::new(
            kind.namespace(),
            root_key.to_vec(),
            expected_root_revision,
            revision,
            root,
        )
        .map_err(journal_error)?,
    );
    values.sort_by(|left, right| {
        (left.namespace(), left.key()).cmp(&(right.namespace(), right.key()))
    });
    Ok(values)
}

fn root_bytes(
    kind: CheckpointKind,
    owner: &[u8; 16],
    sequence: u64,
    state_digest: Sha256Digest,
    bytes: &[u8],
    page_bytes: usize,
    page_count: usize,
) -> Result<Vec<u8>, EvolutionError> {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    writer.write_fixed(kind.root_domain()).map_err(codec)?;
    writer.write_fixed(owner).map_err(codec)?;
    writer.write_u64(sequence).map_err(codec)?;
    writer.write_fixed(state_digest.as_bytes()).map_err(codec)?;
    writer.write_u64(u64::try_from(bytes.len()).map_err(|_| paging())?).map_err(codec)?;
    writer.write_u64(u64::try_from(page_bytes).map_err(|_| paging())?).map_err(codec)?;
    writer.write_u64(u64::try_from(page_count).map_err(|_| paging())?).map_err(codec)?;
    writer.write_fixed(peritus_codec::sha256(bytes).as_bytes()).map_err(codec)?;
    Ok(writer.into_bytes())
}

fn load_bytes(
    journal: &SqliteJournal,
    kind: CheckpointKind,
    record: &DurableStateRecord,
    owner: &[u8; 16],
) -> Result<(Vec<u8>, Option<Root>), EvolutionError> {
    if !record.bytes().starts_with(kind.root_domain()) {
        return Ok((record.bytes().to_vec(), None));
    }
    let root = parse_root(kind, record, owner)?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(root.total_bytes).map_err(|_| paging())?;
    for ordinal in 0..root.page_count {
        let page = journal
            .state_record_revision(
                kind.namespace(),
                &page_key(
                    kind,
                    owner,
                    root.sequence,
                    u64::try_from(ordinal).map_err(|_| paging())?,
                ),
                1,
            )
            .map_err(journal_error)?
            .ok_or_else(|| recovery("paged evolution checkpoint page is absent"))?;
        let expected = if ordinal + 1 == root.page_count {
            root.total_bytes
                .checked_sub(
                    root.page_bytes
                        .checked_mul(root.page_count - 1)
                        .ok_or_else(paging)?,
                )
                .ok_or_else(paging)?
        } else {
            root.page_bytes
        };
        if page.revision() != 1
            || page.bytes().len() != expected
            || page.producing_position() != record.producing_position()
            || bytes.len().checked_add(page.bytes().len()).is_none_or(|length| {
                length > root.total_bytes
            })
        {
            return Err(recovery("paged evolution checkpoint page differs from its root"));
        }
        bytes.extend_from_slice(page.bytes());
    }
    if bytes.len() != root.total_bytes || peritus_codec::sha256(&bytes) != root.complete_digest {
        return Err(recovery("complete paged evolution checkpoint digest differs"));
    }
    Ok((bytes, Some(root)))
}

fn parse_root(
    kind: CheckpointKind,
    record: &DurableStateRecord,
    owner: &[u8; 16],
) -> Result<Root, EvolutionError> {
    let mut reader = CanonicalReader::new(
        &record.bytes()[kind.root_domain().len()..],
        CodecLimits::PRODUCTION,
    );
    let encoded_owner: [u8; 16] = reader.read_fixed().map_err(codec)?;
    let sequence = reader.read_u64().map_err(codec)?;
    let state_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
    let total_bytes = usize::try_from(reader.read_u64().map_err(codec)?).map_err(|_| paging())?;
    let page_bytes = usize::try_from(reader.read_u64().map_err(codec)?).map_err(|_| paging())?;
    let page_count = usize::try_from(reader.read_u64().map_err(codec)?).map_err(|_| paging())?;
    let complete_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
    reader.finish().map_err(codec)?;
    if encoded_owner != *owner
        || sequence == 0
        || sequence != record.revision()
        || total_bytes == 0
        || page_bytes == 0
        || page_bytes > peritus_journal::MAX_STATE_BYTES
        || page_count == 0
        || page_count != total_bytes.div_ceil(page_bytes)
    {
        return Err(recovery("paged evolution checkpoint root is invalid"));
    }
    Ok(Root { sequence, state_digest, total_bytes, page_bytes, page_count, complete_digest })
}

fn page_key(
    kind: CheckpointKind,
    owner: &[u8; 16],
    sequence: u64,
    ordinal: u64,
) -> Vec<u8> {
    let domain = kind.page_key_domain();
    let mut key = Vec::with_capacity(domain.len() + owner.len() + 16);
    key.extend_from_slice(domain);
    key.extend_from_slice(owner);
    key.extend_from_slice(&sequence.to_be_bytes());
    key.extend_from_slice(&ordinal.to_be_bytes());
    key
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "map_err supplies the owned codec failure to this redacting boundary"
)]
fn codec(error: impl core::fmt::Display) -> EvolutionError {
    drop(error);
    EvolutionError::new(
        EvolutionErrorKind::Codec,
        EvolutionOperation::Codec,
        EvolutionRecovery::Quarantine,
        "evolution checkpoint violates canonical paging protocol",
    )
}

fn journal_error(error: peritus_journal::JournalError) -> EvolutionError {
    let recovery = match error.kind() {
        peritus_journal::JournalErrorKind::Busy
        | peritus_journal::JournalErrorKind::IndeterminateCommit => EvolutionRecovery::Retry,
        peritus_journal::JournalErrorKind::StaleHead
        | peritus_journal::JournalErrorKind::StaleAuthorityEpoch
        | peritus_journal::JournalErrorKind::StaleRegistry => EvolutionRecovery::RefreshState,
        _ => EvolutionRecovery::Replay,
    };
    EvolutionError::new(
        EvolutionErrorKind::Journal,
        EvolutionOperation::Commit,
        recovery,
        "C0 could not read or write an evolution checkpoint page",
    )
}

const fn paging() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::LimitExceeded,
        EvolutionOperation::Commit,
        EvolutionRecovery::Retry,
        "evolution checkpoint paging is not representable or allocatable",
    )
}

const fn recovery(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Corruption,
        EvolutionOperation::Recover,
        EvolutionRecovery::Quarantine,
        detail,
    )
}
