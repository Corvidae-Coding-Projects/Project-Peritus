//! Small deterministic binary encoding helpers for projection payloads.

#![allow(
    clippy::redundant_pub_crate,
    reason = "the private module deliberately shares encoding helpers with sibling folds"
)]

use crate::{ProjectionError, ProjectionErrorKind, RecoveryClass};
use peritus_journal::{AggregateId, AggregateKey, AggregateKind};
use peritus_types::Sha256Digest;

pub(super) struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    pub(super) fn new(bytes: &'a [u8], prefix: &[u8]) -> Result<Self, ProjectionError> {
        if !bytes.starts_with(prefix) {
            return Err(decode_error("projection payload prefix is invalid"));
        }
        Ok(Self { bytes, offset: prefix.len() })
    }

    pub(super) fn u16(&mut self) -> Result<u16, ProjectionError> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    pub(super) fn u64(&mut self) -> Result<u64, ProjectionError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    pub(super) fn digest(&mut self) -> Result<Sha256Digest, ProjectionError> {
        Ok(Sha256Digest::new(self.array()?))
    }

    pub(super) fn event_id_bytes(&mut self) -> Result<[u8; 16], ProjectionError> {
        self.array()
    }

    pub(super) fn key(&mut self) -> Result<AggregateKey, ProjectionError> {
        let kind = kind_from_tag(self.u16()?)
            .ok_or_else(|| decode_error("projection aggregate kind is invalid"))?;
        let id = AggregateId::new(self.array()?)
            .map_err(|_| decode_error("projection aggregate identity is invalid"))?;
        Ok(AggregateKey::new(kind, id))
    }

    pub(super) fn count(&mut self, minimum_bytes: usize) -> Result<usize, ProjectionError> {
        let count = usize::try_from(self.u64()?)
            .map_err(|_| decode_error("projection item count overflows"))?;
        if minimum_bytes == 0 || count > self.remaining() / minimum_bytes {
            return Err(decode_error("projection item count exceeds its payload"));
        }
        Ok(count)
    }

    pub(super) fn finish(self) -> Result<(), ProjectionError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(decode_error("projection payload has trailing bytes"))
        }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], ProjectionError> {
        let end = self
            .offset
            .checked_add(N)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| decode_error("projection payload is truncated"))?;
        let value = self.bytes[self.offset..end]
            .try_into()
            .map_err(|_| decode_error("projection field has an invalid length"))?;
        self.offset = end;
        Ok(value)
    }
}

pub(super) fn decode_error(detail: &'static str) -> ProjectionError {
    ProjectionError::new(
        ProjectionErrorKind::CorruptCatalog,
        RecoveryClass::Rebuild,
        "decode projection checkpoint",
        detail,
    )
}

pub(super) fn put_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

pub(super) fn put_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

pub(super) fn put_digest(bytes: &mut Vec<u8>, value: Sha256Digest) {
    bytes.extend_from_slice(value.as_bytes());
}

pub(super) fn put_key(bytes: &mut Vec<u8>, key: AggregateKey) {
    put_u16(bytes, kind_tag(key.kind()));
    bytes.extend_from_slice(key.id().as_bytes());
}

pub(super) const fn kind_tag(kind: AggregateKind) -> u16 {
    match kind {
        AggregateKind::Kernel => 1,
        AggregateKind::Budget => 2,
        AggregateKind::Lease => 3,
        AggregateKind::Approval => 4,
        AggregateKind::CredentialRegistry => 5,
        AggregateKind::Agent => 6,
        AggregateKind::Gate => 7,
        AggregateKind::Trace => 8,
        AggregateKind::Review => 9,
        AggregateKind::Scheduler => 10,
        AggregateKind::Collaboration => 11,
        AggregateKind::Orchestrator => 12,
        AggregateKind::Harness => 13,
        AggregateKind::Debugger => 14,
        AggregateKind::Evaluation => 15,
        AggregateKind::EvolutionCampaign => 16,
        AggregateKind::ProductionHarness => 17,
        AggregateKind::Application => 18,
    }
}

const fn kind_from_tag(tag: u16) -> Option<AggregateKind> {
    match tag {
        1 => Some(AggregateKind::Kernel),
        2 => Some(AggregateKind::Budget),
        3 => Some(AggregateKind::Lease),
        4 => Some(AggregateKind::Approval),
        5 => Some(AggregateKind::CredentialRegistry),
        6 => Some(AggregateKind::Agent),
        7 => Some(AggregateKind::Gate),
        8 => Some(AggregateKind::Trace),
        9 => Some(AggregateKind::Review),
        10 => Some(AggregateKind::Scheduler),
        11 => Some(AggregateKind::Collaboration),
        12 => Some(AggregateKind::Orchestrator),
        13 => Some(AggregateKind::Harness),
        14 => Some(AggregateKind::Debugger),
        15 => Some(AggregateKind::Evaluation),
        16 => Some(AggregateKind::EvolutionCampaign),
        17 => Some(AggregateKind::ProductionHarness),
        18 => Some(AggregateKind::Application),
        _ => None,
    }
}
