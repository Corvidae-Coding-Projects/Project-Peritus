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
    /// Complete stored failure category.
    pub closed spec fn spec_kind(&self) -> RoleErrorKind { self.kind }
    /// Complete optional context-class payload.
    pub closed spec fn spec_context_class(&self) -> Option<ContextClass> { self.context_class }
    /// Complete optional operation payload.
    pub closed spec fn spec_operation(&self) -> Option<OperationClass> { self.operation }
    /// Exact error with no contextual payload.
    pub open spec fn spec_plain(&self, kind: RoleErrorKind) -> bool {
        self.spec_kind() == kind && self.spec_context_class().is_none()
            && self.spec_operation().is_none()
    }
    /// Exact error concerning a context class and no operation.
    pub open spec fn spec_class_error(&self, kind: RoleErrorKind, class: ContextClass) -> bool {
        self.spec_kind() == kind && self.spec_context_class() == Some(class)
            && self.spec_operation().is_none()
    }
    /// Exact error concerning an operation and no context class.
    pub open spec fn spec_operation_error(&self, kind: RoleErrorKind, operation: OperationClass) -> bool {
        self.spec_kind() == kind && self.spec_context_class().is_none()
            && self.spec_operation() == Some(operation)
    }

    pub(crate) const fn empty_collection() -> (error: Self)
        ensures error.spec_plain(RoleErrorKind::EmptyCollection),
    {
        Self { kind: RoleErrorKind::EmptyCollection, context_class: None, operation: None }
    }

    pub(crate) const fn context_class(kind: RoleErrorKind, context_class: ContextClass) -> (error: Self)
        ensures error.spec_class_error(kind, context_class),
    {
        Self { kind, context_class: Some(context_class), operation: None }
    }

    pub(crate) const fn operation(kind: RoleErrorKind, operation: OperationClass) -> (error: Self)
        ensures error.spec_operation_error(kind, operation),
    {
        Self { kind, context_class: None, operation: Some(operation) }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> (value: RoleErrorKind)
        ensures value == self.spec_kind(),
    { self.kind }

    /// Returns the offending context class, when the failure concerns one.
    #[must_use]
    pub const fn context_class_value(&self) -> (value: Option<ContextClass>)
        ensures value == self.spec_context_class(),
    { self.context_class }

    /// Returns the offending operation, when the failure concerns one.
    #[must_use]
    pub const fn operation_value(&self) -> (value: Option<OperationClass>)
        ensures value == self.spec_operation(),
    { self.operation }
}

} // verus!
