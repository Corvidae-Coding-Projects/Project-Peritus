//! Bounded content bytes and semantic content classes.

use crate::{ContextError, ContextErrorKind};
use peritus_types::Sha256Digest;

#[doc = " Semantic content kind used for protection and provider-neutral rendering."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ContentKind {
    #[doc = " System policy text."]
    SystemPolicy,
    #[doc = " Application policy text."]
    ApplicationPolicy,
    #[doc = " Immutable acceptance specification."]
    ImmutableSpecification,
    #[doc = " Active user instruction."]
    ActiveUserInstruction,
    #[doc = " A fact describing effective capabilities or authorization."]
    CapabilityFact,
    #[doc = " Repository-local instruction text."]
    RepositoryInstruction,
    #[doc = " Repository source material."]
    RepositorySource,
    #[doc = " Candidate patch or diff."]
    CandidateDiff,
    #[doc = " Workspace state observation."]
    WorkspaceState,
    #[doc = " Gate result or evidence."]
    GateEvidence,
    #[doc = " Tool observation."]
    ToolObservation,
    #[doc = " Derived memory evidence."]
    MemoryEvidence,
    #[doc = " A nonblocking finding."]
    Finding,
    #[doc = " An unresolved blocking finding."]
    UnresolvedBlockingFinding,
    #[doc = " Finding resolution evidence."]
    FindingResolution,
    #[doc = " Agent progress report."]
    AgentProgress,
    #[doc = " Hidden model reasoning."]
    HiddenReasoning,
    #[doc = " Output of validated compaction."]
    DerivedSummary,
}
impl ContentKind {
    #[doc = " Whether this kind is forbidden as a compaction source."]
    #[must_use]
    pub const fn is_compaction_protected(self) -> bool {
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
#[doc = " Explicit allocation and graph bounds for checked context construction."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(clippy::struct_field_names, reason = "public accessor names spell out each maximum")]
pub struct ContextLimits {
    max_nodes: usize,
    max_content_bytes: usize,
    max_dependencies_per_node: usize,
    max_visibility_roles: usize,
}
impl ContextLimits {
    #[doc = " Creates nonzero limits."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`ContextErrorKind::InvalidLimit`] when any bound is zero."]
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
    #[doc = " Maximum nodes in one graph."]
    #[must_use]
    pub const fn max_nodes(self) -> usize {
        self.max_nodes
    }
    #[doc = " Maximum content bytes in one node."]
    #[must_use]
    pub const fn max_content_bytes(self) -> usize {
        self.max_content_bytes
    }
    #[doc = " Maximum direct dependencies in one node."]
    #[must_use]
    pub const fn max_dependencies_per_node(self) -> usize {
        self.max_dependencies_per_node
    }
    #[doc = " Maximum explicit roles in a node visibility set."]
    #[must_use]
    pub const fn max_visibility_roles(self) -> usize {
        self.max_visibility_roles
    }
}
#[doc = " Immutable nonempty bytes whose supplied digest has been checked."]
#[derive(Debug, Eq, PartialEq)]
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
    #[doc = " Borrows the exact immutable content bytes."]
    #[must_use]
    pub const fn bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }
    #[doc = " Returns the verified digest."]
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    #[doc = " Returns the byte length."]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.bytes.len()
    }
    #[doc = " Returns false because checked content is always nonempty."]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }
}
impl Clone for ContextContent {
    fn clone(&self) -> Self {
        Self { bytes: self.bytes.clone(), digest: self.digest }
    }
}
