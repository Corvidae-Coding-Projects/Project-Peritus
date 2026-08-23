//! Exact checked construction for finite selectors.

use super::{ActorSelector, EnvironmentSelector, RoleSelector, SelectorValues};
use crate::{
    scope_validation::{validate_actor_values, validate_environment_values, validate_role_values},
    ActorRole, PolicyError,
};
#[cfg(verus_only)]
use crate::{
    scope_validation::{
        actor_validation_error, environment_validation_error, role_validation_error,
    },
    CanonicalCollection,
};
use peritus_types::{ActorId, EnvironmentId};
use vstd::prelude::*;

verus! {

impl ActorSelector {
    /// Creates a canonical nonempty exact actor selector.
    ///
    /// # Errors
    ///
    /// Returns the exact first canonical-collection failure.
    pub fn exact(values: Vec<ActorId>) -> (result: Result<Self, PolicyError>)
        ensures
            match result {
                Ok(selector) => {
                    actor_validation_error(values@).is_none()
                        && selector.spec_exact_values() == Some(values@)
                }
                Err(error) => {
                    actor_validation_error(values@) == Some(error.spec_kind())
                        && error.spec_collection() == Some(CanonicalCollection::Actors)
                        && error.spec_dimension().is_none()
                }
            },
    {
        validate_actor_values(values.as_slice())?;
        let selector = Self(SelectorValues::Exact(values));
        reveal(ActorSelector::spec_exact_values);
        Ok(selector)
    }
}

impl RoleSelector {
    /// Creates a canonical nonempty exact role selector.
    ///
    /// # Errors
    ///
    /// Returns the exact first canonical-collection failure.
    pub fn exact(values: Vec<ActorRole>) -> (result: Result<Self, PolicyError>)
        ensures
            match result {
                Ok(selector) => {
                    role_validation_error(values@).is_none()
                        && selector.spec_exact_values() == Some(values@)
                }
                Err(error) => {
                    role_validation_error(values@) == Some(error.spec_kind())
                        && error.spec_collection() == Some(CanonicalCollection::Roles)
                        && error.spec_dimension().is_none()
                }
            },
    {
        validate_role_values(values.as_slice())?;
        let selector = Self(SelectorValues::Exact(values));
        reveal(RoleSelector::spec_exact_values);
        Ok(selector)
    }
}

impl EnvironmentSelector {
    /// Creates a canonical nonempty exact environment selector.
    ///
    /// # Errors
    ///
    /// Returns the exact first canonical-collection failure.
    pub fn exact(values: Vec<EnvironmentId>) -> (result: Result<Self, PolicyError>)
        ensures
            match result {
                Ok(selector) => {
                    environment_validation_error(values@).is_none()
                        && selector.spec_exact_values() == Some(values@)
                }
                Err(error) => {
                    environment_validation_error(values@) == Some(error.spec_kind())
                        && error.spec_collection() == Some(CanonicalCollection::Environments)
                        && error.spec_dimension().is_none()
                }
            },
    {
        validate_environment_values(values.as_slice())?;
        let selector = Self(SelectorValues::Exact(values));
        reveal(EnvironmentSelector::spec_exact_values);
        Ok(selector)
    }
}

} // verus!
