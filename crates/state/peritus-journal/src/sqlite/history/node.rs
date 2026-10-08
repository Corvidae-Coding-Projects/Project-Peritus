//! Canonical fixed-leaf, fixed-fanout node encoding. Logical state hashes remain unchanged.

use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use crate::JournalError;

use super::corrupt;

pub(super) const LEAF_BYTES: usize = 512;
pub(super) const FANOUT: usize = 16;
#[derive(Debug, Eq, PartialEq)]
pub(super) struct Node {
    level: u8,
    byte_length: usize,
    payload: Vec<u8>,
}

impl Node {
    pub(super) fn new(
        level: u8,
        byte_length: usize,
        payload: Vec<u8>,
    ) -> Result<Self, JournalError> {
        if byte_length > capacity(level)? {
            return Err(corrupt("history node length exceeds its logical bound"));
        }
        let expected_payload = if level == 0 {
            byte_length
        } else {
            if byte_length == 0 {
                return Err(corrupt("empty history value must be a leaf"));
            }
            byte_length.div_ceil(capacity(level - 1)?) * 32
        };
        if payload.len() != expected_payload {
            return Err(corrupt("history node payload does not have canonical length"));
        }
        Ok(Self { level, byte_length, payload })
    }

    pub(super) fn digest(&self) -> Sha256Digest {
        let mut hasher = Sha256::new();
        hasher.update(b"peritus/journal-state-history-node/v1\0");
        hasher.update([self.level]);
        hasher.update((self.byte_length as u64).to_be_bytes());
        hasher.update(&self.payload);
        Sha256Digest::new(hasher.finalize().into())
    }

    pub(super) const fn level(&self) -> u8 {
        self.level
    }
    pub(super) const fn byte_length(&self) -> usize {
        self.byte_length
    }
    pub(super) fn payload(&self) -> &[u8] {
        &self.payload
    }
}

pub(super) fn capacity(level: u8) -> Result<usize, JournalError> {
    let maximum_level = native_max_level();
    if level > maximum_level {
        return Err(corrupt("history node exceeds maximum tree height"));
    }
    let mut capacity = LEAF_BYTES;
    for _ in 0..level {
        capacity = capacity.saturating_mul(FANOUT);
    }
    Ok(capacity)
}

pub(super) fn root_level(length: usize) -> Result<u8, JournalError> {
    let maximum_level = native_max_level();
    for level in 0..=maximum_level {
        if length <= capacity(level)? {
            return Ok(level);
        }
    }
    Err(corrupt("history value length cannot be represented"))
}

const fn native_max_level() -> u8 {
    let mut level = 0_u8;
    let mut capacity = LEAF_BYTES;
    while capacity.checked_mul(FANOUT).is_some() {
        capacity *= FANOUT;
        level += 1;
    }
    level.saturating_add(1)
}
