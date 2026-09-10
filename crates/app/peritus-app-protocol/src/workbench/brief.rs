//! User-confirmed brief projection with exact immutable input provenance.

use crate::{
    AppErrorCode, AppProtocolError, ControlOperationId, WorkbenchInputRow, WorkbenchInputState,
    WorkbenchInputText, WorkbenchInvocationId, WorkbenchQuery,
};
use peritus_types::Sha256Digest;

#[cfg(test)]
mod tests;

/// Maximum explicit fields in one task brief.
pub const MAX_WORKBENCH_BRIEF_FIELDS: usize = 4;
/// Maximum exact agent reply candidates shown without silent truncation.
pub const MAX_WORKBENCH_BRIEF_PROPOSALS: usize = 8;
/// Maximum observed attachment facts shown in one brief projection.
pub const MAX_WORKBENCH_BRIEF_OBSERVATIONS: usize = 32;

const fn invalid() -> AppProtocolError {
    AppProtocolError::new(AppErrorCode::MalformedFrame, None)
}

/// Closed, canonically ordered user-confirmed brief fields.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum WorkbenchBriefField {
    /// The requested outcome.
    Objective,
    /// User-approved completion criteria.
    Acceptance,
    /// User-imposed work constraints, not additional authority.
    Constraints,
    /// Explicitly confirmed assumptions, never unaccepted model suggestions.
    Assumptions,
}

/// One field and its exact user-input source; lifecycle determines eligibility.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchBriefEntry {
    field: WorkbenchBriefField,
    source: WorkbenchInputRow,
}

/// Exact immutable agent reply that may be explicitly accepted into one brief field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchBriefProposal {
    operation: ControlOperationId,
    invocation: WorkbenchInvocationId,
    digest: Sha256Digest,
    text: WorkbenchInputText,
}
impl WorkbenchBriefProposal {
    /// Validates that the bounded public text matches the retained reply digest.
    ///
    /// # Errors
    /// Rejects a mismatched digest.
    pub fn new(
        operation: ControlOperationId,
        invocation: WorkbenchInvocationId,
        digest: Sha256Digest,
        text: WorkbenchInputText,
    ) -> Result<Self, AppProtocolError> {
        if peritus_codec::sha256(text.as_str().as_bytes()) != digest {
            return Err(invalid());
        }
        Ok(Self { operation, invocation, digest, text })
    }
    /// Returns the immutable reply publication operation.
    #[must_use]
    pub const fn operation(&self) -> ControlOperationId {
        self.operation
    }
    /// Returns the invocation immediately preceding the public reply.
    #[must_use]
    pub const fn invocation(&self) -> WorkbenchInvocationId {
        self.invocation
    }
    /// Returns the exact public reply digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Borrows exact bounded agent text; it is not a requirement until explicitly accepted.
    #[must_use]
    pub const fn text(&self) -> &WorkbenchInputText {
        &self.text
    }
}

/// Category of a host-observed attachment fact; neither category is an instruction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchBriefObservationKind {
    /// Validated immutable image metadata.
    Image,
    /// Validated immutable text-file metadata.
    File,
}

