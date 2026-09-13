//! Production traversal for canonical metadata dependencies.

#[cfg(verus_only)]
use super::model;
use crate::{ContextError, ContextErrorKind, ContextNodeId};
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

pub(super) fn validate_dependencies(
    id: ContextNodeId,
    dependencies: &[ContextNodeId],
) -> (result: Result<(), ContextError>)
    ensures
        result.is_ok() == model::first_dependency_error(id, dependencies@, 0).is_none(),
        match result {
            Ok(()) => true,
            Err(error) => model::dependency_error(id, dependencies@, error),
        },
{
    let mut index = 0;
    while index < dependencies.len()
        invariant
            index <= dependencies.len(),
            model::first_dependency_error(id, dependencies@, 0)
                == model::first_dependency_error(id, dependencies@, index as nat),
        decreases dependencies.len() - index,
    {
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

} // verus!
