//! Bounded content bytes and semantic content classes.

use crate::{ContextError, ContextErrorKind};
#[cfg(not(verus_only))]
use peritus_codec::sha256;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Semantic content kind used for protection and provider-neutral rendering.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ContentKind {
    /// System policy text.
    SystemPolicy,
    /// Application policy text.
    ApplicationPolicy,
    /// Immutable acceptance specification.
    ImmutableSpecification,
    /// Active user instruction.
    ActiveUserInstruction,
    /// A fact describing effective capabilities or authorization.
    CapabilityFact,
    /// Repository-local instruction text.
    RepositoryInstruction,
    /// Repository source material.
    RepositorySource,
    /// Candidate patch or diff.
    CandidateDiff,
    /// Workspace state observation.
    WorkspaceState,
    /// Gate result or evidence.
    GateEvidence,
    /// Tool observation.
    ToolObservation,
    /// Derived memory evidence.
    MemoryEvidence,
    /// A nonblocking finding.
    Finding,
    /// An unresolved blocking finding.
    UnresolvedBlockingFinding,
    /// Finding resolution evidence.
    FindingResolution,
    /// Agent progress report.
    AgentProgress,
    /// Hidden model reasoning.
    HiddenReasoning,
    /// Output of validated compaction.
    DerivedSummary,
}

impl ContentKind {
    /// Mathematical classification of content that compaction may never replace.
    pub open spec fn spec_is_compaction_protected(self) -> bool {
        matches!(
            self,
            Self::SystemPolicy
                | Self::ApplicationPolicy
                | Self::ImmutableSpecification
                | Self::ActiveUserInstruction
                | Self::CapabilityFact
                | Self::UnresolvedBlockingFinding
        )
    }

    /// Whether this kind is forbidden as a compaction source.
    #[must_use]
    pub const fn is_compaction_protected(self) -> (result: bool)
        ensures result == self.spec_is_compaction_protected(),
    {
        matches!(
            self,
            Self::SystemPolicy
                | Self::ApplicationPolicy
                | Self::ImmutableSpecification
                | Self::ActiveUserInstruction
                | Self::CapabilityFact
                | Self::UnresolvedBlockingFinding
        )
    }
}

/// Explicit allocation and graph bounds for checked context construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(clippy::struct_field_names, reason = "public accessor names spell out each maximum")]
pub struct ContextLimits {
    max_nodes: usize,
    max_content_bytes: usize,
    max_dependencies_per_node: usize,
    max_visibility_roles: usize,
}

impl ContextLimits {
    /// Creates nonzero limits.
    ///
    /// # Errors
    ///
    /// Returns [`ContextErrorKind::InvalidLimit`] when any bound is zero.
    pub const fn new(
        max_nodes: usize,
        max_content_bytes: usize,
        max_dependencies_per_node: usize,
        max_visibility_roles: usize,
    ) -> Result<Self, ContextError> {
        if max_nodes == 0
            || max_content_bytes == 0
            || max_dependencies_per_node == 0
            || max_visibility_roles == 0
        {
            Err(ContextError::plain(ContextErrorKind::InvalidLimit))
        } else {
            Ok(Self {
                max_nodes,
                max_content_bytes,
                max_dependencies_per_node,
                max_visibility_roles,
            })
        }
    }

    /// Maximum nodes in one graph.
    #[must_use]
    pub const fn max_nodes(self) -> usize { self.max_nodes }
    /// Maximum content bytes in one node.
    #[must_use]
    pub const fn max_content_bytes(self) -> usize { self.max_content_bytes }
    /// Maximum direct dependencies in one node.
    #[must_use]
    pub const fn max_dependencies_per_node(self) -> usize { self.max_dependencies_per_node }
    /// Maximum explicit roles in a node visibility set.
    #[must_use]
    pub const fn max_visibility_roles(self) -> usize { self.max_visibility_roles }
}

/// Immutable nonempty bytes whose supplied digest has been checked.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextContent {
    bytes: Vec<u8>,
    digest: Sha256Digest,
}

impl ContextContent {
    pub(crate) fn from_digest_checked(
        bytes: Vec<u8>,
        digest: Sha256Digest,
        limits: ContextLimits,
    ) -> Result<Self, ContextError> {
        if bytes.is_empty() {
            return Err(ContextError::plain(ContextErrorKind::EmptyContent));
        }
        if bytes.len() > limits.max_content_bytes {
            return Err(ContextError::with_numbers(
                ContextErrorKind::ContentTooLarge,
                limits.max_content_bytes as u64,
                bytes.len() as u64,
            ));
        }
        Ok(Self { bytes, digest })
    }

    /// Borrows the exact immutable content bytes.
    #[must_use]
    pub const fn bytes(&self) -> &[u8] { self.bytes.as_slice() }
    /// Returns the verified digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest { self.digest }
    /// Returns the byte length.
    #[must_use]
    pub const fn len(&self) -> usize { self.bytes.len() }
    /// Returns false because checked content is always nonempty.
    #[must_use]
    pub const fn is_empty(&self) -> bool { false }
}

} // verus!

/// Validates content bounds and its exact SHA-256 digest.
///
/// SHA-256 is the crate's audited H-class boundary; all bounds and metadata validation remain in
/// Verus code. This is the only public way to construct [`ContextContent`].
///
/// # Errors
///
/// Returns a typed error for empty, oversized, or digest-mismatched content.
#[cfg(not(verus_only))]
pub fn bind_context_content(
    bytes: Vec<u8>,
    digest: Sha256Digest,
    limits: ContextLimits,
) -> Result<ContextContent, ContextError> {
    if sha256(bytes.as_slice()) != digest {
        return Err(ContextError::plain(ContextErrorKind::DigestMismatch));
    }
    ContextContent::from_digest_checked(bytes, digest, limits)
}
