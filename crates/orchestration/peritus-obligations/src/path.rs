//! Exact path mentions with explicit acceptance roles.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{ObligationError, ObligationErrorKind, PathId};
use vstd::prelude::*;

mod validation;

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
    /// Exactly the two path roles that demand candidate output evidence.
    pub open spec fn spec_requires_candidate_evidence(self) -> bool {
        matches!(self, Self::RequiredOutput | Self::RequiredModification)
    }

    /// Whether candidate evidence must include the path.
    #[must_use]
    pub const fn requires_candidate_evidence(self) -> (required: bool)
        ensures required == self.spec_requires_candidate_evidence(),
    {
        matches!(self, Self::RequiredOutput | Self::RequiredModification)
    }
}

/// Exact public path bytes and their declared role.
#[derive(Debug, Eq, PartialEq)]
pub struct PathMention {
    id: PathId,
    exact: Vec<u8>,
    role: PathRole,
}

impl PathMention {
    /// Exact size-first, then earliest noncanonical adjacent-pair error.
    pub open spec fn spec_validation_error(
        paths: Seq<Self>, maximum: usize, error: ObligationError,
    ) -> bool { validation::validation_error(paths, maximum, error) }

    /// Complete path identity.
    pub closed spec fn spec_id(&self) -> PathId { self.id }
    /// Every exact path spelling byte.
    pub closed spec fn spec_exact(&self) -> Seq<u8> { self.exact@ }
    /// Exact declared path role.
    pub closed spec fn spec_role(&self) -> PathRole { self.role }

    /// Complete semantic equality of path mentions.
    pub open spec fn spec_same_content(&self, other: &Self) -> bool {
        self.spec_id() == other.spec_id() && self.spec_exact() == other.spec_exact()
            && self.spec_role() == other.spec_role()
    }

    /// Complete semantic equality in the same sequence order.
    pub open spec fn sequence_same_content(left: Seq<Self>, right: Seq<Self>) -> bool {
        left.len() == right.len() && forall |index: int| 0 <= index < left.len() ==>
            #[trigger] left[index].spec_same_content(&right[index])
    }

    /// Complete identity bytes in sequence order.
    pub open spec fn keys(paths: Seq<Self>) -> Seq<Seq<u8>> {
        paths.map(|_index: int, path: Self| path.spec_id().spec_digest().spec_bytes()@)
    }

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
    ) -> (result: Result<Self, ObligationError>)
        ensures result.is_ok() == (0 < exact@.len() <= maximum_bytes),
            match result {
                Ok(value) => value.spec_id() == id && value.spec_exact() == exact@
                    && value.spec_role() == role,
                Err(error) => error.spec_numbers(ObligationErrorKind::InvalidText,
                    maximum_bytes as u64, exact@.len() as u64),
            },
    {
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
    pub const fn id(&self) -> (value: PathId)
        ensures value == self.spec_id(),
    { self.id }

    /// Exact bytes copied from the public clause.
    #[must_use]
    pub const fn exact(&self) -> (value: &[u8])
        ensures value@ == self.spec_exact(),
    { self.exact.as_slice() }

    /// Publicly declared path role.
    #[must_use]
    pub const fn role(&self) -> (value: PathRole)
        ensures value == self.spec_role(),
    { self.role }
}

impl Clone for PathMention {
    fn clone(&self) -> (value: Self)
        ensures self.spec_same_content(&value),
    {
        let exact = self.exact.clone();
        proof { assert(exact@ =~= self.exact@); }
        Self { id: self.id, exact, role: self.role }
    }
}

impl PathMention {
    pub(crate) fn clone_sequence(paths: &[Self]) -> (result: Vec<Self>)
        ensures Self::sequence_same_content(paths@, result@),
            Self::keys(paths@) == Self::keys(result@),
    {
        let mut result = Vec::with_capacity(paths.len());
        let mut index = 0;
        while index < paths.len()
            invariant index <= paths.len(), result@.len() == index,
                forall |prior: int| #![auto] 0 <= prior < index ==>
                    paths@[prior].spec_same_content(&result@[prior]),
            decreases paths.len() - index,
        {
            result.push(paths[index].clone());
            index += 1;
        }
        assert(Self::keys(paths@) =~= Self::keys(result@));
        result
    }
}

pub fn validate_paths(paths: &[PathMention], maximum: usize) -> (result: Result<(), ObligationError>)
    ensures result.is_ok() == (paths@.len() <= maximum
        && crate::order::ordered(PathMention::keys(paths@))),
        result.is_ok() ==> crate::order::unique(PathMention::keys(paths@)),
        match result {
            Err(error) => PathMention::spec_validation_error(paths@, maximum, error),
            Ok(()) => true,
        },
{
    validation::validate(paths, maximum)
}

} // verus!
