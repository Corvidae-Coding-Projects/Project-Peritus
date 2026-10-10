//! Production traversal for canonical metadata dependencies.

use crate::{ContextError, ContextErrorKind, ContextNodeId};
use core::cmp::Ordering;

pub(super) fn validate_dependencies(
    id: ContextNodeId,
    dependencies: &[ContextNodeId],
) -> Result<(), ContextError> {
    let mut index = 0;
    while index < dependencies.len() {
        if dependencies[index].matches(&id) {
            return Err(ContextError::nodes(ContextErrorKind::SelfDependency, id, id));
        }
        if index > 0 {
            match dependencies[index - 1].canonical_order(&dependencies[index]) {
                Ordering::Equal => {
                    return Err(ContextError::nodes(
                        ContextErrorKind::DuplicateValue,
                        id,
                        dependencies[index],
                    ));
                }
                Ordering::Greater => {
                    return Err(ContextError::nodes(
                        ContextErrorKind::NonCanonicalOrder,
                        id,
                        dependencies[index],
                    ));
                }
                Ordering::Less => {}
            }
        }
        index += 1;
    }
    Ok(())
}
