//! Immutable initialization content references and bounded review pages.

use super::{
    AppProtocolError, InitDiscoveryRequest, InitSourceKind, Sha256Digest, WorkbenchQuery, invalid,
    valid_relative_path,
};

/// Exact immutable content identity; possession grants no read or mutation authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitContentReference {
    digest: Sha256Digest,
    bytes: u64,
}
impl InitContentReference {
    /// Binds complete bytes to their digest.
    #[must_use]
    pub const fn new(digest: Sha256Digest, bytes: u64) -> Self {
        Self { digest, bytes }
    }
    /// Returns the complete content digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
    /// Returns the complete byte count.
    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.bytes
    }
}

/// One explicit additional source, confined to the selected workspace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitSourceSelection {
    path: String,
    kind: InitSourceKind,
}
impl InitSourceSelection {
    /// Validates a canonical source path. No source is executed.
    /// # Errors
    /// Rejects unsafe or noncanonical relative paths.
    pub fn new(path: String, kind: InitSourceKind) -> Result<Self, AppProtocolError> {
        if !valid_relative_path(&path) {
            return Err(invalid());
        }
        Ok(Self { path, kind })
    }
    /// Borrows the selected relative path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
    /// Returns its requested interpretation.
    #[must_use]
    pub const fn kind(&self) -> InitSourceKind {
        self.kind
    }
}

/// One incremental discovery/selection step; prior selections live in the exact manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitArtifactDiscovery {
    request: InitDiscoveryRequest,
    previous: Option<InitContentReference>,
    source: Option<InitSourceSelection>,
    command: Option<u64>,
}
impl InitArtifactDiscovery {
    /// Starts default discovery or extends an exact prior manifest with one source or toggle.
    /// # Errors
    /// Command toggles require a prior manifest; one request performs one selection step.
    pub fn new(
        request: InitDiscoveryRequest,
        previous: Option<InitContentReference>,
        source: Option<InitSourceSelection>,
        command: Option<u64>,
    ) -> Result<Self, AppProtocolError> {
        if command.is_some() && (previous.is_none() || source.is_some()) {
            return Err(invalid());
        }
        Ok(Self { request, previous, source, command })
    }
    /// Returns the revision-bound discovery scope.
    #[must_use]
    pub const fn request(&self) -> InitDiscoveryRequest {
        self.request
    }
    /// Returns the exact previous immutable selection manifest.
    #[must_use]
    pub const fn previous(&self) -> Option<InitContentReference> {
        self.previous
    }
    /// Borrows the one added source selection.
    #[must_use]
    pub const fn source(&self) -> Option<&InitSourceSelection> {
        self.source.as_ref()
    }
    /// Returns the zero-based command ordinal to toggle in the previous manifest.
    #[must_use]
    pub const fn command(&self) -> Option<u64> {
        self.command
    }
}

/// Constant-size approval identity for a complete immutable initialization proposal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitArtifactProposal {
    request: InitDiscoveryRequest,
    manifest: InitContentReference,
    review: InitContentReference,
}
impl InitArtifactProposal {
    /// Binds the complete manifest and its exact human-readable review bytes.
    #[must_use]
    pub const fn new(
        request: InitDiscoveryRequest,
        manifest: InitContentReference,
        review: InitContentReference,
    ) -> Self {
        Self { request, manifest, review }
    }
    /// Returns the scoped inspected revision.
    #[must_use]
    pub const fn request(self) -> InitDiscoveryRequest {
        self.request
    }
    /// Returns the complete manifest identity used for consent and recovery.
    #[must_use]
    pub const fn manifest(self) -> InitContentReference {
        self.manifest
    }
    /// Returns the exact review artifact, including sources, diagnostics, commands and diff.
    #[must_use]
    pub const fn review(self) -> InitContentReference {
        self.review
    }
    /// Returns the selected conversation and workspace.
    #[must_use]
    pub const fn query(self) -> WorkbenchQuery {
        self.request.query()
    }
}

/// One bounded byte page of the exact review artifact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitArtifactPageRequest {
    proposal: InitArtifactProposal,
    offset: u64,
    maximum: u32,
}
impl InitArtifactPageRequest {
    /// Selects one byte page; the limit is transport-only, never a whole-review ceiling.
    /// # Errors
    /// Rejects an absent offset or zero/oversized transport page.
    pub const fn new(
        proposal: InitArtifactProposal,
        offset: u64,
        maximum: u32,
    ) -> Result<Self, AppProtocolError> {
        if offset > proposal.review.bytes || maximum == 0 || maximum > 64 * 1024 {
            return Err(invalid());
        }
        Ok(Self { proposal, offset, maximum })
    }
    /// Returns the complete approval identity.
    #[must_use]
    pub const fn proposal(self) -> InitArtifactProposal {
        self.proposal
    }
    /// Returns the selected exact byte offset.
    #[must_use]
    pub const fn offset(self) -> u64 {
        self.offset
    }
    /// Returns the maximum transport bytes.
    #[must_use]
    pub const fn maximum(self) -> u32 {
        self.maximum
    }
}

/// Exact contiguous bytes from an immutable review, including its complete binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitArtifactPage {
    request: InitArtifactPageRequest,
    bytes: Vec<u8>,
}
impl InitArtifactPage {
    /// Validates exact page bounds and completion shape.
    /// # Errors
    /// Rejects gaps, short nonterminal pages, or bytes outside the selected immutable artifact.
    pub fn new(request: InitArtifactPageRequest, bytes: Vec<u8>) -> Result<Self, AppProtocolError> {
        let remaining = request.proposal.review.bytes - request.offset;
        if bytes.len() as u64 != remaining.min(u64::from(request.maximum)) {
            return Err(invalid());
        }
        Ok(Self { request, bytes })
    }
    /// Returns the exact page request binding.
    #[must_use]
    pub const fn request(&self) -> InitArtifactPageRequest {
        self.request
    }
    /// Borrows the original bytes; transport does not change UTF-8 boundaries.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Returns the next offset, or `None` at exact completion.
    #[must_use]
    pub fn next(&self) -> Option<u64> {
        let next = self.request.offset + self.bytes.len() as u64;
        (next < self.request.proposal.review.bytes).then_some(next)
    }
}
