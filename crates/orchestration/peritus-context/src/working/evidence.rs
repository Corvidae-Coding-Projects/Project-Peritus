//! Stable locators for exact locally archived observations.

use super::WorkingError;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {
/// Positive sequence in one role-scoped observation archive; displayed as `obs:NNNNNN` by hosts.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ObservationId(u64);

impl ObservationId {
    /// Creates a positive stable source handle.
    ///
    /// # Errors
    /// Rejects the reserved zero value.
    pub const fn new(value: u64) -> Result<Self, WorkingError> {
        if value == 0 { Err(WorkingError::ZeroSequence) } else { Ok(Self(value)) }
    }
    /// Exact sequence within the bound archive.
    #[must_use]
    pub const fn get(self) -> u64 { self.0 }
}

/// Origin supplied by the host, never inferred from instruction-like source text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationKind {
    /// Current system or application policy.
    HostPolicy,
    /// Literal user requirement or correction.
    UserInstruction,
    /// Completed authorized tool output; its claims are not automatically true.
    ToolOutput,
    /// Visible assistant text or unexecuted proposals.
    AgentMessage,
}

/// An exact byte range of an already persisted and verified artifact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservationSource {
    id: ObservationId,
    artifact: Sha256Digest,
    artifact_bytes: u64,
    start: u64,
    end: u64,
    kind: ObservationKind,
}

impl ObservationSource {
    /// Creates a checked source range; the host must verify artifact existence and digest first.
    ///
    /// # Errors
    /// Rejects reversed or out-of-artifact ranges. Empty output is represented by an empty range.
    pub const fn new(
        id: ObservationId,
        artifact: Sha256Digest,
        artifact_bytes: u64,
        start: u64,
        end: u64,
        kind: ObservationKind,
    ) -> Result<Self, WorkingError> {
        if !source_range_valid(start, end, artifact_bytes) {
            return Err(WorkingError::SourceRange);
        }
        Ok(Self { id, artifact, artifact_bytes, start, end, kind })
    }
    /// Stable handle within the containing state's binding.
    #[must_use]
    pub const fn id(self) -> ObservationId { self.id }
    /// Exact artifact content digest.
    #[must_use]
    pub const fn artifact(self) -> Sha256Digest { self.artifact }
    /// Verified complete artifact byte count.
    #[must_use]
    pub const fn artifact_bytes(self) -> u64 { self.artifact_bytes }
    /// Inclusive byte offset.
    #[must_use]
    pub const fn start(self) -> u64 { self.start }
    /// Exclusive byte offset.
    #[must_use]
    pub const fn end(self) -> u64 { self.end }
    /// Host-recorded source origin.
    #[must_use]
    pub const fn kind(self) -> ObservationKind { self.kind }
}

pub(super) const fn source_range_valid(start: u64, end: u64, bytes: u64) -> (valid: bool)
    ensures valid == (start <= end && end <= bytes),
{
    start <= end && end <= bytes
}
}