/// Content-free host observation included beside, but never promoted into, the brief.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchBriefObservation {
    kind: WorkbenchBriefObservationKind,
    operation: ControlOperationId,
    version: Option<ControlOperationId>,
    label: String,
    digest: Sha256Digest,
    bytes: u64,
    selected: bool,
}
impl WorkbenchBriefObservation {
    /// Constructs one exact attachment fact.
    ///
    /// # Errors
    /// Rejects invalid labels, sizes, or file/image version shapes.
    pub fn new(
        kind: WorkbenchBriefObservationKind,
        operation: ControlOperationId,
        version: Option<ControlOperationId>,
        label: String,
        digest: Sha256Digest,
        bytes: u64,
        selected: bool,
    ) -> Result<Self, AppProtocolError> {
        if label.trim().is_empty()
            || label.len() > 4096
            || label.chars().any(char::is_control)
            || bytes == 0
            || bytes > 64 * 1024 * 1024
            || matches!(kind, WorkbenchBriefObservationKind::File) != version.is_some()
        {
            return Err(invalid());
        }
        Ok(Self { kind, operation, version, label, digest, bytes, selected })
    }
    /// Returns observed attachment category.
    #[must_use]
    pub const fn kind(&self) -> WorkbenchBriefObservationKind {
        self.kind
    }
    /// Returns original attachment operation.
    #[must_use]
    pub const fn operation(&self) -> ControlOperationId {
        self.operation
    }
    /// Returns current file version, absent for images.
    #[must_use]
    pub const fn version(&self) -> Option<ControlOperationId> {
        self.version
    }
    /// Borrows the inert source label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
    /// Returns immutable content digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Returns observed encoded/selected byte size.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }
    /// Returns current explicit selection state, not provider delivery.
    #[must_use]
    pub const fn selected(&self) -> bool {
        self.selected
    }
}
impl WorkbenchBriefEntry {
    /// Binds a field to its latest revisioned source.
    ///
    /// # Errors
    /// Rejects superseded sources; a brief must point at the current content revision.
    pub fn new(
        field: WorkbenchBriefField,
        source: WorkbenchInputRow,
    ) -> Result<Self, AppProtocolError> {
        if source.state() == WorkbenchInputState::Superseded {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(Self { field, source })
    }
    /// Returns the selected field.
    #[must_use]
    pub const fn field(&self) -> WorkbenchBriefField {
        self.field
    }
    /// Borrows exact user text, revision and observed lifecycle.
    #[must_use]
    pub const fn source(&self) -> &WorkbenchInputRow {
        &self.source
    }
}

/// Complete bounded brief at an exact aggregate revision; reading it starts no work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchBrief {
    query: WorkbenchQuery,
    revision: u64,
    entries: Vec<WorkbenchBriefEntry>,
    proposals: Vec<WorkbenchBriefProposal>,
    observations: Vec<WorkbenchBriefObservation>,
    excluded_proposals: u32,
}
impl WorkbenchBrief {
    /// Validates a complete, canonically ordered projection.
    ///
    /// # Errors
    /// Rejects zero revision, excess fields, duplicate/out-of-order fields or reused source identities.
    pub fn new(
        query: WorkbenchQuery,
        revision: u64,
        entries: Vec<WorkbenchBriefEntry>,
    ) -> Result<Self, AppProtocolError> {
        if revision == 0
            || entries.len() > MAX_WORKBENCH_BRIEF_FIELDS
            || entries.windows(2).any(|pair| pair[0].field >= pair[1].field)
            || entries
                .iter()
                .map(|entry| entry.source.selected().id())
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != entries.len()
        {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(Self {
            query,
            revision,
            entries,
            proposals: Vec::new(),
            observations: Vec::new(),
            excluded_proposals: 0,
        })
    }
    /// Validates the full separated user-confirmed, agent-proposed and observed projection.
    ///
    /// # Errors
    /// Rejects excessive, duplicate, noncanonical, or inconsistent source collections.
    pub fn with_sources(
        query: WorkbenchQuery,
        revision: u64,
        entries: Vec<WorkbenchBriefEntry>,
        proposals: Vec<WorkbenchBriefProposal>,
        observations: Vec<WorkbenchBriefObservation>,
        excluded_proposals: u32,
    ) -> Result<Self, AppProtocolError> {
        let mut value = Self::new(query, revision, entries)?;
        if proposals.len() > MAX_WORKBENCH_BRIEF_PROPOSALS
            || observations.len() > MAX_WORKBENCH_BRIEF_OBSERVATIONS
            || proposals.windows(2).any(|pair| pair[0].invocation >= pair[1].invocation)
            || observations.windows(2).any(|pair| pair[0].operation >= pair[1].operation)
        {
            return Err(invalid());
        }
        value.proposals = proposals;
        value.observations = observations;
        value.excluded_proposals = excluded_proposals;
        Ok(value)
    }
    /// Returns exact conversation/workspace scope.
    #[must_use]
    pub const fn query(&self) -> WorkbenchQuery {
        self.query
    }
    /// Returns the durable aggregate revision for subsequent mutations.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Borrows all explicitly user-confirmed fields, including held/withdrawn sources.
    #[must_use]
    pub fn entries(&self) -> &[WorkbenchBriefEntry] {
        &self.entries
    }
    /// Borrows exact bounded agent reply candidates; none are accepted instructions.
    #[must_use]
    pub fn proposals(&self) -> &[WorkbenchBriefProposal] {
        &self.proposals
    }
    /// Borrows host-observed attachment facts, separate from instructions and suggestions.
    #[must_use]
    pub fn observations(&self) -> &[WorkbenchBriefObservation] {
        &self.observations
    }
    /// Returns agent replies omitted because they cannot fit a brief field or page bound.
    #[must_use]
    pub const fn excluded_proposals(&self) -> u32 {
        self.excluded_proposals
    }
}
