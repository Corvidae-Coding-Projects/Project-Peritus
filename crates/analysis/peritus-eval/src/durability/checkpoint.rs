//! Atomic paged storage for complete family-87 evaluation checkpoints.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits, decode_message, encode_message};
use peritus_journal::{DurableStateRecord, SqliteJournal, StateInstall};
use peritus_types::Sha256Digest;

use crate::{
    EvaluationCampaignId, EvaluationError, EvaluationErrorKind, EvaluationOperation,
    EvaluationRecovery, EvaluationState, wire::EvaluationStateFrame,
};

use super::EVALUATION_STATE_NAMESPACE;

const ROOT_DOMAIN: &[u8] = b"peritus.evaluation.paged-checkpoint.v1\0";
const PAGE_KEY_DOMAIN: &[u8] = b"peritus.evaluation.state-page.v1\0";

pub(super) fn installs(
    journal: &SqliteJournal,
    root_key: &[u8],
    expected_root_revision: Option<u64>,
    state: &EvaluationState,
) -> Result<Vec<StateInstall>, EvaluationError> {
    let bytes = encode_message(
        &EvaluationStateFrame::from_state(state),
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    let page_bytes = peritus_journal::MAX_STATE_BYTES;
    let page_count = bytes.len().div_ceil(page_bytes);
    let page_count_u64 = u64::try_from(page_count).map_err(|_| paging())?;
    let total_bytes = u64::try_from(bytes.len()).map_err(|_| paging())?;
    let complete_digest = peritus_codec::sha256(&bytes);

    let mut values = Vec::new();
    values.try_reserve(page_count.checked_add(1).ok_or_else(paging)?).map_err(|_| paging())?;
    for (ordinal, page) in bytes.chunks(page_bytes).enumerate() {
        let ordinal = u64::try_from(ordinal).map_err(|_| paging())?;
        let key = page_key(state.campaign_id(), ordinal);
        let current = journal
            .state_record(EVALUATION_STATE_NAMESPACE, &key)
            .map_err(journal_error)?;
        let expected = current.as_ref().map(DurableStateRecord::revision);
        let revision = expected.map_or(Ok(1), |value| value.checked_add(1).ok_or_else(paging))?;
        values.push(
            StateInstall::new(EVALUATION_STATE_NAMESPACE, key, expected, revision, page.to_vec())
                .map_err(journal_error)?,
        );
    }

    let mut root = CanonicalWriter::new(CodecLimits::PRODUCTION);
    root.write_fixed(ROOT_DOMAIN).map_err(codec)?;
    root.write_fixed(state.campaign_id().as_bytes()).map_err(codec)?;
    root.write_u64(state.sequence()).map_err(codec)?;
    root.write_fixed(state.state_digest().as_bytes()).map_err(codec)?;
    root.write_u64(total_bytes).map_err(codec)?;
    root.write_u64(page_count_u64).map_err(codec)?;
    root.write_fixed(complete_digest.as_bytes()).map_err(codec)?;
    values.push(
        StateInstall::new(
            EVALUATION_STATE_NAMESPACE,
            root_key.to_vec(),
            expected_root_revision,
            state.sequence(),
            root.into_bytes(),
        )
        .map_err(journal_error)?,
    );
    values.sort_by(|left, right| {
        (left.namespace(), left.key()).cmp(&(right.namespace(), right.key()))
    });
    Ok(values)
}

pub(super) fn decode(
    journal_owner: &SqliteJournal,
    record: &DurableStateRecord,
    campaign_id: EvaluationCampaignId,
) -> Result<EvaluationStateFrame, EvaluationError> {
    if !record.bytes().starts_with(ROOT_DOMAIN) {
        return decode_message::<EvaluationStateFrame>(record.bytes(), CodecLimits::PRODUCTION)
            .map_err(codec);
    }
    let mut reader = CanonicalReader::new(&record.bytes()[ROOT_DOMAIN.len()..], CodecLimits::PRODUCTION);
    let encoded_campaign = EvaluationCampaignId::new(reader.read_fixed().map_err(codec)?)?;
    let sequence = reader.read_u64().map_err(codec)?;
    let state_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
    let total_bytes = reader.read_u64().map_err(codec)?;
    let page_count = reader.read_u64().map_err(codec)?;
    let complete_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
    reader.finish().map_err(codec)?;
    if encoded_campaign != campaign_id || sequence != record.revision() || page_count == 0 {
        return Err(recovery("paged evaluation checkpoint root identity differs"));
    }
    let capacity = usize::try_from(total_bytes).map_err(|_| paging())?;
    let count = usize::try_from(page_count).map_err(|_| paging())?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(capacity).map_err(|_| paging())?;
    for ordinal in 0..count {
        let key = page_key(campaign_id, u64::try_from(ordinal).map_err(|_| paging())?);
        let page = journal_owner
            .state_record(EVALUATION_STATE_NAMESPACE, &key)
            .map_err(journal_error)?
            .ok_or_else(|| recovery("paged evaluation checkpoint is missing a state page"))?;
        if page.producing_position() != record.producing_position() {
            return Err(recovery("paged evaluation checkpoint contains a stale state page"));
        }
        let length = bytes
            .len()
            .checked_add(page.bytes().len())
            .ok_or_else(|| recovery("paged evaluation checkpoint byte length overflowed"))?;
        if length > capacity {
            return Err(recovery("paged evaluation checkpoint exceeds its declared byte length"));
        }
        bytes.extend_from_slice(page.bytes());
    }
    if bytes.len() != capacity || peritus_codec::sha256(&bytes) != complete_digest {
        return Err(recovery("paged evaluation checkpoint bytes differ from their root"));
    }
    let frame = decode_message::<EvaluationStateFrame>(&bytes, CodecLimits::PRODUCTION)
        .map_err(codec)?;
    if frame.campaign_id() != campaign_id
        || frame.sequence() != sequence
        || frame.state_digest() != state_digest
    {
        return Err(recovery("paged evaluation checkpoint semantics differ from their root"));
    }
    Ok(frame)
}

fn page_key(campaign_id: EvaluationCampaignId, ordinal: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(PAGE_KEY_DOMAIN.len() + campaign_id.as_bytes().len() + 8);
    key.extend_from_slice(PAGE_KEY_DOMAIN);
    key.extend_from_slice(campaign_id.as_bytes());
    key.extend_from_slice(&ordinal.to_be_bytes());
    key
}

const fn codec(_: peritus_codec::CodecError) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Corruption,
        EvaluationOperation::Codec,
        EvaluationRecovery::Quarantine,
        "evaluation checkpoint violates canonical paging",
    )
}

fn journal_error(_: impl core::fmt::Display) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Journal,
        EvaluationOperation::Recover,
        EvaluationRecovery::Replay,
        "C0 failed while reading or writing an evaluation checkpoint page",
    )
}

const fn paging() -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::LimitExceeded,
        EvaluationOperation::Commit,
        EvaluationRecovery::Replay,
        "evaluation checkpoint paging is not representable or allocatable",
    )
}

const fn recovery(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Recovery,
        EvaluationOperation::Recover,
        EvaluationRecovery::Quarantine,
        detail,
    )
}
