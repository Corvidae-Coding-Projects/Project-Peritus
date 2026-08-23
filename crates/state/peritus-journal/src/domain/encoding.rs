//! Small canonical encoder for journal-owned transition state values.

use peritus_policy::{AuthorityInstant, UseLimit};
use peritus_types::{RevisionTuple, Sha256Digest};

pub(super) const FORMAT_VERSION: u16 = 1;

pub(super) fn value(kind: u16, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(32 + payload.len());
    bytes.extend_from_slice(b"PERITUS-C0-STATE\0");
    u16_value(&mut bytes, FORMAT_VERSION);
    u16_value(&mut bytes, kind);
    bytes_value(&mut bytes, payload);
    bytes
}

pub(super) fn payload(value: &[u8], expected_kind: u16) -> Option<&[u8]> {
    const PREFIX: &[u8] = b"PERITUS-C0-STATE\0";
    let header = PREFIX.len().checked_add(12)?;
    if value.len() < header || &value[..PREFIX.len()] != PREFIX {
        return None;
    }
    let version = u16::from_be_bytes(value[PREFIX.len()..PREFIX.len() + 2].try_into().ok()?);
    let kind = u16::from_be_bytes(value[PREFIX.len() + 2..PREFIX.len() + 4].try_into().ok()?);
    let length = u64::from_be_bytes(value[PREFIX.len() + 4..header].try_into().ok()?);
    let length = usize::try_from(length).ok()?;
    if version != FORMAT_VERSION || kind != expected_kind || value.len() - header != length {
        return None;
    }
    Some(&value[header..])
}

pub(super) fn u8_value(bytes: &mut Vec<u8>, value: u8) {
    bytes.push(value);
}

pub(super) fn u16_value(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

pub(super) fn u64_value(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

pub(super) fn bytes_value(bytes: &mut Vec<u8>, value: &[u8]) {
    u64_value(bytes, value.len() as u64);
    bytes.extend_from_slice(value);
}

pub(super) fn digest(bytes: &mut Vec<u8>, value: Sha256Digest) {
    bytes.extend_from_slice(value.as_bytes());
}

pub(super) fn optional_u64(bytes: &mut Vec<u8>, value: Option<u64>) {
    match value {
        Some(value) => {
            u8_value(bytes, 1);
            u64_value(bytes, value);
        }
        None => u8_value(bytes, 0),
    }
}

pub(super) fn optional_digest(bytes: &mut Vec<u8>, value: Option<Sha256Digest>) {
    match value {
        Some(value) => {
            u8_value(bytes, 1);
            digest(bytes, value);
        }
        None => u8_value(bytes, 0),
    }
}

pub(super) fn use_limit(bytes: &mut Vec<u8>, value: UseLimit) {
    optional_u64(bytes, value.remaining());
}

pub(super) fn instant(bytes: &mut Vec<u8>, value: AuthorityInstant) {
    u64_value(bytes, value.epoch().get());
    u64_value(bytes, value.tick_millis());
}

pub(super) fn revision(bytes: &mut Vec<u8>, value: RevisionTuple) {
    bytes.extend_from_slice(value.acceptance_spec_id().as_bytes());
    bytes.extend_from_slice(value.harness_id().as_bytes());
    bytes.extend_from_slice(value.workspace_id().as_bytes());
    u64_value(bytes, value.workspace_generation().get());
    u64_value(bytes, value.workspace_revision().get());
    bytes.extend_from_slice(value.policy_id().as_bytes());
    bytes.extend_from_slice(value.provider_profile_id().as_bytes());
}
