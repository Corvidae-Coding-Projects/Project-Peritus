//! Atomic content-addressed paging for complete family-87 evaluation checkpoints.

use std::collections::BTreeSet;

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits, decode_message, encode_message};
use peritus_journal::{DurableStateRecord, SqliteJournal, StateInstall};
use peritus_types::Sha256Digest;

use crate::{
    EvaluationCampaignId, EvaluationError, EvaluationErrorKind, EvaluationOperation,
    EvaluationRecovery, EvaluationState, wire::EvaluationStateFrame,
};

use super::EVALUATION_STATE_NAMESPACE;

const LEGACY_ROOT_DOMAIN: &[u8] = b"peritus.evaluation.paged-checkpoint.v1\0";
const CONTENT_ROOT_DOMAIN: &[u8] = b"peritus.evaluation.paged-checkpoint.v2\0";
const SEQUENCE_ROOT_DOMAIN: &[u8] = b"peritus.evaluation.paged-checkpoint.v2-sequence\0";
const LEGACY_PAGE_KEY_DOMAIN: &[u8] = b"peritus.evaluation.state-page.v1\0";
const CONTENT_PAGE_KEY_DOMAIN: &[u8] = b"peritus.evaluation.state-page.v2\0";
const SEQUENCE_PAGE_KEY_DOMAIN: &[u8] = b"peritus.evaluation.state-page.v2-sequence\0";
const PAGE_DESCRIPTOR_BYTES: usize = 40;

#[derive(Clone, Copy)]
struct PageDescriptor {
    digest: Sha256Digest,
    length: u64,
}

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
    let page_bytes = usize::try_from(state.state_page_bytes()).map_err(|_| paging())?;
    if page_bytes == 0 || page_bytes > peritus_journal::MAX_STATE_BYTES {
        return Err(paging());
    }
    let chunks = content_chunks(&bytes, page_bytes);
    let descriptors = chunks
        .iter()
        .map(|page| {
            Ok(PageDescriptor {
                digest: peritus_codec::sha256(page),
                length: u64::try_from(page.len()).map_err(|_| paging())?,
            })
        })
        .collect::<Result<Vec<_>, EvaluationError>>()?;
    let descriptor_bytes = descriptors
        .len()
        .checked_mul(PAGE_DESCRIPTOR_BYTES)
        .and_then(|value| value.checked_add(256))
        .ok_or_else(paging)?;
    if descriptor_bytes > peritus_journal::MAX_STATE_BYTES {
        return sequence_installs(
            journal,
            root_key,
            expected_root_revision,
            state,
            &bytes,
            page_bytes,
        );
    }
    let root = content_root(state, &bytes, page_bytes, &descriptors)?;

    let mut values = Vec::new();
    values
        .try_reserve(descriptors.len().checked_add(1).ok_or_else(paging)?)
        .map_err(|_| paging())?;
    let mut observed = BTreeSet::new();
    for (page, descriptor) in chunks.into_iter().zip(&descriptors) {
        if !observed.insert(descriptor.digest) {
            continue;
        }
        let key = content_page_key(state.campaign_id(), descriptor.digest);
        if let Some(current) = journal
            .state_record(EVALUATION_STATE_NAMESPACE, &key)
            .map_err(journal_error)?
        {
            if current.revision() != 1
                || current.digest() != descriptor.digest
                || current.bytes() != page
            {
                return Err(recovery(
                    "content-addressed evaluation checkpoint page differs from its identity",
                ));
            }
        } else {
            values.push(
                StateInstall::new(EVALUATION_STATE_NAMESPACE, key, None, 1, page.to_vec())
                    .map_err(journal_error)?,
            );
        }
    }
    values.push(
        StateInstall::new(
            EVALUATION_STATE_NAMESPACE,
            root_key.to_vec(),
            expected_root_revision,
            state.sequence(),
            root,
        )
        .map_err(journal_error)?,
    );
    values.sort_by(|left, right| {
        (left.namespace(), left.key()).cmp(&(right.namespace(), right.key()))
    });
    Ok(values)
}

