//! Small deterministic envelope encoding primitives.

use peritus_policy::AuthorityInstant;
use peritus_types::RevisionTuple;

pub fn begin(family: u16) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(256);
    bytes.extend_from_slice(b"PTL1");
    u16_value(&mut bytes, family);
    u16_value(&mut bytes, 1);
    bytes
}

pub fn bytes(target: &mut Vec<u8>, value: &[u8]) {
    u64_value(target, value.len() as u64);
    target.extend_from_slice(value);
}

pub fn text(target: &mut Vec<u8>, value: &str) {
    bytes(target, value.as_bytes());
}
pub fn u16_value(target: &mut Vec<u8>, value: u16) {
    target.extend_from_slice(&value.to_be_bytes());
}
pub fn u32_value(target: &mut Vec<u8>, value: u32) {
    target.extend_from_slice(&value.to_be_bytes());
}
pub fn u64_value(target: &mut Vec<u8>, value: u64) {
    target.extend_from_slice(&value.to_be_bytes());
}

pub fn instant(target: &mut Vec<u8>, value: AuthorityInstant) {
    u64_value(target, value.epoch().get());
    u64_value(target, value.tick_millis());
}

pub fn revision(target: &mut Vec<u8>, value: RevisionTuple) {
    target.extend_from_slice(value.acceptance_spec_id().as_bytes());
    target.extend_from_slice(value.harness_id().as_bytes());
    target.extend_from_slice(value.workspace_id().as_bytes());
    u64_value(target, value.workspace_generation().get());
    u64_value(target, value.workspace_revision().get());
    target.extend_from_slice(value.policy_id().as_bytes());
    target.extend_from_slice(value.provider_profile_id().as_bytes());
}
