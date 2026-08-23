//! Immutable requirement, exclusion, and assumption declarations.

use crate::{ContentReference, RequirementId};
use vstd::prelude::*;

verus! {

/// One stable requirement and its immutable content.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Requirement {
    id: RequirementId,
    content: ContentReference,
}

impl Requirement {
    /// Creates an immutable requirement declaration.
    #[must_use]
    pub const fn new(id: RequirementId, content: ContentReference) -> Self { Self { id, content } }

    /// Returns the stable requirement identifier.
    #[must_use]
    pub const fn id(&self) -> RequirementId { self.id }

    /// Returns the immutable requirement content reference.
    #[must_use]
    pub const fn content(&self) -> ContentReference { self.content }
}

/// An explicit behavior or surface excluded from the contract.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Exclusion {
    content: ContentReference,
}

impl Exclusion {
    /// Creates an exclusion from immutable content.
    #[must_use]
    pub const fn new(content: ContentReference) -> Self { Self { content } }

    /// Returns the immutable exclusion content reference.
    #[must_use]
    pub const fn content(&self) -> ContentReference { self.content }
}

/// An explicit premise under which the contract is evaluated.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Assumption {
    content: ContentReference,
}

impl Assumption {
    /// Creates an assumption from immutable content.
    #[must_use]
    pub const fn new(content: ContentReference) -> Self { Self { content } }

    /// Returns the immutable assumption content reference.
    #[must_use]
    pub const fn content(&self) -> ContentReference { self.content }
}

} // verus!
