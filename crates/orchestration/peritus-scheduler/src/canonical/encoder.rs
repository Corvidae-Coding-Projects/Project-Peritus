use peritus_types::{RevisionTuple, Sha256Digest};
use sha2::{Digest, Sha256};

pub(super) struct Encoder(Sha256);

impl Encoder {
    pub(super) fn new(domain: &[u8]) -> Self {
        let mut hash = Sha256::new();
        hash.update(domain);
        Self(hash)
    }
    pub(super) fn raw(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
    pub(super) fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }
    pub(super) fn u8(&mut self, value: u8) {
        self.raw(&[value]);
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
    pub(super) fn option<T>(&mut self, value: Option<&T>, encode: impl FnOnce(&mut Self, &T)) {
        self.bool(value.is_some());
        if let Some(value) = value {
            encode(self, value);
        }
    }
    pub(super) fn revision(&mut self, value: RevisionTuple) {
        self.raw(value.acceptance_spec_id().as_bytes());
        self.raw(value.harness_id().as_bytes());
        self.raw(value.workspace_id().as_bytes());
        self.u64(value.workspace_generation().get());
        self.u64(value.workspace_revision().get());
        self.raw(value.policy_id().as_bytes());
        self.raw(value.provider_profile_id().as_bytes());
    }
    pub(super) fn hash(self) -> Sha256Digest {
        Sha256Digest::new(self.0.finalize().into())
    }
}