fn content_root(
    state: &EvaluationState,
    bytes: &[u8],
    page_bytes: usize,
    descriptors: &[PageDescriptor],
) -> Result<Vec<u8>, EvaluationError> {
    let mut root = CanonicalWriter::new(CodecLimits::PRODUCTION);
    root.write_fixed(CONTENT_ROOT_DOMAIN).map_err(codec)?;
    root.write_fixed(state.campaign_id().as_bytes()).map_err(codec)?;
    root.write_u64(state.sequence()).map_err(codec)?;
    root.write_fixed(state.state_digest().as_bytes()).map_err(codec)?;
    root.write_u64(u64::try_from(bytes.len()).map_err(|_| paging())?).map_err(codec)?;
    root.write_u64(u64::try_from(page_bytes).map_err(|_| paging())?).map_err(codec)?;
    root.write_collection_len(descriptors.len()).map_err(codec)?;
    root.write_fixed(peritus_codec::sha256(bytes).as_bytes()).map_err(codec)?;
    for descriptor in descriptors {
        root.write_fixed(descriptor.digest.as_bytes()).map_err(codec)?;
        root.write_u64(descriptor.length).map_err(codec)?;
    }
    Ok(root.into_bytes())
}

fn sequence_installs(
    journal: &SqliteJournal,
    root_key: &[u8],
    expected_root_revision: Option<u64>,
    state: &EvaluationState,
    bytes: &[u8],
    page_bytes: usize,
) -> Result<Vec<StateInstall>, EvaluationError> {
    let page_count = bytes.len().div_ceil(page_bytes);
    let mut values = Vec::new();
    values
        .try_reserve(page_count.checked_add(1).ok_or_else(paging)?)
        .map_err(|_| paging())?;
    for (ordinal, page) in bytes.chunks(page_bytes).enumerate() {
        let key = sequence_page_key(
            state.campaign_id(),
            state.sequence(),
            u64::try_from(ordinal).map_err(|_| paging())?,
        );
        if let Some(current) = journal
            .state_record(EVALUATION_STATE_NAMESPACE, &key)
            .map_err(journal_error)?
        {
            if current.revision() != 1 || current.bytes() != page {
                return Err(recovery(
                    "sequence-addressed evaluation checkpoint page differs from its identity",
                ));
            }
        } else {
            values.push(
                StateInstall::new(EVALUATION_STATE_NAMESPACE, key, None, 1, page.to_vec())
                    .map_err(journal_error)?,
            );
        }
    }
    let mut root = CanonicalWriter::new(CodecLimits::PRODUCTION);
    root.write_fixed(SEQUENCE_ROOT_DOMAIN).map_err(codec)?;
    root.write_fixed(state.campaign_id().as_bytes()).map_err(codec)?;
    root.write_u64(state.sequence()).map_err(codec)?;
    root.write_fixed(state.state_digest().as_bytes()).map_err(codec)?;
    root.write_u64(u64::try_from(bytes.len()).map_err(|_| paging())?).map_err(codec)?;
    root.write_u64(u64::try_from(page_bytes).map_err(|_| paging())?).map_err(codec)?;
    root.write_u64(u64::try_from(page_count).map_err(|_| paging())?).map_err(codec)?;
    root.write_fixed(peritus_codec::sha256(bytes).as_bytes()).map_err(codec)?;
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
    journal: &SqliteJournal,
    record: &DurableStateRecord,
    campaign_id: EvaluationCampaignId,
) -> Result<EvaluationStateFrame, EvaluationError> {
    decode_record(journal, record, campaign_id, false)
}

pub(super) fn decode_historical(
    journal: &SqliteJournal,
    record: &DurableStateRecord,
    campaign_id: EvaluationCampaignId,
) -> Result<EvaluationStateFrame, EvaluationError> {
    decode_record(journal, record, campaign_id, true)
}

