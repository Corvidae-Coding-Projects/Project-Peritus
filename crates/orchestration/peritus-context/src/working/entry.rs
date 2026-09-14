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
#[derive(Debug, Eq, PartialEq)]
pub struct WorkingLinks {
    supports: Vec<ObservationId>,
    contradicts: Vec<ObservationId>,
    depends_on: Vec<ContextNodeId>,
}

impl WorkingLinks {
    /// Logical supporting observation sequence.
    pub closed spec fn spec_supports(&self) -> Seq<ObservationId> { self.supports@ }
    /// Logical contradicting observation sequence.
    pub closed spec fn spec_contradicts(&self) -> Seq<ObservationId> { self.contradicts@ }
    /// Logical prerequisite entry sequence.
    pub closed spec fn spec_depends_on(&self) -> Seq<ContextNodeId> { self.depends_on@ }
    /// Complete semantic equality retained by cloning links.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.spec_supports() == right.spec_supports()
            && left.spec_contradicts() == right.spec_contradicts()
            && left.spec_depends_on() == right.spec_depends_on()
    }

    /// Checks bounds, strict ordering, and support/contradiction disjointness.
    ///
    /// # Errors
    /// Rejects unbounded, duplicate, unordered, overlapping, or source-free references.
    pub fn new(
        supports: Vec<ObservationId>,
        contradicts: Vec<ObservationId>,
        depends_on: Vec<ContextNodeId>,
        limits: WorkingLimits,
    ) -> (result: Result<Self, WorkingError>)
        ensures result.is_err() ==> result.unwrap_err().spec_is_delta_error(),
    {
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
    pub const fn depends_on(&self) -> (result: &[ContextNodeId])
        ensures result@ == self.spec_depends_on(),
    { self.depends_on.as_slice() }
}

impl Clone for WorkingLinks {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        Self {
            supports: self.supports.clone(),
            contradicts: self.contradicts.clone(),
            depends_on: self.depends_on.clone(),
        }
    }
}

