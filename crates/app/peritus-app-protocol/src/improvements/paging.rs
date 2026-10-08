//! Content-free history pages and independent immutable text slices.

use super::ImprovementEvaluation;
use crate::{AppErrorCode, AppProtocolError};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

/// Per-response work size; it never limits retained candidates or observations.
pub const IMPROVEMENT_PAGE_ITEMS: usize = 128;
/// Per-response UTF-8 body slice, independent of total retained body size.
pub const IMPROVEMENT_TEXT_CHUNK_BYTES: usize = 32 * 1024;

/// A keyset cursor bound to one exact inbox or evidence membership snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImprovementPageCursor {
    workspace: WorkspaceId,
    candidate: Option<Sha256Digest>,
    revision: u64,
    highwater_sequence: u64,
    after_sequence: u64,
    dismissed: bool,
}

impl ImprovementPageCursor {
    /// Retains the exact scope and last durable ordering key.
    ///
    /// # Errors
    /// Rejects a zero revision or sequence and the impossible dismissed evidence partition.
    pub const fn new(workspace: WorkspaceId, candidate: Option<Sha256Digest>, revision: u64, sequence: u64, dismissed: bool) -> Result<Self, AppProtocolError> {
        if revision == 0 || sequence == 0 || (candidate.is_some() && dismissed) {
            return Err(malformed());
        }
        Ok(Self {
            workspace,
            candidate,
            revision,
            highwater_sequence: sequence,
            after_sequence: sequence,
            dismissed,
        })
    }
    /// Retains an immutable membership boundary and the last emitted key.
    ///
    /// # Errors
    /// Rejects zero or reversed snapshot positions.
    pub const fn snapshot(
        workspace: WorkspaceId,
        candidate: Option<Sha256Digest>,
        revision: u64,
        highwater_sequence: u64,
        after_sequence: u64,
    ) -> Result<Self, AppProtocolError> {
        if revision == 0
            || highwater_sequence == 0
            || after_sequence == 0
            || after_sequence > highwater_sequence
        {
            return Err(malformed());
        }
        Ok(Self {
            workspace,
            candidate,
            revision,
            highwater_sequence,
            after_sequence,
            dismissed: false,
        })
    }
    /// Returns the owning workspace.
    #[must_use]
    pub const fn workspace(self) -> WorkspaceId { self.workspace }
    /// Returns the evidence owner, or `None` for an inbox cursor.
    #[must_use]
    pub const fn candidate(self) -> Option<Sha256Digest> { self.candidate }
    /// Returns the inbox revision observed when this traversal began.
    #[must_use]
    pub const fn revision(self) -> u64 { self.revision }
    /// Returns the immutable upper membership boundary.
    #[must_use]
    pub const fn highwater_sequence(self) -> u64 { self.highwater_sequence }
    /// Returns the durable last ordering key.
    #[must_use]
    pub const fn sequence(self) -> u64 { self.after_sequence }
    /// Returns the legacy inbox partition flag.
    #[must_use]
    pub const fn dismissed(self) -> bool { self.dismissed }
}

/// Immutable body identity and byte length; metadata carries no executable instructions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImprovementTextReference { digest: Sha256Digest, bytes: u64 }

impl ImprovementTextReference {
    /// Binds the exact retained body digest and size.
    ///
    /// # Errors
    /// Rejects an empty body, which cannot be a retained proposal or observation.
    pub const fn new(digest: Sha256Digest, bytes: u64) -> Result<Self, AppProtocolError> {
        if bytes == 0 {
            return Err(malformed());
        }
        Ok(Self { digest, bytes })
    }
    /// Returns the exact body digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest { self.digest }
    /// Returns the full UTF-8 byte size, independent of slice size.
    #[must_use]
    pub const fn bytes(self) -> u64 { self.bytes }
}

/// Candidate metadata without its complete proposal or observation bodies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImprovementCandidateSummary {
    id: Sha256Digest,
    proposal: ImprovementTextReference,
    evidence_count: u64,
    evaluation: Option<ImprovementEvaluation>,
    dismissed: bool,
}

