//! Revision-bound brief metadata and immutable proposal-body transport pages.
use super::{
    AppProtocolError, ControlOperationId, Sha256Digest, WorkbenchBrief, WorkbenchBriefEntry,
    WorkbenchBriefObservation, WorkbenchInvocationId, WorkbenchQuery, invalid,
};

/// Metadata entries per transport page; this never limits the complete brief inventory.
pub const WORKBENCH_BRIEF_PAGE_ITEMS: usize = 16;
/// Maximum UTF-8 bytes per proposal-body page; the complete reply remains immutable.
pub const WORKBENCH_BRIEF_BODY_BYTES: usize = 32 * 1024;

/// Exact metadata inventory page, optionally fenced to an already inspected revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchBriefPageRequest {
    query: WorkbenchQuery,
    revision: u64,
    proposals: u64,
    observations: u64,
}
impl WorkbenchBriefPageRequest {
    /// Starts at revision zero or continues an exact inspected revision.
    /// # Errors
    /// Rejects continuation offsets without a revision fence.
    pub const fn new(
        query: WorkbenchQuery,
        revision: u64,
        proposals: u64,
        observations: u64,
    ) -> Result<Self, AppProtocolError> {
        if revision == 0 && (proposals != 0 || observations != 0) {
            return Err(invalid());
        }
        Ok(Self { query, revision, proposals, observations })
    }
    /// Returns the authenticated conversation/workspace selection.
    #[must_use]
    pub const fn query(self) -> WorkbenchQuery {
        self.query
    }
    /// Returns the expected revision, or zero for initial inspection.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
    /// Returns the first proposal ordinal in canonical invocation order.
    #[must_use]
    pub const fn proposals(self) -> u64 {
        self.proposals
    }
    /// Returns the first attachment ordinal in canonical operation order.
    #[must_use]
    pub const fn observations(self) -> u64 {
        self.observations
    }
}

/// Constant-size exact public reply identity; it is not a user-confirmed instruction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchBriefProposalReference {
    operation: ControlOperationId,
    invocation: WorkbenchInvocationId,
    digest: Sha256Digest,
    bytes: u64,
}
impl WorkbenchBriefProposalReference {
    /// Binds publication identity and complete immutable UTF-8 content.
    /// # Errors
    /// Rejects an empty reply, which cannot be published by the reply ledger.
    pub const fn new(
        operation: ControlOperationId,
        invocation: WorkbenchInvocationId,
        digest: Sha256Digest,
        bytes: u64,
    ) -> Result<Self, AppProtocolError> {
        if bytes == 0 {
            return Err(invalid());
        }
        Ok(Self { operation, invocation, digest, bytes })
    }
    /// Returns the exact reply publication operation.
    #[must_use]
    pub const fn operation(self) -> ControlOperationId {
        self.operation
    }
    /// Returns the invocation preceding this reply.
    #[must_use]
    pub const fn invocation(self) -> WorkbenchInvocationId {
        self.invocation
    }
    /// Returns the complete reply digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
    /// Returns the complete reply byte count.
    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.bytes
    }
}

