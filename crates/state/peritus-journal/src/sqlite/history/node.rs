//! Canonical fixed-leaf, fixed-fanout node encoding. Logical state hashes remain unchanged.

use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use crate::{JournalError, record::MAX_STATE_BYTES};

use super::corrupt;

pub(super) const LEAF_BYTES: usize = 512;
pub(super) const FANOUT: usize = 16;
const MAX_LEVEL: u8 = 4;

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
        if byte_length > MAX_STATE_BYTES || byte_length > capacity(level)? {
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
    if level > MAX_LEVEL {
        return Err(corrupt("history node exceeds maximum tree height"));
    }
    Ok(LEAF_BYTES * FANOUT.pow(u32::from(level)))
}

pub(super) fn root_level(length: usize) -> Result<u8, JournalError> {
    if length > MAX_STATE_BYTES {
        return Err(corrupt("history value exceeds maximum logical state bytes"));
    }
    let mut level = 0;
    while length > capacity(level)? {
        level += 1;
    }
    Ok(level)
}