impl ImprovementCandidateSummary {
    /// Binds retained candidate metadata.
    #[must_use]
    pub const fn new(id: Sha256Digest, proposal: ImprovementTextReference, evidence_count: u64, evaluation: Option<ImprovementEvaluation>, dismissed: bool) -> Self {
        Self { id, proposal, evidence_count, evaluation, dismissed }
    }
    /// Returns the stable deduplication identity.
    #[must_use]
    pub const fn id(self) -> Sha256Digest { self.id }
    /// Returns the independently readable proposal.
    #[must_use]
    pub const fn proposal(self) -> ImprovementTextReference { self.proposal }
    /// Returns the retained observation count.
    #[must_use]
    pub const fn evidence_count(self) -> u64 { self.evidence_count }
    /// Returns the original explicit evaluation route, if reserved.
    #[must_use]
    pub const fn evaluation(self) -> Option<ImprovementEvaluation> { self.evaluation }
    /// Returns the durable dismissal state.
    #[must_use]
    pub const fn dismissed(self) -> bool { self.dismissed }
}

/// One inert observation's metadata without the full retained summary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImprovementEvidenceSummary { run: RunId, summary: ImprovementTextReference }

impl ImprovementEvidenceSummary {
    /// Binds the source run to its immutable observation.
    #[must_use]
    pub const fn new(run: RunId, summary: ImprovementTextReference) -> Self { Self { run, summary } }
    /// Returns the terminal source run.
    #[must_use]
    pub const fn run(self) -> RunId { self.run }
    /// Returns the immutable observation body.
    #[must_use]
    pub const fn summary(self) -> ImprovementTextReference { self.summary }
}

/// One snapshot-bounded page of retained candidate metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImprovementPage {
    workspace: WorkspaceId,
    revision: u64,
    candidates: Vec<ImprovementCandidateSummary>,
    next: Option<ImprovementPageCursor>,
}

impl ImprovementPage {
    /// Checks one page's distinct identities and cursor scope.
    ///
    /// # Errors
    /// Rejects malformed page metadata or a cursor for another owner or revision.
    pub fn new(workspace: WorkspaceId, revision: u64, candidates: Vec<ImprovementCandidateSummary>, next: Option<ImprovementPageCursor>) -> Result<Self, AppProtocolError> {
        let mut seen = std::collections::BTreeSet::new();
        if candidates.len() > IMPROVEMENT_PAGE_ITEMS
            || (!candidates.is_empty() && revision == 0)
            || candidates.iter().any(|v| v.evidence_count == 0 || !seen.insert(v.id.into_bytes()))
            || next.is_some_and(|v| v.workspace != workspace || v.candidate.is_some() || v.revision != revision || v.after_sequence == 0 || v.after_sequence > v.highwater_sequence || v.dismissed)
        { return Err(malformed()); }
        Ok(Self { workspace, revision, candidates, next })
    }
    /// Returns the owning workspace.
    #[must_use]
    pub const fn workspace(&self) -> WorkspaceId { self.workspace }
    /// Returns the history revision shared by this page and its cursor.
    #[must_use]
    pub const fn revision(&self) -> u64 { self.revision }
    /// Borrows this page's candidate metadata.
    #[must_use]
    pub fn candidates(&self) -> &[ImprovementCandidateSummary] { &self.candidates }
    /// Returns the continuation cursor, never an expiration or work allowance.
    #[must_use]
    pub const fn next(&self) -> Option<ImprovementPageCursor> { self.next }
}

/// One snapshot-bounded page of a candidate's observation metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImprovementEvidencePage {
    workspace: WorkspaceId,
    candidate: Sha256Digest,
    revision: u64,
    evidence: Vec<ImprovementEvidenceSummary>,
    next: Option<ImprovementPageCursor>,
}

