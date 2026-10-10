//! Exact canonical validation for selector identity collections.

use crate::{ActorRole, CanonicalCollection, PolicyError, identity::compare_identifier_bytes};
use core::cmp::Ordering;
use peritus_types::{ActorId, EnvironmentId};

pub fn validate_actor_values(values: &[ActorId]) -> Result<(), PolicyError> {
    if values.is_empty() {
        return Err(PolicyError::empty_canonical_collection(CanonicalCollection::Actors));
    }
    let mut index = 1;
    while index < values.len() {
        match compare_identifier_bytes(values[index - 1].as_bytes(), values[index].as_bytes()) {
            Ordering::Less => {}
            Ordering::Equal => {
                return Err(PolicyError::duplicate_canonical_value(CanonicalCollection::Actors));
            }
            Ordering::Greater => {
                return Err(PolicyError::non_canonical_order(CanonicalCollection::Actors));
            }
        }
        index += 1;
    }
    Ok(())
}
pub fn validate_environment_values(values: &[EnvironmentId]) -> Result<(), PolicyError> {
    if values.is_empty() {
        return Err(PolicyError::empty_canonical_collection(CanonicalCollection::Environments));
    }
    let mut index = 1;
    while index < values.len() {
        match compare_identifier_bytes(values[index - 1].as_bytes(), values[index].as_bytes()) {
            Ordering::Less => {}
            Ordering::Equal => {
                return Err(PolicyError::duplicate_canonical_value(
                    CanonicalCollection::Environments,
                ));
            }
            Ordering::Greater => {
                return Err(PolicyError::non_canonical_order(CanonicalCollection::Environments));
            }
        }
        index += 1;
    }
    Ok(())
}
pub fn validate_role_values(values: &[ActorRole]) -> Result<(), PolicyError> {
    if values.is_empty() {
        return Err(PolicyError::empty_canonical_collection(CanonicalCollection::Roles));
    }
    let mut index = 1;
    while index < values.len() {
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
