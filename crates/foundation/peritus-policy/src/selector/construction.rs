//! Exact checked construction for finite selectors.

use super::{ActorSelector, EnvironmentSelector, RoleSelector, SelectorValues};
use crate::{
    ActorRole, PolicyError,
    scope_validation::{validate_actor_values, validate_environment_values, validate_role_values},
};

use peritus_types::{ActorId, EnvironmentId};

impl ActorSelector {
    #[doc = " Creates a canonical nonempty exact actor selector."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the exact first canonical-collection failure."]
    pub fn exact(values: Vec<ActorId>) -> Result<Self, PolicyError> {
        validate_actor_values(values.as_slice())?;
        let selector = Self(SelectorValues::Exact(values));
        Ok(selector)
    }
}
impl RoleSelector {
    #[doc = " Creates a canonical nonempty exact role selector."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the exact first canonical-collection failure."]
    pub fn exact(values: Vec<ActorRole>) -> Result<Self, PolicyError> {
        validate_role_values(values.as_slice())?;
        let selector = Self(SelectorValues::Exact(values));
        Ok(selector)
    }
}
impl EnvironmentSelector {
    #[doc = " Creates a canonical nonempty exact environment selector."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the exact first canonical-collection failure."]
    pub fn exact(values: Vec<EnvironmentId>) -> Result<Self, PolicyError> {
        validate_environment_values(values.as_slice())?;
        let selector = Self(SelectorValues::Exact(values));
        Ok(selector)
    }
}