impl ImprovementEvidencePage {
    /// Checks one page's distinct source runs and cursor scope.
    ///
    /// # Errors
    /// Rejects malformed page metadata or a cursor for another owner or revision.
    pub fn new(workspace: WorkspaceId, candidate: Sha256Digest, revision: u64, evidence: Vec<ImprovementEvidenceSummary>, next: Option<ImprovementPageCursor>) -> Result<Self, AppProtocolError> {
        let mut seen = std::collections::BTreeSet::new();
        if evidence.len() > IMPROVEMENT_PAGE_ITEMS
            || (!evidence.is_empty() && revision == 0)
            || evidence.iter().any(|v| !seen.insert(v.run.into_bytes()))
            || next.is_some_and(|v| v.workspace != workspace || v.candidate != Some(candidate) || v.revision != revision || v.after_sequence == 0 || v.after_sequence > v.highwater_sequence || v.dismissed)
        { return Err(malformed()); }
        Ok(Self { workspace, candidate, revision, evidence, next })
    }
    /// Returns the owning workspace.
    #[must_use]
    pub const fn workspace(&self) -> WorkspaceId { self.workspace }
    /// Returns the exact retained candidate.
    #[must_use]
    pub const fn candidate(&self) -> Sha256Digest { self.candidate }
    /// Returns the retained-history revision.
    #[must_use]
    pub const fn revision(&self) -> u64 { self.revision }
    /// Borrows this page's observations.
    #[must_use]
    pub fn evidence(&self) -> &[ImprovementEvidenceSummary] { &self.evidence }
    /// Returns the exact continuation cursor.
    #[must_use]
    pub const fn next(&self) -> Option<ImprovementPageCursor> { self.next }
}

/// An exact immutable text source and byte position, independent of changing inbox metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImprovementTextQuery {
    workspace: WorkspaceId,
    candidate: Sha256Digest,
    run: Option<RunId>,
    source: ImprovementTextReference,
    offset: u64,
}

impl ImprovementTextQuery {
    /// Binds a proposal (`run=None`) or source observation (`run=Some`) and its exact body.
    ///
    /// # Errors
    /// Rejects an offset beyond the exact retained source length.
    pub const fn new(workspace: WorkspaceId, candidate: Sha256Digest, run: Option<RunId>, source: ImprovementTextReference, offset: u64) -> Result<Self, AppProtocolError> {
        if offset > source.bytes {
            return Err(malformed());
        }
        Ok(Self { workspace, candidate, run, source, offset })
    }
    /// Returns the owning workspace.
    #[must_use]
    pub const fn workspace(self) -> WorkspaceId { self.workspace }
    /// Returns the exact retained candidate.
    #[must_use]
    pub const fn candidate(self) -> Sha256Digest { self.candidate }
    /// Returns the observation's source run or `None` for the proposal.
    #[must_use]
    pub const fn run(self) -> Option<RunId> { self.run }
    /// Returns the exact immutable body identity.
    #[must_use]
    pub const fn source(self) -> ImprovementTextReference { self.source }
    /// Returns the requested UTF-8 byte offset.
    #[must_use]
    pub const fn offset(self) -> u64 { self.offset }
}

/// A UTF-8 slice carrying its original exact source query and next byte offset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImprovementTextPage { query: ImprovementTextQuery, text: String, next: Option<u64> }

impl ImprovementTextPage {
    /// Checks a progress-making slice without limiting its full retained source.
    ///
    /// # Errors
    /// Rejects overflow, out-of-range or inconsistent continuation metadata.
    pub fn new(query: ImprovementTextQuery, text: String, next: Option<u64>) -> Result<Self, AppProtocolError> {
        let end = query.offset.checked_add(u64::try_from(text.len()).map_err(|_| malformed())?)
            .ok_or_else(malformed)?;
        if text.len() > IMPROVEMENT_TEXT_CHUNK_BYTES
            || text.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
            || end > query.source.bytes
            || (end < query.source.bytes && (text.is_empty() || next != Some(end)))
            || (end == query.source.bytes && next.is_some())
        { return Err(malformed()); }
        Ok(Self { query, text, next })
    }
    /// Returns the exact original source and offset.
    #[must_use]
    pub const fn query(&self) -> ImprovementTextQuery { self.query }
    /// Borrows this inert source slice.
    #[must_use]
    pub fn text(&self) -> &str { &self.text }
    /// Returns the next UTF-8 byte offset, or `None` at the retained source end.
    #[must_use]
    pub const fn next(&self) -> Option<u64> { self.next }
}

const fn malformed() -> AppProtocolError { AppProtocolError::new(AppErrorCode::MalformedFrame, None) }