pub(super) fn validate_event_root(
    record: &DurableStateRecord,
    campaign_id: EvaluationCampaignId,
    event: &crate::EvaluationEvent,
) -> Result<(), EvaluationError> {
    if record.revision() != event.sequence() || event.campaign_id() != campaign_id {
        return Err(recovery("evaluation event has another historical checkpoint root"));
    }
    for domain in [LEGACY_ROOT_DOMAIN, CONTENT_ROOT_DOMAIN, SEQUENCE_ROOT_DOMAIN] {
        if record.bytes().starts_with(domain) {
            let mut reader = CanonicalReader::new(
                &record.bytes()[domain.len()..],
                CodecLimits::PRODUCTION,
            );
            let encoded_campaign =
                EvaluationCampaignId::new(reader.read_fixed().map_err(codec)?)?;
            let sequence = reader.read_u64().map_err(codec)?;
            let state_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
            return if encoded_campaign == campaign_id
                && sequence == event.sequence()
                && state_digest == event.successor_state_digest()
            {
                Ok(())
            } else {
                Err(recovery(
                    "evaluation event differs from its historical checkpoint root",
                ))
            };
        }
    }
    let frame = decode_message::<EvaluationStateFrame>(record.bytes(), CodecLimits::PRODUCTION)
        .map_err(codec)?;
    if frame.campaign_id() == campaign_id
        && frame.sequence() == event.sequence()
        && frame.state_digest() == event.successor_state_digest()
    {
        Ok(())
    } else {
        Err(recovery(
            "evaluation event differs from its historical inline checkpoint",
        ))
    }
}

fn decode_record(
    journal: &SqliteJournal,
    record: &DurableStateRecord,
    campaign_id: EvaluationCampaignId,
    historical: bool,
) -> Result<EvaluationStateFrame, EvaluationError> {
    if record.bytes().starts_with(CONTENT_ROOT_DOMAIN) {
        return decode_content(journal, record, campaign_id);
    }
    if record.bytes().starts_with(SEQUENCE_ROOT_DOMAIN) {
        return decode_sequence(journal, record, campaign_id);
    }
    if record.bytes().starts_with(LEGACY_ROOT_DOMAIN) {
        return decode_legacy(journal, record, campaign_id, historical);
    }
    let frame =
        decode_message::<EvaluationStateFrame>(record.bytes(), CodecLimits::PRODUCTION)
            .map_err(codec)?;
    validate_frame(record, campaign_id, &frame)?;
    Ok(frame)
}

