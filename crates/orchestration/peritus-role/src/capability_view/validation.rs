//! Per-position validation used by the production capability constructor.
use super::operation_rank;

use crate::{RoleError, RoleErrorKind};
use peritus_policy::{ActorRole, OperationClass};

pub(super) fn validate_operation_at(
    role: ActorRole,
    operations: &[OperationClass],
    index: usize,
) -> Result<(), RoleError> {
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
