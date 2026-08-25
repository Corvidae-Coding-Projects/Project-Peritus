//! Checked durable memory scopes and explicit compatibility policy.

use crate::{MemoryError, MemoryErrorKind, MemoryField, RepositoryId};
use peritus_policy::ActorRole;
use peritus_types::{ActorId, ProjectId, WorkspaceId};
use vstd::prelude::*;

verus! {

/// Primary scope dimension used to describe the intended retention boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ScopeKind {
    /// Shared within one project.
    Project,
    /// Shared within one workspace.
    Workspace,
    /// Shared within one repository identity.
    Repository,
    /// Further restricted to one actor identity.
    Actor,
    /// Further restricted to one canonical security role.
    Role,
}

/// Explicit query-to-record scope compatibility behavior.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ScopePolicy {
    /// Every scope field and the scope kind must be identical.
    Exact,
    /// A query may use a record whose set dimensions are a matching subset of the query.
    IncludeBroader,
}

/// Immutable scoped-memory boundary with caller-supplied durable identities.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MemoryScope {
    kind: ScopeKind,
    project: Option<ProjectId>,
    workspace: Option<WorkspaceId>,
    repository: Option<RepositoryId>,
    actor: Option<ActorId>,
    role: Option<ActorRole>,
}

impl MemoryScope {
    /// Creates a scope and checks durable and kind-specific dimensions.
    ///
    /// At least one project, workspace, or repository identity is mandatory. Actor and role
    /// dimensions only narrow a durable scope; they cannot create an ambient global scope.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryErrorKind::IncompleteScope`] when durable or kind-specific data is absent.
    pub const fn new(
        kind: ScopeKind,
        project: Option<ProjectId>,
        workspace: Option<WorkspaceId>,
        repository: Option<RepositoryId>,
        actor: Option<ActorId>,
        role: Option<ActorRole>,
    ) -> Result<Self, MemoryError> {
        if project.is_none() && workspace.is_none() && repository.is_none() {
            return Err(MemoryError::field(MemoryErrorKind::EmptyValue, MemoryField::Scope));
        }
        let complete = match kind {
            ScopeKind::Project => project.is_some(),
            ScopeKind::Workspace => workspace.is_some(),
            ScopeKind::Repository => repository.is_some(),
            ScopeKind::Actor => actor.is_some(),
            ScopeKind::Role => role.is_some(),
        };
        if !complete {
            return Err(MemoryError::field(MemoryErrorKind::IncompleteScope, MemoryField::Scope));
        }
        Ok(Self { kind, project, workspace, repository, actor, role })
    }

    /// Returns the declared primary scope kind.
    #[must_use]
    pub const fn kind(&self) -> ScopeKind { self.kind }

    /// Returns the project dimension.
    #[must_use]
    pub const fn project(&self) -> Option<ProjectId> { self.project }

    /// Returns the workspace dimension.
    #[must_use]
    pub const fn workspace(&self) -> Option<WorkspaceId> { self.workspace }

    /// Returns the repository dimension.
    #[must_use]
    pub const fn repository(&self) -> Option<RepositoryId> { self.repository }

    /// Returns the actor restriction.
    #[must_use]
    pub const fn actor(&self) -> Option<ActorId> { self.actor }

    /// Returns the role restriction.
    #[must_use]
    pub const fn role(&self) -> Option<ActorRole> { self.role }

    /// Returns whether this record scope is eligible for `query` under an explicit policy.
    #[must_use]
    pub fn compatible_with(&self, query: &Self, policy: ScopePolicy) -> bool {
        if policy == ScopePolicy::Exact {
            return self == query;
        }
        project_matches(self.project, query.project)
            && workspace_matches(self.workspace, query.workspace)
            && repository_matches(self.repository, query.repository)
            && actor_matches(self.actor, query.actor)
            && role_matches(self.role, query.role)
    }

    /// Returns specificity in basis points from the number of bound dimensions.
    #[must_use]
    pub const fn specificity(&self) -> u16 {
        let mut count = 0_u16;
        if self.project.is_some() { count += 1; }
        if self.workspace.is_some() { count += 1; }
        if self.repository.is_some() { count += 1; }
        if self.actor.is_some() { count += 1; }
        if self.role.is_some() { count += 1; }
        count * 2_000
    }
}

fn project_matches(record: Option<ProjectId>, query: Option<ProjectId>) -> bool {
    record.is_none() || record == query
}

fn workspace_matches(record: Option<WorkspaceId>, query: Option<WorkspaceId>) -> bool {
    record.is_none() || record == query
}

fn repository_matches(record: Option<RepositoryId>, query: Option<RepositoryId>) -> bool {
    record.is_none() || record == query
}

fn actor_matches(record: Option<ActorId>, query: Option<ActorId>) -> bool {
    record.is_none() || record == query
}

fn role_matches(record: Option<ActorRole>, query: Option<ActorRole>) -> bool {
    record.is_none() || record == query
}

} // verus!
