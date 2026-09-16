//! Read-only operation views that can only narrow B1 role permissions.

use crate::RoleError;
#[cfg(verus_only)]
use crate::RoleErrorKind;
use peritus_policy::{ActorRole, OperationClass};
use vstd::prelude::*;

mod validation;

verus! {

/// Canonically ordered operation classes exposed to one role profile.
#[derive(Debug, Eq, PartialEq)]
pub struct CapabilityView {
    role: ActorRole,
    operations: Vec<OperationClass>,
}

impl CapabilityView {
    /// Complete input admission over exact permissions and canonical operation order.
    pub open spec fn spec_valid(role: ActorRole, operations: Seq<OperationClass>) -> bool {
        operations.len() > 0 && Self::spec_valid_through(role, operations, operations.len() as int)
    }
    /// One supplied operation is permitted and follows its predecessor canonically.
    pub open spec fn spec_operation_valid(
        role: ActorRole, operations: Seq<OperationClass>, index: int,
    ) -> bool {
        role.spec_permits_operation(operations[index])
            && (index > 0 ==> operation_rank_spec(operations[index - 1])
                < operation_rank_spec(operations[index]))
    }
    /// Every admitted operation and adjacent pair in a supplied input prefix.
    pub open spec fn spec_valid_through(
        role: ActorRole, operations: Seq<OperationClass>, end: int,
    ) -> bool {
        0 <= end <= operations.len()
            && forall |index: int| 0 <= index < end ==>
                Self::spec_operation_valid(role, operations, index)
    }
    /// Exact error at a supplied position, with permission checked before canonicality.
    pub open spec fn spec_error_at(
        role: ActorRole, operations: Seq<OperationClass>, index: int, error: RoleError,
    ) -> bool {
        if !role.spec_permits_operation(operations[index]) {
            error.spec_operation_error(RoleErrorKind::OperationNotPermitted, operations[index])
        } else {
            index > 0 && if operations[index - 1] == operations[index] {
                error.spec_operation_error(RoleErrorKind::DuplicateValue, operations[index])
            } else {
                operation_rank_spec(operations[index - 1]) > operation_rank_spec(operations[index])
                    && error.spec_operation_error(RoleErrorKind::NonCanonicalOrder, operations[index])
            }
        }
    }
    /// Exact first input error and all optional error payload fields.
    pub open spec fn spec_error(
        role: ActorRole, operations: Seq<OperationClass>, error: RoleError,
    ) -> bool {
        if operations.len() == 0 {
            error.spec_plain(RoleErrorKind::EmptyCollection)
        } else {
            exists |index: int| 0 <= index < operations.len()
                && Self::spec_valid_through(role, operations, index)
                && Self::spec_error_at(role, operations, index, error)
        }
    }
    /// Exact ordered operation projection for each canonical security role.
    pub open spec fn spec_role_operations(role: ActorRole) -> Seq<OperationClass> {
        match role {
            ActorRole::Writer | ActorRole::Fixer => [OperationClass::Inspection, OperationClass::WorkspaceMutation, OperationClass::Execution, OperationClass::Network, OperationClass::DependencyEnvironment, OperationClass::RepositoryHistoryMutation, OperationClass::SecretUse, OperationClass::ExternalSideEffect]@,
            ActorRole::Reviewer | ActorRole::Plugin | ActorRole::HumanAuthority | ActorRole::ProviderToolWorker => Seq::empty().push(OperationClass::Inspection),
            ActorRole::Evaluator | ActorRole::GateRunner | ActorRole::Orchestrator | ActorRole::DaemonService => [OperationClass::Inspection, OperationClass::Execution]@,
            ActorRole::EvolutionAgent => [OperationClass::Inspection, OperationClass::WorkspaceMutation, OperationClass::Execution, OperationClass::Network, OperationClass::DependencyEnvironment]@,
        }
    }
    /// Complete semantic equality of stored capability projections.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.spec_role() == right.spec_role() && left.spec_operations() == right.spec_operations()
    }

    /// Returns the exact ordered operation sequence used by specifications.
    pub closed spec fn spec_operations(&self) -> Seq<OperationClass> { self.operations@ }

    /// Returns the exact B1 role used by specifications.
    pub closed spec fn spec_role(&self) -> ActorRole { self.role }

    /// Returns whether every projected operation remains permitted by B1.
    pub open spec fn spec_is_narrow(&self) -> bool {
        forall |index: int| 0 <= index < self.spec_operations().len() ==>
            self.spec_role().spec_permits_operation(#[trigger] self.spec_operations()[index])
    }

    /// Creates a checked, non-widening view.
    ///
    /// # Errors
    ///
    /// Returns a typed error for an empty, duplicate, unordered, or B1-denied operation.
    pub fn new(
        role: ActorRole,
        operations: Vec<OperationClass>,
    ) -> (result: Result<Self, RoleError>)
        ensures
            result.is_ok() == Self::spec_valid(role, operations@),
            match result { Err(error) => Self::spec_error(role, operations@, error), Ok(_) => true },
            result.is_ok() ==> result.unwrap().spec_is_narrow(),
            result.is_ok() ==> result.unwrap().spec_role() == role,
            result.is_ok() ==> result.unwrap().spec_operations() == operations@,
    {
        if operations.is_empty() {
            return Err(RoleError::empty_collection());
        }
        let mut index = 0;
        while index < operations.len()
            invariant
                index <= operations.len(),
                Self::spec_valid_through(role, operations@, index as int),
            decreases operations.len() - index,
        {
            if let Err(error) = validation::validate_operation_at(role, operations.as_slice(), index) {
                assert(Self::spec_error(role, operations@, error)) by {
                    let first = index as int;
                }
                return Err(error);
            }
            assert(Self::spec_valid_through(role, operations@, index as int + 1));
            index += 1;
        }
        assert forall |prior: int| 0 <= prior < operations.len() implies
            role.spec_permits_operation(#[trigger] operations@[prior]) by {
            assert(Self::spec_operation_valid(role, operations@, prior));
        }
        let view = Self { role, operations };
        reveal(CapabilityView::spec_operations);
        Ok(view)
    }

    pub(crate) fn for_role(role: ActorRole) -> (result: Self)
        ensures result.spec_is_narrow(), result.spec_role() == role,
            result.spec_operations() == Self::spec_role_operations(role),
    {
        let operations = match role {
            ActorRole::Writer | ActorRole::Fixer => vec![
                OperationClass::Inspection,
                OperationClass::WorkspaceMutation,
                OperationClass::Execution,
                OperationClass::Network,
                OperationClass::DependencyEnvironment,
                OperationClass::RepositoryHistoryMutation,
                OperationClass::SecretUse,
                OperationClass::ExternalSideEffect,
            ],
            ActorRole::Reviewer | ActorRole::Plugin | ActorRole::HumanAuthority => {
                vec![OperationClass::Inspection]
            }
            ActorRole::Evaluator
            | ActorRole::GateRunner
            | ActorRole::Orchestrator
            | ActorRole::DaemonService => {
                vec![OperationClass::Inspection, OperationClass::Execution]
            }
            ActorRole::EvolutionAgent => vec![
                OperationClass::Inspection,
                OperationClass::WorkspaceMutation,
                OperationClass::Execution,
                OperationClass::Network,
                OperationClass::DependencyEnvironment,
            ],
            ActorRole::ProviderToolWorker => vec![OperationClass::Inspection],
        };
        let result = Self { role, operations };
        reveal(CapabilityView::spec_operations);
        result
    }

    /// Returns the underlying canonical B1 role.
    #[must_use]
    pub const fn role(&self) -> (role: ActorRole)
        ensures role == self.spec_role(),
    { self.role }

    /// Returns the exposed operation classes in canonical order.
    #[must_use]
    pub const fn operations(&self) -> (operations: &[OperationClass])
        ensures operations@ == self.spec_operations(),
    {
        reveal(CapabilityView::spec_operations);
        self.operations.as_slice()
    }

    /// Returns whether the view exposes an operation.
    #[must_use]
    pub fn permits(&self, operation: OperationClass) -> (permitted: bool)
        ensures permitted == self.spec_operations().contains(operation),
    {
        proof { reveal(CapabilityView::spec_operations); }
        let mut index = 0;
        while index < self.operations.len()
            invariant
                index <= self.operations.len(),
                forall |prior: int| 0 <= prior < index ==>
                    self.operations@[prior] != operation,
            decreases self.operations.len() - index,
        {
            if operation_rank(self.operations[index]) == operation_rank(operation) {
                return true;
            }
            index += 1;
        }
        false
    }

    /// Returns true exactly when every operation in the view remains B1-permitted.
    #[must_use]
    pub fn is_narrow(&self) -> (result: bool)
        ensures result == self.spec_is_narrow(),
    {
        proof {
            reveal(CapabilityView::spec_operations);
            reveal(CapabilityView::spec_role);
        }
        let mut index = 0;
        while index < self.operations.len()
            invariant
                index <= self.operations.len(),
                forall |prior: int| 0 <= prior < index ==>
                    self.role.spec_permits_operation(#[trigger] self.operations@[prior]),
            decreases self.operations.len() - index,
        {
            let operation = self.operations[index];
            if !self.role.permits_operation(operation) {
                assert(operation == self.operations@[index as int]);
                assert(!self.role.spec_permits_operation(operation));
                assert(!self.spec_is_narrow()) by {
                    reveal(CapabilityView::spec_is_narrow);
                    assert(self.spec_operations()[index as int] == operation);
                    assert(self.spec_role() == self.role);
                    assert(exists |found: int| found == index
                        && 0 <= found < self.operations@.len()
                        && !self.spec_role().spec_permits_operation(
                            #[trigger] self.spec_operations()[found]
                        ));
                }
                return false;
            }
            assert(self.role.spec_permits_operation(self.operations@[index as int]));
            index += 1;
        }
        true
    }
}

pub open spec fn operation_rank_spec(operation: OperationClass) -> u8 {
    match operation {
        OperationClass::Inspection => 0,
        OperationClass::WorkspaceMutation => 1,
        OperationClass::Execution => 2,
        OperationClass::Network => 3,
        OperationClass::DependencyEnvironment => 4,
        OperationClass::RepositoryHistoryMutation => 5,
        OperationClass::SecretUse => 6,
        OperationClass::ExternalSideEffect => 7,
        OperationClass::Acceptance => 8,
        OperationClass::Waiver => 9,
        OperationClass::PolicyAmendment => 10,
        OperationClass::HarnessPromotion => 11,
        OperationClass::HumanAuthority => 12,
        OperationClass::RawEffect => 13,
    }
}

const fn operation_rank(operation: OperationClass) -> (rank: u8)
    ensures rank == operation_rank_spec(operation),
{
    match operation {
        OperationClass::Inspection => 0,
        OperationClass::WorkspaceMutation => 1,
        OperationClass::Execution => 2,
        OperationClass::Network => 3,
        OperationClass::DependencyEnvironment => 4,
        OperationClass::RepositoryHistoryMutation => 5,
        OperationClass::SecretUse => 6,
        OperationClass::ExternalSideEffect => 7,
        OperationClass::Acceptance => 8,
        OperationClass::Waiver => 9,
        OperationClass::PolicyAmendment => 10,
        OperationClass::HarnessPromotion => 11,
        OperationClass::HumanAuthority => 12,
        OperationClass::RawEffect => 13,
    }
}

impl Clone for CapabilityView {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    { Self { role: self.role, operations: self.operations.clone() } }
}

} // verus!
