//! Exact path mentions with explicit acceptance roles.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{ObligationError, ObligationErrorKind, PathId};

mod validation;

#[doc = " Meaning of one path mentioned by the public task."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PathRole {
    #[doc = " The candidate must create this exact output."]
    RequiredOutput,
    #[doc = " The candidate must modify this existing path."]
    RequiredModification,
    #[doc = " The path is an input that must be read, not emitted."]
    RequiredInput,
    #[doc = " The path identifies context but is not an output requirement."]
    Reference,
    #[doc = " The path is illustrative and never mandatory."]
    Example,
}
impl PathRole {
    #[doc = " Whether candidate evidence must include the path."]
    #[must_use]
    pub const fn requires_candidate_evidence(self) -> bool {
        matches!(self, Self::RequiredOutput | Self::RequiredModification)
    }
}
#[doc = " Exact public path bytes and their declared role."]
#[derive(Debug, Eq, PartialEq)]
pub struct PathMention {
    id: PathId,
    exact: Vec<u8>,
    role: PathRole,
}
impl PathMention {
    #[doc = " Creates a nonempty bounded exact path mention."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects an empty path or one exceeding `maximum_bytes`."]
    pub fn new(
        id: PathId,
        exact: Vec<u8>,
        role: PathRole,
        maximum_bytes: usize,
    ) -> Result<Self, ObligationError> {
        if exact.is_empty() || exact.len() > maximum_bytes {
            Err(ObligationError::numbers(
                ObligationErrorKind::InvalidText,
                maximum_bytes as u64,
                exact.len() as u64,
            ))
        } else {
            Ok(Self { id, exact, role })
        }
    }
    #[doc = " Stable path identity."]
    #[must_use]
    pub const fn id(&self) -> PathId {
        self.id
    }
    #[doc = " Exact bytes copied from the public clause."]
    #[must_use]
    pub const fn exact(&self) -> &[u8] {
        self.exact.as_slice()
    }
    #[doc = " Publicly declared path role."]
    #[must_use]
    pub const fn role(&self) -> PathRole {
        self.role
    }
}
impl Clone for PathMention {
    fn clone(&self) -> Self {
        let exact = self.exact.clone();
        Self { id: self.id, exact, role: self.role }
    }
}
impl PathMention {
    pub(crate) fn clone_sequence(paths: &[Self]) -> Vec<Self> {
        let mut result = Vec::with_capacity(paths.len());
        let mut index = 0;
        while index < paths.len() {
            result.push(paths[index].clone());
            index += 1;
        }
        result
    }
}
pub fn validate_paths(paths: &[PathMention], maximum: usize) -> Result<(), ObligationError> {
    validation::validate(paths, maximum)
}
