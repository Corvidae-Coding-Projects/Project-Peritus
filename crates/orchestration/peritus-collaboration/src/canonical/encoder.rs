//! Minimal canonical byte accumulator used only for collaboration semantic digests.

use peritus_types::{RevisionTuple, Sha256Digest};

pub(super) struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    pub(super) fn new(domain: &[u8]) -> Self {
        Self { bytes: domain.to_vec() }
    }
    pub(super) fn hash(self) -> Sha256Digest {
        peritus_codec::sha256(&self.bytes)
    }
    pub(super) fn raw(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }
    pub(super) fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }
    pub(super) fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }
    pub(super) fn u16(&mut self, value: u16) {
        self.raw(&value.to_be_bytes());
    }
    pub(super) fn u32(&mut self, value: u32) {
        self.raw(&value.to_be_bytes());
    }
    pub(super) fn u64(&mut self, value: u64) {
        self.raw(&value.to_be_bytes());
    }
    pub(super) fn len(&mut self, value: usize) {
        self.u64(u64::try_from(value).unwrap_or(u64::MAX));
    }
    pub(super) fn digest(&mut self, value: Sha256Digest) {
        self.raw(value.as_bytes());
    }
    pub(super) fn text(&mut self, value: &str) {
        self.len(value.len());
        self.raw(value.as_bytes());
    }
    pub(super) fn option<T>(&mut self, value: Option<T>, encode: impl FnOnce(&mut Self, T)) {
        match value {
            Some(value) => {
                self.u8(1);
                encode(self, value);
            }
            None => self.u8(0),
        }
    }
    pub(super) fn revision(&mut self, revision: RevisionTuple) {
        self.raw(revision.acceptance_spec_id().as_bytes());
        self.raw(revision.harness_id().as_bytes());
        self.raw(revision.workspace_id().as_bytes());
        self.u64(revision.workspace_generation().get());
        self.u64(revision.workspace_revision().get());
        self.raw(revision.policy_id().as_bytes());
        self.raw(revision.provider_profile_id().as_bytes());
    }
}
