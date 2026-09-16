//! Per-position validation used by the production capability constructor.
use super::operation_rank;
#[cfg(verus_only)]
use super::CapabilityView;
use crate::{RoleError, RoleErrorKind};
use peritus_policy::{ActorRole, OperationClass};
use vstd::prelude::*;

verus! {

pub(super) fn validate_operation_at(
    role: ActorRole,
    operations: &[OperationClass],
    index: usize,
) -> (result: Result<(), RoleError>)
    requires index < operations.len(),
    ensures
        result.is_ok() == CapabilityView::spec_operation_valid(role, operations@, index as int),
        match result {
            Err(error) => CapabilityView::spec_error_at(role, operations@, index as int, error),
            Ok(()) => true,
        },
{
    let operation = operations[index];
    if !role.permits_operation(operation) {
        return Err(RoleError::operation(RoleErrorKind::OperationNotPermitted, operation));
    }
    if index > 0 {
        if operation_rank(operations[index - 1]) == operation_rank(operation) {
            return Err(RoleError::operation(RoleErrorKind::DuplicateValue, operation));
        }
        if operation_rank(operations[index - 1]) > operation_rank(operation) {
            return Err(RoleError::operation(RoleErrorKind::NonCanonicalOrder, operation));
        }
    }
    Ok(())
}

} // verus!
