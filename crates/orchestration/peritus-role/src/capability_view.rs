//! Read-only operation views that can only narrow B1 role permissions.

use crate::{RoleError, RoleErrorKind};
use peritus_policy::{ActorRole, OperationClass};
use vstd::prelude::*;

verus! {

/// Canonically ordered operation classes exposed to one role profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityView {
    role: ActorRole,
    operations: Vec<OperationClass>,
}

impl CapabilityView {
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
        ensures result.is_ok() ==> result.unwrap().spec_is_narrow(),
    {
        if operations.is_empty() {
            return Err(RoleError::empty_collection());
        }
        let mut index = 0;
        while index < operations.len()
            invariant
                index <= operations.len(),
                forall |prior: int| 0 <= prior < index ==>
                    role.spec_permits_operation(#[trigger] operations@[prior]),
            decreases operations.len() - index,
        {
            let operation = operations[index];
            if !role.permits_operation(operation) {
                return Err(RoleError::operation(RoleErrorKind::OperationNotPermitted, operation));
            }
            if index > 0 {
                if operations[index - 1] == operation {
                    return Err(RoleError::operation(RoleErrorKind::DuplicateValue, operation));
                }
                if operation_rank(operations[index - 1]) > operation_rank(operation) {
                    return Err(RoleError::operation(RoleErrorKind::NonCanonicalOrder, operation));
                }
            }
            index += 1;
        }
        let view = Self { role, operations };
        reveal(CapabilityView::spec_operations);
        Ok(view)
    }

    pub(crate) fn for_role(role: ActorRole) -> (result: Self)
        ensures result.spec_is_narrow(),
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
    pub const fn role(&self) -> ActorRole { self.role }

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
    pub fn permits(&self, operation: OperationClass) -> bool {
        let mut index = 0;
        while index < self.operations.len()
            invariant index <= self.operations.len(),
            decreases self.operations.len() - index,
        {
            if self.operations[index] == operation {
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

const fn operation_rank(operation: OperationClass) -> u8 {
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

} // verus!
