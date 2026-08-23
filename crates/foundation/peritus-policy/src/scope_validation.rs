//! Exact canonical validation for selector identity collections.

use crate::{identity::compare_identifier_bytes, ActorRole, CanonicalCollection, PolicyError};
use core::cmp::Ordering;
use peritus_types::{ActorId, EnvironmentId};
use vstd::prelude::*;

verus! {

pub open spec fn first_actor_order_error(
    values: Seq<ActorId>,
    index: nat,
) -> Option<crate::PolicyErrorKind>
    decreases values.len() - index,
{
    if index >= values.len() {
        None
    } else {
        match peritus_types::canonical_byte_order_from(
            values[index as int - 1].spec_bytes()@,
            values[index as int].spec_bytes()@,
            0,
        ) {
            Ordering::Less => first_actor_order_error(values, index + 1),
            Ordering::Equal => Some(crate::PolicyErrorKind::DuplicateCanonicalValue),
            Ordering::Greater => Some(crate::PolicyErrorKind::NonCanonicalOrder),
        }
    }
}

pub open spec fn actor_validation_error(values: Seq<ActorId>) -> Option<crate::PolicyErrorKind> {
    if values.len() == 0 {
        Some(crate::PolicyErrorKind::EmptyCanonicalCollection)
    } else {
        first_actor_order_error(values, 1)
    }
}

pub open spec fn first_environment_order_error(
    values: Seq<EnvironmentId>,
    index: nat,
) -> Option<crate::PolicyErrorKind>
    decreases values.len() - index,
{
    if index >= values.len() {
        None
    } else {
        match peritus_types::canonical_byte_order_from(
            values[index as int - 1].spec_bytes()@,
            values[index as int].spec_bytes()@,
            0,
        ) {
            Ordering::Less => first_environment_order_error(values, index + 1),
            Ordering::Equal => Some(crate::PolicyErrorKind::DuplicateCanonicalValue),
            Ordering::Greater => Some(crate::PolicyErrorKind::NonCanonicalOrder),
        }
    }
}

pub open spec fn environment_validation_error(
    values: Seq<EnvironmentId>,
) -> Option<crate::PolicyErrorKind> {
    if values.len() == 0 {
        Some(crate::PolicyErrorKind::EmptyCanonicalCollection)
    } else {
        first_environment_order_error(values, 1)
    }
}

pub open spec fn first_role_order_error(
    values: Seq<ActorRole>,
    index: nat,
) -> Option<crate::PolicyErrorKind>
    decreases values.len() - index,
{
    if index >= values.len() {
        None
    } else if values[index as int - 1].spec_rank() == values[index as int].spec_rank() {
        Some(crate::PolicyErrorKind::DuplicateCanonicalValue)
    } else if values[index as int - 1].spec_rank() > values[index as int].spec_rank() {
        Some(crate::PolicyErrorKind::NonCanonicalOrder)
    } else {
        first_role_order_error(values, index + 1)
    }
}

pub open spec fn role_validation_error(values: Seq<ActorRole>) -> Option<crate::PolicyErrorKind> {
    if values.len() == 0 {
        Some(crate::PolicyErrorKind::EmptyCanonicalCollection)
    } else {
        first_role_order_error(values, 1)
    }
}

pub fn validate_actor_values(values: &[ActorId]) -> (result: Result<(), PolicyError>)
    ensures validation_result_is_exact(result, actor_validation_error(values@), CanonicalCollection::Actors),
{
    if values.is_empty() {
        return Err(PolicyError::empty_canonical_collection(CanonicalCollection::Actors));
    }
    let mut index = 1;
    while index < values.len()
        invariant
            1 <= index <= values.len(),
            values@.len() > 0,
            first_actor_order_error(values@, 1) == first_actor_order_error(values@, index as nat),
        decreases values.len() - index,
    {
        match compare_identifier_bytes(values[index - 1].as_bytes(), values[index].as_bytes()) {
            Ordering::Less => {},
            Ordering::Equal => return Err(PolicyError::duplicate_canonical_value(CanonicalCollection::Actors)),
            Ordering::Greater => return Err(PolicyError::non_canonical_order(CanonicalCollection::Actors)),
        }
        index += 1;
    }
    Ok(())
}

pub fn validate_environment_values(
    values: &[EnvironmentId],
) -> (result: Result<(), PolicyError>)
    ensures validation_result_is_exact(
        result,
        environment_validation_error(values@),
        CanonicalCollection::Environments,
    ),
{
    if values.is_empty() {
        return Err(PolicyError::empty_canonical_collection(CanonicalCollection::Environments));
    }
    let mut index = 1;
    while index < values.len()
        invariant
            1 <= index <= values.len(),
            values@.len() > 0,
            first_environment_order_error(values@, 1)
                == first_environment_order_error(values@, index as nat),
        decreases values.len() - index,
    {
        match compare_identifier_bytes(values[index - 1].as_bytes(), values[index].as_bytes()) {
            Ordering::Less => {},
            Ordering::Equal => return Err(PolicyError::duplicate_canonical_value(CanonicalCollection::Environments)),
            Ordering::Greater => return Err(PolicyError::non_canonical_order(CanonicalCollection::Environments)),
        }
        index += 1;
    }
    Ok(())
}

pub fn validate_role_values(values: &[ActorRole]) -> (result: Result<(), PolicyError>)
    ensures validation_result_is_exact(result, role_validation_error(values@), CanonicalCollection::Roles),
{
    if values.is_empty() {
        return Err(PolicyError::empty_canonical_collection(CanonicalCollection::Roles));
    }
    let mut index = 1;
    while index < values.len()
        invariant
            1 <= index <= values.len(),
            values@.len() > 0,
            first_role_order_error(values@, 1) == first_role_order_error(values@, index as nat),
        decreases values.len() - index,
    {
        let previous = values[index - 1].canonical_rank();
        let current = values[index].canonical_rank();
        if previous == current {
            return Err(PolicyError::duplicate_canonical_value(CanonicalCollection::Roles));
        }
        if previous > current {
            return Err(PolicyError::non_canonical_order(CanonicalCollection::Roles));
        }
        index += 1;
    }
    Ok(())
}

pub open spec fn validation_result_is_exact(
    result: Result<(), PolicyError>,
    expected_error: Option<crate::PolicyErrorKind>,
    collection: CanonicalCollection,
) -> bool {
    match result {
        Ok(()) => expected_error.is_none(),
        Err(error) => {
            expected_error == Some(error.spec_kind())
                && error.spec_collection() == Some(collection)
                && error.spec_dimension().is_none()
        }
    }
}

} // verus!