/// Immutable bounded investigation entry. Hosts must redact content before construction.
#[derive(Debug, Eq, PartialEq)]
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
    /// Logical stable entry identity.
    pub closed spec fn spec_id(&self) -> ContextNodeId { self.id }
    /// Logical entry purpose.
    pub closed spec fn spec_kind(&self) -> WorkingEntryKind { self.kind }
    /// Logical current investigation status.
    pub closed spec fn spec_status(&self) -> WorkingEntryStatus { self.status }
    /// Logical exact bounded content.
    pub closed spec fn spec_content(&self) -> ContextContent { self.content }
    /// Logical source and prerequisite links.
    pub closed spec fn spec_links(&self) -> WorkingLinks { self.links }
    /// Logical validity dependencies.
    pub closed spec fn spec_validity(&self) -> WorkingValidity { self.validity }
    /// Logical superseded predecessor.
    pub closed spec fn spec_supersedes(&self) -> Option<ContextNodeId> { self.supersedes }
    /// Logical observation frontier for stale status.
    pub closed spec fn spec_stale_through(&self) -> u64 { self.stale_through }

    /// Complete semantic equality retained by cloning an entry.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        &&& left.spec_id() == right.spec_id()
        &&& left.spec_kind() == right.spec_kind()
        &&& left.spec_status() == right.spec_status()
        &&& ContextContent::clone_equivalent(&left.spec_content(), &right.spec_content())
        &&& WorkingLinks::clone_equivalent(&left.spec_links(), &right.spec_links())
        &&& WorkingValidity::clone_equivalent(&left.spec_validity(), &right.spec_validity())
        &&& left.spec_supersedes() == right.spec_supersedes()
        &&& left.spec_stale_through() == right.spec_stale_through()
    }

    /// All source-backed payload fields preserved while the host derives status.
    /// Source-backed payload equality while host-derived status may differ.
    pub open spec fn payload_equivalent(left: &Self, right: &Self) -> bool {
        &&& left.spec_id() == right.spec_id()
        &&& left.spec_kind() == right.spec_kind()
        &&& ContextContent::clone_equivalent(&left.spec_content(), &right.spec_content())
        &&& WorkingLinks::clone_equivalent(&left.spec_links(), &right.spec_links())
        &&& WorkingValidity::clone_equivalent(&left.spec_validity(), &right.spec_validity())
        &&& left.spec_supersedes() == right.spec_supersedes()
    }

    /// Elementwise source-backed payload equality for entry sequences.
    pub open spec fn sequence_payload_equivalent(
        left: Seq<Self>,
        right: Seq<Self>,
    ) -> bool {
        left.len() == right.len()
            && forall |index: int| #![auto] 0 <= index < left.len() ==>
                Self::payload_equivalent(&left[index], &right[index])
    }

    /// Clone equivalence implies source-backed payload equivalence.
    pub proof fn sequence_clone_implies_payload(left: Seq<Self>, right: Seq<Self>)
        requires Self::sequence_clone_equivalent(left, right),
        ensures Self::sequence_payload_equivalent(left, right),
    {
        assert forall |index: int| #![auto] 0 <= index < left.len() implies
            Self::payload_equivalent(&left[index], &right[index]) by {
            reveal(WorkingEntry::clone_equivalent);
            reveal(WorkingEntry::payload_equivalent);
        }
    }

    /// Source-backed payload equivalence composes transitively.
    pub proof fn payload_transitive(left: &Self, middle: &Self, right: &Self)
        requires
            Self::payload_equivalent(left, middle),
            Self::payload_equivalent(middle, right),
        ensures Self::payload_equivalent(left, right),
    {
        reveal(WorkingEntry::payload_equivalent);
        reveal(WorkingLinks::clone_equivalent);
        reveal(WorkingValidity::clone_equivalent);
        reveal(ContextContent::clone_equivalent);
    }

    /// Complete semantic equality is reflexive for an unchanged entry value.
    pub proof fn clone_reflexive(entry: &Self)
        ensures Self::clone_equivalent(entry, entry),
    {
        reveal(WorkingEntry::clone_equivalent);
        reveal(WorkingLinks::clone_equivalent);
        reveal(WorkingValidity::clone_equivalent);
        reveal(ContextContent::clone_equivalent);
    }

    /// Elementwise semantic equivalence retained by cloning an entry sequence.
    pub open spec fn sequence_clone_equivalent(
        left: Seq<Self>,
        right: Seq<Self>,
    ) -> bool {
        left.len() == right.len()
            && forall |index: int| #![auto] 0 <= index < left.len() ==>
                Self::clone_equivalent(&left[index], &right[index])
    }

    /// Clones an entry sequence with complete semantic field preservation.
    pub(super) fn clone_sequence(entries: &[Self]) -> (result: Vec<Self>)
        ensures Self::sequence_clone_equivalent(entries@, result@),
    {
        let mut result = Vec::with_capacity(entries.len());
        let mut index = 0;
        while index < entries.len()
            invariant
                index <= entries.len(),
                result@.len() == index,
                forall |prior: int| #![auto] 0 <= prior < index ==>
                    Self::clone_equivalent(&entries@[prior], &result@[prior]),
            decreases entries.len() - index,
        {
            result.push(entries[index].clone());
            index += 1;
        }
        result
    }

    /// Changes only status fields owned by the host invalidation reducer.
    pub(super) const fn with_host_status(
        self,
        status: WorkingEntryStatus,
        stale_through: u64,
    ) -> (result: Self)
        ensures
            Self::payload_equivalent(&self, &result),
            result.spec_status() == status,
            result.spec_stale_through() == stale_through,
    {
        let mut result = self;
        result.status = status;
        result.stale_through = stale_through;
        result
    }
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
    ) -> (result: Result<Self, WorkingError>)
        ensures match result {
            Ok(_) => true,
            Err(error) => error == WorkingError::Capacity,
        },
    {
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
    pub const fn id(&self) -> (result: ContextNodeId)
        ensures result == self.spec_id(),
    { self.id }
    /// Semantic entry purpose.
    #[must_use]
    pub const fn kind(&self) -> WorkingEntryKind { self.kind }
    /// Current investigation or host-derived status.
    #[must_use]
    pub const fn status(&self) -> (result: WorkingEntryStatus)
        ensures result == self.spec_status(),
    { self.status }
    /// Exact bounded content and digest.
    #[must_use]
    pub const fn content(&self) -> &ContextContent { &self.content }
    /// Supporting, contradicting, and prerequisite references.
    #[must_use]
    pub const fn links(&self) -> (result: &WorkingLinks)
        ensures WorkingLinks::clone_equivalent(result, &self.spec_links()),
    { &self.links }
    /// Explicit dependency constraints.
    #[must_use]
    pub const fn validity(&self) -> (result: &WorkingValidity)
        ensures WorkingValidity::clone_equivalent(result, &self.spec_validity()),
    { &self.validity }
    /// Earlier record replaced by this one.
    #[must_use]
    pub const fn supersedes(&self) -> (result: Option<ContextNodeId>)
        ensures result == self.spec_supersedes(),
    { self.supersedes }
    /// Observation frontier recorded when the host derived stale status.
    #[must_use]
    pub const fn stale_through(&self) -> (result: u64)
        ensures result == self.spec_stale_through(),
    { self.stale_through }
    /// Every working entry remains non-authoritative, regardless of model-assigned status.
    #[must_use]
    pub const fn authority(&self) -> (authority: AuthorityClass)
        ensures authority == AuthorityClass::NonAuthoritative,
    { AuthorityClass::NonAuthoritative }
}

impl Clone for WorkingEntry {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        Self {
            id: self.id,
            kind: self.kind,
            status: self.status,
            content: self.content.clone(),
            links: self.links.clone(),
            validity: self.validity.clone(),
            supersedes: self.supersedes,
            stale_through: self.stale_through,
        }
    }
}

fn validate_sources(sources: &[ObservationId]) -> (result: Result<(), WorkingError>)
    ensures match result {
        Ok(()) => true,
        Err(error) => error == WorkingError::NonCanonicalOrder,
    },
{
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
