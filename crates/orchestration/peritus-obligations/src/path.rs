//! Exact path mentions with explicit acceptance roles.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{ObligationError, ObligationErrorKind, PathId};
use vstd::prelude::*;

verus! {

/// Meaning of one path mentioned by the public task.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PathRole {
    /// The candidate must create this exact output.
    RequiredOutput,
    /// The candidate must modify this existing path.
    RequiredModification,
    /// The path is an input that must be read, not emitted.
    RequiredInput,
    /// The path identifies context but is not an output requirement.
    Reference,
    /// The path is illustrative and never mandatory.
    Example,
}

impl PathRole {
    /// Whether candidate evidence must include the path.
    #[must_use]
    pub const fn requires_candidate_evidence(self) -> bool {
        matches!(self, Self::RequiredOutput | Self::RequiredModification)
    }
}

/// Exact public path bytes and their declared role.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathMention {
    id: PathId,
    exact: Vec<u8>,
    role: PathRole,
}

impl PathMention {
    /// Creates a nonempty bounded exact path mention.
    ///
    /// # Errors
    ///
    /// Rejects an empty path or one exceeding `maximum_bytes`.
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

    /// Stable path identity.
    #[must_use]
    pub const fn id(&self) -> PathId { self.id }

    /// Exact bytes copied from the public clause.
    #[must_use]
    pub const fn exact(&self) -> &[u8] { self.exact.as_slice() }

    /// Publicly declared path role.
    #[must_use]
    pub const fn role(&self) -> PathRole { self.role }
}

pub fn validate_paths(
    paths: &[PathMention],
    maximum: usize,
) -> Result<(), ObligationError> {
    if paths.len() > maximum {
        return Err(ObligationError::numbers(
            ObligationErrorKind::LimitExceeded,
            maximum as u64,
            paths.len() as u64,
        ));
    }
    let mut index = 0;
    while index < paths.len()
        invariant index <= paths.len(),
        decreases paths.len() - index,
    {
        if index > 0 {
            if paths[index - 1].id() == paths[index].id() {
                return Err(ObligationError::plain(ObligationErrorKind::DuplicateValue));
            }
            if paths[index - 1].id() > paths[index].id() {
                return Err(ObligationError::plain(ObligationErrorKind::NonCanonicalOrder));
            }
        }
        index += 1;
    }
    Ok(())
}

} // verus!
