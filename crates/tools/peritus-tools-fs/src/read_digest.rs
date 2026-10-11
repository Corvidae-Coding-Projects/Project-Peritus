//! Streaming digests over complete filesystem observations.

use peritus_types::Sha256Digest;
use peritus_workspace::WorkspaceEntryKind;
use sha2::{Digest as _, Sha256};

use crate::{MetadataObservation, ScopeOmission, SearchMatch};

/// Incremental digest builder for deterministic complete discovery observations.
pub struct DiscoverDigestBuilder(Sha256);

impl DiscoverDigestBuilder {
    /// Starts a versioned discovery digest bound to the requested root.
    #[must_use]
    pub fn new(root: Option<&peritus_patch::WorkspacePath>) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"PERITUS-FS-DISCOVER-V4\0");
        put_bytes(&mut hasher, root.map_or("", peritus_patch::WorkspacePath::as_str));
        Self(hasher)
    }

    /// Adds one entry in canonical traversal order.
    pub fn entry(&mut self, metadata: &MetadataObservation, depth: u16) {
        self.0.update(*b"E");
        put_metadata(&mut self.0, metadata);
        self.0.update(depth.to_be_bytes());
    }

    /// Adds one associated scope omission in canonical traversal order.
    pub fn omission(&mut self, value: &ScopeOmission) {
        self.0.update(*b"O");
        put_omission(&mut self.0, value);
    }

    /// Finishes the digest with exact complete counts.
    #[must_use]
    pub fn finish(mut self, entries: u64, omissions: u64) -> Sha256Digest {
        self.0.update(entries.to_be_bytes());
        self.0.update(omissions.to_be_bytes());
        Sha256Digest::new(self.0.finalize().into())
    }
}

/// Incremental digest builder for a search page and the complete search omission set.
pub struct SearchDigestBuilder(Sha256);

impl SearchDigestBuilder {
    /// Starts a versioned search digest bound to the match continuation.
    #[must_use]
    pub fn new(match_page_start: u64, omission_page_start: u64) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"PERITUS-FS-SEARCH-V4\0");
        hasher.update(match_page_start.to_be_bytes());
        hasher.update(omission_page_start.to_be_bytes());
        Self(hasher)
    }

    /// Adds all matches from one validated UTF-8 file without retaining them.
    pub fn match_group(
        &mut self,
        path: &peritus_patch::WorkspacePath,
        count: u64,
        digest: Sha256Digest,
    ) {
        self.0.update(*b"G");
        put_bytes(&mut self.0, path.as_str());
        self.0.update(count.to_be_bytes());
        self.0.update(digest.as_bytes());
    }

    /// Adds one skipped/depth omission in canonical traversal order.
    pub fn omission(&mut self, value: &ScopeOmission) {
        self.0.update(*b"O");
        put_omission(&mut self.0, value);
    }

    /// Finishes the digest with exact scan and page continuation metadata.
    #[must_use]
    pub fn finish(
        mut self,
        scanned_files: u64,
        scanned_bytes: u64,
        next_match: Option<u64>,
        matches: u64,
        omissions: u64,
        next_omission: Option<u64>,
    ) -> Sha256Digest {
        self.0.update(scanned_files.to_be_bytes());
        self.0.update(scanned_bytes.to_be_bytes());
        put_option_u64(&mut self.0, next_match);
        self.0.update(matches.to_be_bytes());
        self.0.update(omissions.to_be_bytes());
        put_option_u64(&mut self.0, next_omission);
        Sha256Digest::new(self.0.finalize().into())
    }
}

pub fn update_match(hasher: &mut Sha256, value: &SearchMatch) {
    put_bytes(hasher, value.path().as_str());
    hasher.update(value.line().to_be_bytes());
    hasher.update(value.column_bytes().to_be_bytes());
    put_bytes(hasher, value.preview());
}

fn put_metadata(hasher: &mut Sha256, value: &MetadataObservation) {
    put_bytes(hasher, value.path().as_str());
    hasher.update([match value.kind() {
        WorkspaceEntryKind::File => 1,
        WorkspaceEntryKind::Directory => 2,
        WorkspaceEntryKind::Other => 3,
    }]);
    hasher.update(value.size().to_be_bytes());
    hasher.update([u8::from(value.executable())]);
}

fn put_omission(hasher: &mut Sha256, omission: &ScopeOmission) {
    put_bytes_raw(hasher, omission.native_path_bytes());
    hasher.update([match omission.reason() {
        crate::OmissionReason::DepthLimit => 1,
        crate::OmissionReason::UnsafeEntry => 2,
        crate::OmissionReason::FileByteLimit => 3,
        crate::OmissionReason::BinaryContent => 4,
        crate::OmissionReason::UnsupportedName => 5,
        crate::OmissionReason::UnsupportedType => 6,
    }]);
}

fn put_bytes(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}

fn put_option_u64(hasher: &mut Sha256, value: Option<u64>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hasher.update(value.to_be_bytes());
        }
        None => hasher.update([0]),
    }
}

fn put_bytes_raw(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}
