//! Source-backed investigation records with no evidence or instruction authority.

use super::{ObservationId, WorkingError, WorkingLimits, WorkingValidity};
use crate::{AuthorityClass, ContextContent, ContextNodeId};
use vstd::prelude::*;

verus! {
/// Semantic purpose of a derived, task-local record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkingEntryKind {
    /// A record of what an observed source returned, not endorsement of its claims.
    Observation,
    /// An agent assertion that remains subject to evidence and independent gates.
    Assertion,
    /// A tentative explanation with supporting and contradicting observations.
    Hypothesis,
    /// A decision with a concise, visible explanation.
    Decision,
    /// An unsuccessful approach retained to avoid repeating unchanged work.
    FailedApproach,
    /// A next action with explicit prerequisite records.
    Plan,
    /// A bounded entity or terminology definition.
    Glossary,
}

/// Investigation status; no status represents independent gate acceptance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkingEntryStatus {
    /// Current observation or unfinished investigation.
    Open,
    /// The model has recorded conflicting evidence.
    Contradicted,
    /// The model reports its investigation concluded; still non-authoritative.
    Resolved,
    /// Host invalidation found changed or uncertain dependencies.
    Stale,
    /// Another retained record explicitly replaced this entry.
    Superseded,
}

/// Canonical source and entry references for a proposed record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingLinks {
    supports: Vec<ObservationId>,
    contradicts: Vec<ObservationId>,
    depends_on: Vec<ContextNodeId>,
}

impl WorkingLinks {
    /// Checks bounds, strict ordering, and support/contradiction disjointness.
    ///
    /// # Errors
    /// Rejects unbounded, duplicate, unordered, overlapping, or source-free references.
    pub fn new(
        supports: Vec<ObservationId>,
        contradicts: Vec<ObservationId>,
        depends_on: Vec<ContextNodeId>,
        limits: WorkingLimits,
    ) -> Result<Self, WorkingError> {
        if supports.is_empty() && contradicts.is_empty() { return Err(WorkingError::EmptyEntry); }
        if supports.len() > limits.links() || contradicts.len() > limits.links()
            || depends_on.len() > limits.links()
        { return Err(WorkingError::Capacity); }
        validate_sources(&supports)?;
        validate_sources(&contradicts)?;
        let mut index = 1;
        while index < depends_on.len()
            invariant index >= 1,
            decreases depends_on.len() - index,
        {
            if depends_on[index - 1] >= depends_on[index] { return Err(WorkingError::NonCanonicalOrder); }
            index += 1;
        }
        index = 0;
        while index < supports.len()
            invariant index <= supports.len(),
            decreases supports.len() - index,
        {
            let mut other = 0;
            while other < contradicts.len()
                invariant other <= contradicts.len(), index < supports.len(),
                decreases contradicts.len() - other,
            {
                if supports[index] == contradicts[other] { return Err(WorkingError::ConflictingEvidence); }
                other += 1;
            }
            index += 1;
        }
        Ok(Self { supports, contradicts, depends_on })
    }
    /// Exact supporting observation handles.
    #[must_use]
    pub const fn supports(&self) -> &[ObservationId] { self.supports.as_slice() }
    /// Exact counterevidence handles, retained alongside support.
    #[must_use]
    pub const fn contradicts(&self) -> &[ObservationId] { self.contradicts.as_slice() }
    /// Entry prerequisites, in strict identifier order.
    #[must_use]
    pub const fn depends_on(&self) -> &[ContextNodeId] { self.depends_on.as_slice() }
}

/// Immutable bounded investigation entry. Hosts must redact content before construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingEntry {
    pub(super) id: ContextNodeId,
    pub(super) kind: WorkingEntryKind,
    pub(super) status: WorkingEntryStatus,
    pub(super) content: ContextContent,
    pub(super) links: WorkingLinks,
    pub(super) validity: WorkingValidity,
    pub(super) supersedes: Option<ContextNodeId>,
    pub(super) stale_through: u64,
}

impl WorkingEntry {
    /// Creates an open, source-backed record under the supplied allocation limits.
    ///
    /// # Errors
    /// Rejects excessive content or references. Source existence and acyclicity are checked on
    /// atomic application to the bound state.
    pub fn new(
        id: ContextNodeId,
        kind: WorkingEntryKind,
        content: ContextContent,
        links: WorkingLinks,
        validity: WorkingValidity,
        limits: WorkingLimits,
    ) -> Result<Self, WorkingError> {
        if content.len() > limits.entry_bytes() || links.supports.len() > limits.links()
            || links.contradicts.len() > limits.links() || links.depends_on.len() > limits.links()
            || validity.files().len() > limits.links()
        { return Err(WorkingError::Capacity); }
        Ok(Self { id, kind, content, links, validity, status: WorkingEntryStatus::Open, supersedes: None, stale_through: 0 })
    }
    /// Changes only model-declared investigation status.
    ///
    /// # Errors
    /// Host-derived stale and superseded states cannot be assigned by an agent update.
    pub fn with_status(self, status: WorkingEntryStatus) -> Result<Self, WorkingError> {
        if matches!(status, WorkingEntryStatus::Stale | WorkingEntryStatus::Superseded) {
            return Err(WorkingError::DerivedStatus);
        }
        let mut entry = self;
        entry.status = status;
        Ok(entry)
    }
    /// Explicitly replaces another retained entry, leaving its source evidence intact.
    ///
    /// # Errors
    /// Rejects self-supersession; target existence and graph checks occur on application.
    pub fn with_supersedes(self, previous: ContextNodeId) -> Result<Self, WorkingError> {
        if previous == self.id { return Err(WorkingError::DependencyCycle); }
        let mut entry = self;
        entry.supersedes = Some(previous);
        Ok(entry)
    }
    /// Stable identifier within this working model.
    #[must_use]
    pub const fn id(&self) -> ContextNodeId { self.id }
    /// Semantic entry purpose.
    #[must_use]
    pub const fn kind(&self) -> WorkingEntryKind { self.kind }
    /// Current investigation or host-derived status.
    #[must_use]
    pub const fn status(&self) -> WorkingEntryStatus { self.status }
    /// Exact bounded content and digest.
    #[must_use]
    pub const fn content(&self) -> &ContextContent { &self.content }
    /// Supporting, contradicting, and prerequisite references.
    #[must_use]
    pub const fn links(&self) -> &WorkingLinks { &self.links }
    /// Explicit dependency constraints.
    #[must_use]
    pub const fn validity(&self) -> &WorkingValidity { &self.validity }
    /// Earlier record replaced by this one.
    #[must_use]
    pub const fn supersedes(&self) -> Option<ContextNodeId> { self.supersedes }
    /// Every working entry remains non-authoritative, regardless of model-assigned status.
    #[must_use]
    pub const fn authority(&self) -> (authority: AuthorityClass)
        ensures authority == AuthorityClass::NonAuthoritative,
    { AuthorityClass::NonAuthoritative }
}

fn validate_sources(sources: &[ObservationId]) -> Result<(), WorkingError> {
    let mut index = 1;
    while index < sources.len()
        invariant index >= 1,
        decreases sources.len() - index,
    {
        if sources[index - 1] >= sources[index] { return Err(WorkingError::NonCanonicalOrder); }
        index += 1;
    }
    Ok(())
}
}
