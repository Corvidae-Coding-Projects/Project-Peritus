//! Typed role-policy construction failures.

use crate::ContextClass;
use peritus_policy::OperationClass;
use vstd::prelude::*;

verus! {

/// Stable category for a role-policy failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RoleErrorKind {
    /// A collection is empty where at least one value is required.
    EmptyCollection,
    /// A collection is not in canonical strictly increasing order.
    NonCanonicalOrder,
    /// A collection contains a duplicate.
    DuplicateValue,
    /// An operation would widen the B1 security role.
    OperationNotPermitted,
}

/// Checked role-policy error with the relevant value when available.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoleError {
    kind: RoleErrorKind,
    context_class: Option<ContextClass>,
    operation: Option<OperationClass>,
}

impl RoleError {
    pub(crate) const fn empty_collection() -> Self {
        Self { kind: RoleErrorKind::EmptyCollection, context_class: None, operation: None }
    }

    pub(crate) const fn context_class(kind: RoleErrorKind, context_class: ContextClass) -> Self {
        Self { kind, context_class: Some(context_class), operation: None }
    }

    pub(crate) const fn operation(kind: RoleErrorKind, operation: OperationClass) -> Self {
        Self { kind, context_class: None, operation: Some(operation) }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> RoleErrorKind { self.kind }

    /// Returns the offending context class, when the failure concerns one.
    #[must_use]
    pub const fn context_class_value(&self) -> Option<ContextClass> { self.context_class }

    /// Returns the offending operation, when the failure concerns one.
    #[must_use]
    pub const fn operation_value(&self) -> Option<OperationClass> { self.operation }
}

} // verus!