/// One complete metadata page with exact total counts and confirmed field provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchBriefPage {
    request: WorkbenchBriefPageRequest,
    revision: u64,
    entries: Vec<WorkbenchBriefEntry>,
    proposal_total: u64,
    proposals: Vec<WorkbenchBriefProposalReference>,
    observation_total: u64,
    observations: Vec<WorkbenchBriefObservation>,
}
impl WorkbenchBriefPage {
    /// Validates canonical collections and exact page coverage without a whole-inventory ceiling.
    /// # Errors
    /// Rejects changed revisions, missing rows, invalid provenance, or noncanonical identities.
    pub fn new(
        request: WorkbenchBriefPageRequest,
        revision: u64,
        entries: Vec<WorkbenchBriefEntry>,
        proposal_total: u64,
        proposals: Vec<WorkbenchBriefProposalReference>,
        observation_total: u64,
        observations: Vec<WorkbenchBriefObservation>,
    ) -> Result<Self, AppProtocolError> {
        WorkbenchBrief::new(request.query(), revision, entries.clone())?;
        if (request.revision != 0 && request.revision != revision)
            || !complete_page(request.proposals, proposal_total, proposals.len())
            || !complete_page(request.observations, observation_total, observations.len())
            || proposals.windows(2).any(|pair| pair[0].invocation >= pair[1].invocation)
            || proposals
                .iter()
                .map(|value| value.operation)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != proposals.len()
            || observations.windows(2).any(|pair| pair[0].operation() >= pair[1].operation())
        {
            return Err(invalid());
        }
        Ok(Self {
            request,
            revision,
            entries,
            proposal_total,
            proposals,
            observation_total,
            observations,
        })
    }
    /// Returns exact page selection and revision request.
    #[must_use]
    pub const fn request(&self) -> WorkbenchBriefPageRequest {
        self.request
    }
    /// Returns the committed inspected aggregate revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Borrows all confirmed fields; fields are a closed vocabulary independent of page size.
    #[must_use]
    pub fn entries(&self) -> &[WorkbenchBriefEntry] {
        &self.entries
    }
    /// Returns complete candidate count, including bodies larger than a transport page.
    #[must_use]
    pub const fn proposal_total(&self) -> u64 {
        self.proposal_total
    }
    /// Borrows this page of immutable reply identities.
    #[must_use]
    pub fn proposals(&self) -> &[WorkbenchBriefProposalReference] {
        &self.proposals
    }
    /// Returns complete observed attachment count.
    #[must_use]
    pub const fn observation_total(&self) -> u64 {
        self.observation_total
    }
    /// Borrows this page of content-free attachment facts.
    #[must_use]
    pub fn observations(&self) -> &[WorkbenchBriefObservation] {
        &self.observations
    }
}
fn complete_page(offset: u64, total: u64, count: usize) -> bool {
    offset <= total && count as u64 == (total - offset).min(WORKBENCH_BRIEF_PAGE_ITEMS as u64)
}

/// One exact UTF-8 byte range of a previously inspected immutable proposal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchBriefProposalRequest {
    query: WorkbenchQuery,
    revision: u64,
    proposal: WorkbenchBriefProposalReference,
    offset: u64,
}
impl WorkbenchBriefProposalRequest {
    /// Selects the next byte position; the host validates the actual UTF-8 boundary.
    /// # Errors
    /// Rejects a missing revision or a range outside the immutable reply.
    pub const fn new(
        query: WorkbenchQuery,
        revision: u64,
        proposal: WorkbenchBriefProposalReference,
        offset: u64,
    ) -> Result<Self, AppProtocolError> {
        if revision == 0 || offset >= proposal.bytes {
            return Err(invalid());
        }
        Ok(Self { query, revision, proposal, offset })
    }
    /// Returns exact authorized conversation/workspace scope.
    #[must_use]
    pub const fn query(self) -> WorkbenchQuery {
        self.query
    }
    /// Returns the inspected aggregate revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
    /// Returns the complete immutable proposal identity.
    #[must_use]
    pub const fn proposal(self) -> WorkbenchBriefProposalReference {
        self.proposal
    }
    /// Returns the exact starting byte offset.
    #[must_use]
    pub const fn offset(self) -> u64 {
        self.offset
    }
}

/// A contiguous exact text page; terminal presentation must render source controls inertly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchBriefProposalPage {
    request: WorkbenchBriefProposalRequest,
    text: String,
}
impl WorkbenchBriefProposalPage {
    /// Validates transport capacity and a complete page ending at a UTF-8 boundary.
    /// # Errors
    /// Rejects empty, excessive, or unexpectedly short pages and mismatched complete digests.
    pub fn new(
        request: WorkbenchBriefProposalRequest,
        text: String,
    ) -> Result<Self, AppProtocolError> {
        let remaining = request.proposal.bytes - request.offset;
        let maximum = remaining.min(WORKBENCH_BRIEF_BODY_BYTES as u64);
        let bytes = text.len() as u64;
        if bytes == 0
            || bytes > maximum
            || (remaining <= WORKBENCH_BRIEF_BODY_BYTES as u64 && bytes != remaining)
            || bytes < maximum.saturating_sub(3)
            || (request.offset == 0
                && bytes == request.proposal.bytes
                && peritus_codec::sha256(text.as_bytes()) != request.proposal.digest)
        {
            return Err(invalid());
        }
        Ok(Self { request, text })
    }
    /// Returns the exact immutable content and byte-range request.
    #[must_use]
    pub const fn request(&self) -> WorkbenchBriefProposalRequest {
        self.request
    }
    /// Borrows exact source bytes as UTF-8, without changing the digest for display purposes.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Returns the next exact UTF-8 boundary, or none at the complete reply end.
    #[must_use]
    pub fn next(&self) -> Option<u64> {
        let next = self.request.offset + self.text.len() as u64;
        (next < self.request.proposal.bytes).then_some(next)
    }
}
