//! Small deterministic binary encoding helpers for projection payloads.

#![allow(
    clippy::redundant_pub_crate,
    reason = "the private module deliberately shares encoding helpers with sibling folds"
)]

use peritus_journal::{AggregateKey, AggregateKind};
use peritus_types::Sha256Digest;

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
    }
}
