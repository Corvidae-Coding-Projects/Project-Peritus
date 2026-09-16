//! Bounded content bytes and semantic content classes.

use crate::{ContextError, ContextErrorKind};
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
    /// Logical maximum graph size.
    pub closed spec fn spec_max_nodes(self) -> nat { self.max_nodes as nat }

    /// Logical maximum content byte length.
    pub closed spec fn spec_max_content_bytes(self) -> nat { self.max_content_bytes as nat }

    /// Logical maximum direct dependencies per node.
    pub closed spec fn spec_max_dependencies_per_node(self) -> nat {
        self.max_dependencies_per_node as nat
    }

    /// Logical maximum visible roles per node.
    pub closed spec fn spec_max_visibility_roles(self) -> nat {
        self.max_visibility_roles as nat
    }

    /// Exact constructor admission for all four nonzero bounds.
    pub open spec fn inputs_valid(
        max_nodes: usize,
        max_content_bytes: usize,
        max_dependencies_per_node: usize,
        max_visibility_roles: usize,
    ) -> bool {
        max_nodes > 0
            && max_content_bytes > 0
            && max_dependencies_per_node > 0
            && max_visibility_roles > 0
    }

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
    ) -> (result: Result<Self, ContextError>)
        ensures
            result.is_ok() == Self::inputs_valid(
                max_nodes,
                max_content_bytes,
                max_dependencies_per_node,
                max_visibility_roles,
            ),
            match result {
                Ok(value) => value.spec_max_nodes() == max_nodes
                    && value.spec_max_content_bytes() == max_content_bytes
                    && value.spec_max_dependencies_per_node() == max_dependencies_per_node
                    && value.spec_max_visibility_roles() == max_visibility_roles,
                Err(error) => error.spec_is_plain(ContextErrorKind::InvalidLimit),
            },
    {
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
    pub const fn max_nodes(self) -> (maximum: usize)
        ensures maximum as nat == self.spec_max_nodes(),
    { self.max_nodes }
    /// Maximum content bytes in one node.
    #[must_use]
    pub const fn max_content_bytes(self) -> (maximum: usize)
        ensures maximum as nat == self.spec_max_content_bytes(),
    { self.max_content_bytes }
    /// Maximum direct dependencies in one node.
    #[must_use]
    pub const fn max_dependencies_per_node(self) -> (maximum: usize)
        ensures maximum as nat == self.spec_max_dependencies_per_node(),
    { self.max_dependencies_per_node }
    /// Maximum explicit roles in a node visibility set.
    #[must_use]
    pub const fn max_visibility_roles(self) -> (maximum: usize)
        ensures maximum as nat == self.spec_max_visibility_roles(),
    { self.max_visibility_roles }
}

/// Immutable nonempty bytes whose supplied digest has been checked.
#[derive(Debug, Eq, PartialEq)]
pub struct ContextContent {
    bytes: Vec<u8>,
    digest: Sha256Digest,
}

impl ContextContent {
    #[verifier::type_invariant]
    pub(crate) open spec fn invariant(&self) -> bool { self.spec_well_formed() }

    /// Exact immutable content bytes.
    pub closed spec fn spec_bytes(&self) -> Seq<u8> { self.bytes@ }

    /// Exact caller-supplied digest retained after the external SHA check.
    pub closed spec fn spec_digest(&self) -> Sha256Digest { self.digest }

    /// Intrinsic shape retained independently of the constructor allocation bound.
    pub open spec fn spec_well_formed(&self) -> bool { self.spec_bytes().len() > 0 }

    /// Exact deterministic admission after the external digest comparison succeeds.
    pub open spec fn inputs_valid(bytes: Seq<u8>, limits: ContextLimits) -> bool {
        0 < bytes.len() <= limits.spec_max_content_bytes()
    }

    /// Exact bounds error and precedence after the external digest comparison succeeds.
    pub open spec fn construction_error(
        bytes: Seq<u8>,
        limits: ContextLimits,
        error: ContextError,
    ) -> bool {
        if bytes.len() == 0 {
            error.spec_is_plain(ContextErrorKind::EmptyContent)
        } else {
            bytes.len() > limits.spec_max_content_bytes()
                && error.spec_is_numbers(
                    ContextErrorKind::ContentTooLarge,
                    limits.spec_max_content_bytes() as u64,
                    bytes.len() as u64,
                )
        }
    }

    /// Semantic fields preserved by cloning.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.spec_bytes() == right.spec_bytes() && left.spec_digest() == right.spec_digest()
    }

    pub(crate) fn from_digest_checked(
        bytes: Vec<u8>,
        digest: Sha256Digest,
        limits: ContextLimits,
    ) -> (result: Result<Self, ContextError>)
        ensures
            result.is_ok() == Self::inputs_valid(bytes@, limits),
            match result {
                Ok(value) => value.spec_bytes() == bytes@
                    && value.spec_digest() == digest
                    && value.spec_well_formed(),
                Err(error) => Self::construction_error(bytes@, limits, error),
            },
    {
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
    pub const fn bytes(&self) -> (bytes: &[u8])
        ensures bytes@ == self.spec_bytes(),
    { self.bytes.as_slice() }
    /// Returns the verified digest.
    #[must_use]
    pub const fn digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_digest(),
    { self.digest }
    /// Returns the byte length.
    #[must_use]
    pub const fn len(&self) -> (length: usize)
        ensures length as nat == self.spec_bytes().len(),
    { self.bytes.len() }
    /// Returns false because checked content is always nonempty.
    #[must_use]
    pub const fn is_empty(&self) -> (empty: bool)
        ensures empty == (self.spec_bytes().len() == 0),
    {
        proof { use_type_invariant(self); }
        false
    }
}

impl Clone for ContextContent {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        proof { use_type_invariant(self); }
        Self { bytes: self.bytes.clone(), digest: self.digest }
    }
}

} // verus!
