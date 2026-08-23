//! Immutable content references that define contract intent and operating scope.

use crate::ContentReference;
use vstd::prelude::*;

verus! {

/// Required human-authored documents and policy references for an acceptance contract.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ContractDocuments {
    objective: ContentReference,
    user_visible_behavior: ContentReference,
    repository_roots: ContentReference,
    permitted_change_surface: ContentReference,
    resource_budget_policy: ContentReference,
    security_approval_policy: ContentReference,
    completion_conditions: ContentReference,
    failure_conditions: ContentReference,
}

impl ContractDocuments {
    /// Creates an explicit immutable reference set for every contract document class.
    #[allow(clippy::too_many_arguments, reason = "every required contract document remains explicit")]
    #[must_use]
    pub const fn new(
        objective: ContentReference,
        user_visible_behavior: ContentReference,
        repository_roots: ContentReference,
        permitted_change_surface: ContentReference,
        resource_budget_policy: ContentReference,
        security_approval_policy: ContentReference,
        completion_conditions: ContentReference,
        failure_conditions: ContentReference,
    ) -> Self {
        Self {
            objective,
            user_visible_behavior,
            repository_roots,
            permitted_change_surface,
            resource_budget_policy,
            security_approval_policy,
            completion_conditions,
            failure_conditions,
        }
    }

    /// Returns the human-authored objective.
    #[must_use]
    pub const fn objective(&self) -> ContentReference { self.objective }

    /// Returns the required user-visible behavior document.
    #[must_use]
    pub const fn user_visible_behavior(&self) -> ContentReference { self.user_visible_behavior }

    /// Returns the repository-root declaration.
    #[must_use]
    pub const fn repository_roots(&self) -> ContentReference { self.repository_roots }

    /// Returns the permitted change-surface declaration.
    #[must_use]
    pub const fn permitted_change_surface(&self) -> ContentReference {
        self.permitted_change_surface
    }

    /// Returns the resource and budget policy reference.
    #[must_use]
    pub const fn resource_budget_policy(&self) -> ContentReference { self.resource_budget_policy }

    /// Returns the security and approval policy reference.
    #[must_use]
    pub const fn security_approval_policy(&self) -> ContentReference {
        self.security_approval_policy
    }

    /// Returns the explicit completion-condition declaration.
    #[must_use]
    pub const fn completion_conditions(&self) -> ContentReference { self.completion_conditions }

    /// Returns the explicit failure-condition declaration.
    #[must_use]
    pub const fn failure_conditions(&self) -> ContentReference { self.failure_conditions }
}

} // verus!