fn decode_content(
    journal: &SqliteJournal,
    record: &DurableStateRecord,
    campaign_id: EvaluationCampaignId,
) -> Result<EvaluationStateFrame, EvaluationError> {
    let mut reader = CanonicalReader::new(
        &record.bytes()[CONTENT_ROOT_DOMAIN.len()..],
        CodecLimits::PRODUCTION,
    );
    let encoded_campaign = EvaluationCampaignId::new(reader.read_fixed().map_err(codec)?)?;
    let sequence = reader.read_u64().map_err(codec)?;
    let state_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
    let total_bytes = reader.read_u64().map_err(codec)?;
    let page_bytes = reader.read_u64().map_err(codec)?;
    let count = reader.read_collection_len(PAGE_DESCRIPTOR_BYTES).map_err(codec)?;
    let complete_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
    let mut descriptors = Vec::new();
    descriptors.try_reserve_exact(count).map_err(|_| paging())?;
    for _ in 0..count {
        descriptors.push(PageDescriptor {
            digest: Sha256Digest::new(reader.read_fixed().map_err(codec)?),
            length: reader.read_u64().map_err(codec)?,
        });
    }
    reader.finish().map_err(codec)?;
    validate_root_identity(record, campaign_id, encoded_campaign, sequence, count)?;
    let capacity = usize::try_from(total_bytes).map_err(|_| paging())?;
    let page_bytes = usize::try_from(page_bytes).map_err(|_| paging())?;
    if page_bytes == 0 || page_bytes > peritus_journal::MAX_STATE_BYTES {
        return Err(recovery("content-addressed checkpoint page size is invalid"));
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(capacity).map_err(|_| paging())?;
    for descriptor in &descriptors {
        let key = content_page_key(campaign_id, descriptor.digest);
        let page = journal
            .state_record_revision(EVALUATION_STATE_NAMESPACE, &key, 1)
            .map_err(journal_error)?
            .ok_or_else(|| recovery("content-addressed evaluation checkpoint page is absent"))?;
        if page.digest() != descriptor.digest
            || u64::try_from(page.bytes().len()).map_err(|_| paging())? != descriptor.length
            || page.bytes().len() > page_bytes
            || page.producing_position() > record.producing_position()
        {
            return Err(recovery(
                "content-addressed evaluation checkpoint page differs from its root",
            ));
        }
        append_page(&mut bytes, page.bytes(), capacity)?;
    }
    let canonical = content_chunks(&bytes, page_bytes);
    if canonical.len() != descriptors.len()
        || canonical.iter().zip(&descriptors).any(|(page, descriptor)| {
            peritus_codec::sha256(page) != descriptor.digest
                || u64::try_from(page.len()).ok() != Some(descriptor.length)
        })
    {
        return Err(recovery("checkpoint content boundaries are not canonical"));
    }
    finish_paged_frame(
        record,
        campaign_id,
        sequence,
        state_digest,
        complete_digest,
        capacity,
        bytes,
    )
}

fn decode_sequence(
    journal: &SqliteJournal,
    record: &DurableStateRecord,
    campaign_id: EvaluationCampaignId,
) -> Result<EvaluationStateFrame, EvaluationError> {
    let mut reader = CanonicalReader::new(
        &record.bytes()[SEQUENCE_ROOT_DOMAIN.len()..],
        CodecLimits::PRODUCTION,
    );
    let encoded_campaign = EvaluationCampaignId::new(reader.read_fixed().map_err(codec)?)?;
    let sequence = reader.read_u64().map_err(codec)?;
    let state_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
    let total_bytes = reader.read_u64().map_err(codec)?;
    let page_bytes = reader.read_u64().map_err(codec)?;
    let page_count = reader.read_u64().map_err(codec)?;
    let complete_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
    reader.finish().map_err(codec)?;
    let count = usize::try_from(page_count).map_err(|_| paging())?;
    validate_root_identity(record, campaign_id, encoded_campaign, sequence, count)?;
    let capacity = usize::try_from(total_bytes).map_err(|_| paging())?;
    let page_bytes = usize::try_from(page_bytes).map_err(|_| paging())?;
    if page_bytes == 0
        || page_bytes > peritus_journal::MAX_STATE_BYTES
        || count != capacity.div_ceil(page_bytes)
    {
        return Err(recovery("sequence-addressed checkpoint paging is invalid"));
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(capacity).map_err(|_| paging())?;
    for ordinal in 0..count {
        let key = sequence_page_key(
            campaign_id,
            sequence,
            u64::try_from(ordinal).map_err(|_| paging())?,
        );
        let page = journal
            .state_record_revision(EVALUATION_STATE_NAMESPACE, &key, 1)
            .map_err(journal_error)?
            .ok_or_else(|| recovery("sequence-addressed evaluation checkpoint page is absent"))?;
        if page.producing_position() != record.producing_position() {
            return Err(recovery("sequence-addressed checkpoint page has another producer"));
        }
        append_page(&mut bytes, page.bytes(), capacity)?;
    }
    validate_fixed_boundaries(capacity, page_bytes, &bytes, count)?;
    finish_paged_frame(
        record,
        campaign_id,
        sequence,
        state_digest,
        complete_digest,
        capacity,
        bytes,
    )
}

fn decode_legacy(
    journal: &SqliteJournal,
    record: &DurableStateRecord,
    campaign_id: EvaluationCampaignId,
    historical: bool,
) -> Result<EvaluationStateFrame, EvaluationError> {
    let mut reader = CanonicalReader::new(
        &record.bytes()[LEGACY_ROOT_DOMAIN.len()..],
        CodecLimits::PRODUCTION,
    );
    let encoded_campaign = EvaluationCampaignId::new(reader.read_fixed().map_err(codec)?)?;
    let sequence = reader.read_u64().map_err(codec)?;
    let state_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
    let total_bytes = reader.read_u64().map_err(codec)?;
    let page_count = reader.read_u64().map_err(codec)?;
    let complete_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
    reader.finish().map_err(codec)?;
    let count = usize::try_from(page_count).map_err(|_| paging())?;
    validate_root_identity(record, campaign_id, encoded_campaign, sequence, count)?;
    let capacity = usize::try_from(total_bytes).map_err(|_| paging())?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(capacity).map_err(|_| paging())?;
    let mut lengths = Vec::new();
    lengths.try_reserve_exact(count).map_err(|_| paging())?;
    for ordinal in 0..count {
        let key = legacy_page_key(campaign_id, u64::try_from(ordinal).map_err(|_| paging())?);
        let page = if historical {
            journal.state_record_at_position(
                EVALUATION_STATE_NAMESPACE,
                &key,
                record.producing_position(),
            )
        } else {
            journal.state_record(EVALUATION_STATE_NAMESPACE, &key)
        }
        .map_err(journal_error)?
        .ok_or_else(|| recovery("legacy evaluation checkpoint page is absent"))?;
        if page.producing_position() != record.producing_position() {
            return Err(recovery("legacy evaluation checkpoint contains a stale state page"));
        }
        lengths.push(page.bytes().len());
        append_page(&mut bytes, page.bytes(), capacity)?;
    }
    let frame = finish_paged_frame(
        record,
        campaign_id,
        sequence,
        state_digest,
        complete_digest,
        capacity,
        bytes,
    )?;
    let page_bytes = usize::try_from(frame.state_page_bytes()).map_err(|_| paging())?;
    validate_lengths(capacity, page_bytes, &lengths)?;
    Ok(frame)
}

fn finish_paged_frame(
    record: &DurableStateRecord,
    campaign_id: EvaluationCampaignId,
    sequence: u64,
    state_digest: Sha256Digest,
    complete_digest: Sha256Digest,
    capacity: usize,
    bytes: Vec<u8>,
) -> Result<EvaluationStateFrame, EvaluationError> {
    if bytes.len() != capacity || peritus_codec::sha256(&bytes) != complete_digest {
        return Err(recovery("paged evaluation checkpoint bytes differ from their root"));
    }
    let frame = decode_message::<EvaluationStateFrame>(&bytes, CodecLimits::PRODUCTION)
        .map_err(codec)?;
    if frame.campaign_id() != campaign_id
        || frame.sequence() != sequence
        || frame.state_digest() != state_digest
        || record.revision() != sequence
    {
        return Err(recovery("paged evaluation checkpoint semantics differ from their root"));
    }
    Ok(frame)
}

fn validate_frame(
    record: &DurableStateRecord,
    campaign_id: EvaluationCampaignId,
    frame: &EvaluationStateFrame,
) -> Result<(), EvaluationError> {
    if frame.campaign_id() != campaign_id || frame.sequence() != record.revision() {
        Err(recovery("evaluation checkpoint identity differs from its state record"))
    } else {
        Ok(())
    }
}

fn validate_root_identity(
    record: &DurableStateRecord,
    campaign_id: EvaluationCampaignId,
    encoded_campaign: EvaluationCampaignId,
    sequence: u64,
    page_count: usize,
) -> Result<(), EvaluationError> {
    if encoded_campaign != campaign_id || sequence != record.revision() || page_count == 0 {
        Err(recovery("paged evaluation checkpoint root identity differs"))
    } else {
        Ok(())
    }
}

fn append_page(
    bytes: &mut Vec<u8>,
    page: &[u8],
    capacity: usize,
) -> Result<(), EvaluationError> {
    let length = bytes.len().checked_add(page.len()).ok_or_else(paging)?;
    if length > capacity {
        return Err(recovery("paged evaluation checkpoint exceeds its declared byte length"));
    }
    bytes.extend_from_slice(page);
    Ok(())
}

fn validate_fixed_boundaries(
    capacity: usize,
    page_bytes: usize,
    bytes: &[u8],
    count: usize,
) -> Result<(), EvaluationError> {
    if bytes.len() != capacity || count != capacity.div_ceil(page_bytes) {
        Err(recovery("paged evaluation checkpoint has noncanonical fixed boundaries"))
    } else {
        Ok(())
    }
}

fn validate_lengths(
    capacity: usize,
    page_bytes: usize,
    lengths: &[usize],
) -> Result<(), EvaluationError> {
    if page_bytes == 0
        || page_bytes > peritus_journal::MAX_STATE_BYTES
        || lengths.len() != capacity.div_ceil(page_bytes)
    {
        return Err(recovery("legacy evaluation checkpoint violates its frozen page size"));
    }
    for (ordinal, length) in lengths.iter().copied().enumerate() {
        let offset = ordinal.checked_mul(page_bytes).ok_or_else(paging)?;
        let expected = capacity.checked_sub(offset).ok_or_else(paging)?.min(page_bytes);
        if length != expected {
            return Err(recovery("legacy checkpoint has a noncanonical page boundary"));
        }
    }
    Ok(())
}

fn content_chunks(bytes: &[u8], maximum: usize) -> Vec<&[u8]> {
    if maximum < 64 {
        return bytes.chunks(maximum).collect();
    }
    let minimum = (maximum / 2).max(1);
    let mask = u64::try_from(minimum.next_power_of_two().saturating_sub(1)).unwrap_or(u64::MAX);
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut hash = 0_u64;
    for (index, byte) in bytes.iter().copied().enumerate() {
        hash = hash.rotate_left(5) ^ u64::from(byte).wrapping_mul(0x9E37_79B1_85EB_CA87);
        let length = index + 1 - start;
        if length == maximum || (length >= minimum && hash & mask == 0) {
            chunks.push(&bytes[start..=index]);
            start = index + 1;
            hash = 0;
        }
    }
    if start < bytes.len() {
        chunks.push(&bytes[start..]);
    }
    chunks
}

fn legacy_page_key(campaign_id: EvaluationCampaignId, ordinal: u64) -> Vec<u8> {
    let mut key =
        Vec::with_capacity(LEGACY_PAGE_KEY_DOMAIN.len() + campaign_id.as_bytes().len() + 8);
    key.extend_from_slice(LEGACY_PAGE_KEY_DOMAIN);
    key.extend_from_slice(campaign_id.as_bytes());
    key.extend_from_slice(&ordinal.to_be_bytes());
    key
}

fn content_page_key(campaign_id: EvaluationCampaignId, digest: Sha256Digest) -> Vec<u8> {
    let mut key = Vec::with_capacity(
        CONTENT_PAGE_KEY_DOMAIN.len() + campaign_id.as_bytes().len() + digest.as_bytes().len(),
    );
    key.extend_from_slice(CONTENT_PAGE_KEY_DOMAIN);
    key.extend_from_slice(campaign_id.as_bytes());
    key.extend_from_slice(digest.as_bytes());
    key
}

fn sequence_page_key(
    campaign_id: EvaluationCampaignId,
    sequence: u64,
    ordinal: u64,
) -> Vec<u8> {
    let mut key = Vec::with_capacity(
        SEQUENCE_PAGE_KEY_DOMAIN.len() + campaign_id.as_bytes().len() + 16,
    );
    key.extend_from_slice(SEQUENCE_PAGE_KEY_DOMAIN);
    key.extend_from_slice(campaign_id.as_bytes());
    key.extend_from_slice(&sequence.to_be_bytes());
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
