//! Read-only operation views that can only narrow B1 role permissions.

use crate::RoleError;

use peritus_policy::{ActorRole, OperationClass};

mod validation;

#[doc = " Canonically ordered operation classes exposed to one role profile."]
#[derive(Debug, Eq, PartialEq)]
pub struct CapabilityView {
    role: ActorRole,
    operations: Vec<OperationClass>,
}
impl CapabilityView {
    #[doc = " Creates a checked, non-widening view."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed error for an empty, duplicate, unordered, or B1-denied operation."]
    pub fn new(role: ActorRole, operations: Vec<OperationClass>) -> Result<Self, RoleError> {
        if operations.is_empty() {
            return Err(RoleError::empty_collection());
        }
        let mut index = 0;
        while index < operations.len() {
            validation::validate_operation_at(role, operations.as_slice(), index)?;
            index += 1;
        }
        let view = Self { role, operations };
        Ok(view)
    }
    pub(crate) fn for_role(role: ActorRole) -> Self {
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

        Self { role, operations }
    }
    #[doc = " Returns the underlying canonical B1 role."]
    #[must_use]
    pub const fn role(&self) -> ActorRole {
        self.role
    }
    #[doc = " Returns the exposed operation classes in canonical order."]
    #[must_use]
    pub const fn operations(&self) -> &[OperationClass] {
        self.operations.as_slice()
    }
    #[doc = " Returns whether the view exposes an operation."]
    #[must_use]
    pub fn permits(&self, operation: OperationClass) -> bool {
        let mut index = 0;
        while index < self.operations.len() {
            if operation_rank(self.operations[index]) == operation_rank(operation) {
                return true;
            }
            index += 1;
        }
        false
    }
    #[doc = " Returns true exactly when every operation in the view remains B1-permitted."]
    #[must_use]
    pub fn is_narrow(&self) -> bool {
        let mut index = 0;
        while index < self.operations.len() {
            let operation = self.operations[index];
            if !self.role.permits_operation(operation) {
                return false;
            }
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
impl Clone for CapabilityView {
    fn clone(&self) -> Self {
        Self { role: self.role, operations: self.operations.clone() }
    }
}
