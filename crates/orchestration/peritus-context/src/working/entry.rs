//! Source-backed investigation records with no evidence or instruction authority.

use super::{ObservationId, WorkingError, WorkingLimits, WorkingValidity};
use crate::{AuthorityClass, ContextContent, ContextNodeId};

#[doc = " Semantic purpose of a derived, task-local record."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkingEntryKind {
    #[doc = " A record of what an observed source returned, not endorsement of its claims."]
    Observation,
    #[doc = " An agent assertion that remains subject to evidence and independent gates."]
    Assertion,
    #[doc = " A tentative explanation with supporting and contradicting observations."]
    Hypothesis,
    #[doc = " A decision with a concise, visible explanation."]
    Decision,
    #[doc = " An unsuccessful approach retained to avoid repeating unchanged work."]
    FailedApproach,
    #[doc = " A next action with explicit prerequisite records."]
    Plan,
    #[doc = " A bounded entity or terminology definition."]
    Glossary,
}
#[doc = " Investigation status; no status represents independent gate acceptance."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkingEntryStatus {
    #[doc = " Current observation or unfinished investigation."]
    Open,
    #[doc = " The model has recorded conflicting evidence."]
    Contradicted,
    #[doc = " The model reports its investigation concluded; still non-authoritative."]
    Resolved,
    #[doc = " Host invalidation found changed or uncertain dependencies."]
    Stale,
    #[doc = " Another retained record explicitly replaced this entry."]
    Superseded,
}
#[doc = " Canonical source and entry references for a proposed record."]
#[derive(Debug, Eq, PartialEq)]
pub struct WorkingLinks {
    supports: Vec<ObservationId>,
    contradicts: Vec<ObservationId>,
    depends_on: Vec<ContextNodeId>,
}
impl WorkingLinks {
    #[doc = " Checks bounds, strict ordering, and support/contradiction disjointness."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Rejects unbounded, duplicate, unordered, overlapping, or source-free references."]
    pub fn new(
        supports: Vec<ObservationId>,
        contradicts: Vec<ObservationId>,
        depends_on: Vec<ContextNodeId>,
        limits: WorkingLimits,
    ) -> Result<Self, WorkingError> {
        if supports.is_empty() && contradicts.is_empty() {
            return Err(WorkingError::EmptyEntry);
        }
        if supports.len() > limits.links()
            || contradicts.len() > limits.links()
            || depends_on.len() > limits.links()
        {
            return Err(WorkingError::Capacity);
        }
        validate_sources(&supports)?;
        validate_sources(&contradicts)?;
        let mut index = 1;
        while index < depends_on.len() {
            if depends_on[index - 1] >= depends_on[index] {
                return Err(WorkingError::NonCanonicalOrder);
            }
            index += 1;
        }
        let mut support = 0;
        let mut contradiction = 0;
        while support < supports.len() && contradiction < contradicts.len() {
            match supports[support].cmp(&contradicts[contradiction]) {
                core::cmp::Ordering::Equal => return Err(WorkingError::ConflictingEvidence),
                core::cmp::Ordering::Less => support += 1,
                core::cmp::Ordering::Greater => contradiction += 1,
            }
        }
        Ok(Self { supports, contradicts, depends_on })
    }
    #[doc = " Exact supporting observation handles."]
    #[must_use]
    pub const fn supports(&self) -> &[ObservationId] {
        self.supports.as_slice()
    }
    #[doc = " Exact counterevidence handles, retained alongside support."]
    #[must_use]
    pub const fn contradicts(&self) -> &[ObservationId] {
        self.contradicts.as_slice()
    }
    #[doc = " Entry prerequisites, in strict identifier order."]
    #[must_use]
    pub const fn depends_on(&self) -> &[ContextNodeId] {
        self.depends_on.as_slice()
    }
}
impl Clone for WorkingLinks {
    fn clone(&self) -> Self {
        Self {
            supports: self.supports.clone(),
            contradicts: self.contradicts.clone(),
            depends_on: self.depends_on.clone(),
        }
    }
}
#[doc = " Immutable bounded investigation entry. Hosts must redact content before construction."]
#[derive(Debug, Eq, PartialEq)]
pub struct WorkingEntry {
    pub(super) id: ContextNodeId,
    pub(super) kind: WorkingEntryKind,
    pub(super) status: WorkingEntryStatus,
    pub(super) content: ContextContent,
    pub(super) links: WorkingLinks,
    pub(super) validity: WorkingValidity,
    pub(super) host_binding: Option<super::WorkingHostBinding>,
    pub(super) host_declared_status: Option<WorkingEntryStatus>,
    pub(super) supersedes: Option<ContextNodeId>,
    pub(super) stale_through: u64,
}
impl WorkingEntry {
    #[doc = " Clones an entry sequence with complete semantic field preservation."]
    pub(super) fn clone_sequence(entries: &[Self]) -> Vec<Self> {
        let mut result = Vec::with_capacity(entries.len());
        let mut index = 0;
        while index < entries.len() {
            result.push(entries[index].clone());
            index += 1;
        }
        result
    }
    #[doc = " Changes only status fields owned by the host invalidation reducer."]
    pub(super) const fn with_host_status(
        self,
        status: WorkingEntryStatus,
        stale_through: u64,
    ) -> Self {
        let mut result = self;
        result.status = status;
        result.stale_through = stale_through;
        result
    }
    #[doc = " Creates an open, source-backed record under the supplied allocation limits."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Rejects excessive content or references. Source existence and acyclicity are checked on"]
    #[doc = " atomic application to the bound state."]
    pub fn new(
        id: ContextNodeId,
        kind: WorkingEntryKind,
        content: ContextContent,
        links: WorkingLinks,
        validity: WorkingValidity,
        limits: WorkingLimits,
    ) -> Result<Self, WorkingError> {
        if content.len() > limits.entry_bytes()
            || links.supports.len() > limits.links()
            || links.contradicts.len() > limits.links()
            || links.depends_on.len() > limits.links()
            || validity.files().len() > limits.links()
        {
            return Err(WorkingError::Capacity);
        }
        Ok(Self {
            id,
            kind,
            content,
            links,
            validity,
            host_binding: None,
            host_declared_status: None,
            status: WorkingEntryStatus::Open,
            supersedes: None,
            stale_through: 0,
        })
    }
    #[doc = " Changes only model-declared investigation status."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Host-derived stale and superseded states cannot be assigned by an agent update."]
    pub fn with_status(self, status: WorkingEntryStatus) -> Result<Self, WorkingError> {
        if matches!(status, WorkingEntryStatus::Stale | WorkingEntryStatus::Superseded) {
            return Err(WorkingError::DerivedStatus);
        }
        let mut entry = self;
        entry.status = status;
        Ok(entry)
    }
    #[doc = " Explicitly replaces another retained entry, leaving its source evidence intact."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Rejects self-supersession; target existence and graph checks occur on application."]
    pub fn with_supersedes(self, previous: ContextNodeId) -> Result<Self, WorkingError> {
        if previous == self.id {
            return Err(WorkingError::DependencyCycle);
        }
        let mut entry = self;
        entry.supersedes = Some(previous);
        Ok(entry)
    }
    #[doc = " Stable identifier within this working model."]
    #[must_use]
    pub const fn id(&self) -> ContextNodeId {
        self.id
    }
    #[doc = " Semantic entry purpose."]
    #[must_use]
    pub const fn kind(&self) -> WorkingEntryKind {
        self.kind
    }
    #[doc = " Current investigation or host-derived status."]
    #[must_use]
    pub const fn status(&self) -> WorkingEntryStatus {
        self.status
    }
    #[doc = " Exact bounded content and digest."]
    #[must_use]
    pub const fn content(&self) -> &ContextContent {
        &self.content
    }
    #[doc = " Supporting, contradicting, and prerequisite references."]
    #[must_use]
    pub const fn links(&self) -> &WorkingLinks {
        &self.links
    }
    #[doc = " Explicit dependency constraints."]
    #[must_use]
    pub const fn validity(&self) -> &WorkingValidity {
        &self.validity
    }
    /// Host-recorded source closure, separate from model-proposed provenance links.
    #[must_use]
    pub const fn host_binding(&self) -> Option<super::WorkingHostBinding> {
        self.host_binding
    }
    /// Original investigation status, separate from effective host freshness.
    /// Legacy stale/superseded entries may have no recoverable declared status.
    #[must_use]
    pub const fn declared_status(&self) -> Option<WorkingEntryStatus> {
        if self.host_binding.is_some() {
            self.host_declared_status
        } else {
            match self.status {
                WorkingEntryStatus::Stale | WorkingEntryStatus::Superseded => None,
                status => Some(status),
            }
        }
    }
    #[doc = " Earlier record replaced by this one."]
    #[must_use]
    pub const fn supersedes(&self) -> Option<ContextNodeId> {
        self.supersedes
    }
    #[doc = " Observation frontier recorded when the host derived stale status."]
    #[must_use]
    pub const fn stale_through(&self) -> u64 {
        self.stale_through
    }
    #[doc = " Every working entry remains non-authoritative, regardless of model-assigned status."]
    #[must_use]
    pub const fn authority(&self) -> AuthorityClass {
        AuthorityClass::NonAuthoritative
    }
}
impl Clone for WorkingEntry {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            kind: self.kind,
            status: self.status,
            content: self.content.clone(),
            links: self.links.clone(),
            validity: self.validity.clone(),
            host_binding: self.host_binding,
            host_declared_status: self.host_declared_status,
            supersedes: self.supersedes,
            stale_through: self.stale_through,
        }
    }
}
fn validate_sources(sources: &[ObservationId]) -> Result<(), WorkingError> {
    let mut index = 1;
    while index < sources.len() {
        if sources[index - 1] >= sources[index] {
            return Err(WorkingError::NonCanonicalOrder);
        }
        index += 1;
    }
    Ok(())
}
