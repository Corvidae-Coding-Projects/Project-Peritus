//! Explicit allocation bounds for run knowledge.

use crate::{KnowledgeError, KnowledgeErrorKind};
use vstd::prelude::*;

verus! {

/// Maximum retained sections, source bindings, and dependencies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KnowledgeLimits {
    sections: usize,
    catalog_sources: usize,
    sources_per_section: usize,
    dependencies_per_section: usize,
}

impl KnowledgeLimits {
    /// Creates nonzero knowledge bounds.
    ///
    /// # Errors
    ///
    /// Returns [`KnowledgeErrorKind::InvalidLimit`] when any bound is zero.
    pub const fn new(
        max_sections: usize,
        max_catalog_sources: usize,
        max_sources_per_section: usize,
        max_dependencies_per_section: usize,
    ) -> Result<Self, KnowledgeError> {
        if max_sections == 0
            || max_catalog_sources == 0
            || max_sources_per_section == 0
            || max_dependencies_per_section == 0
        {
            Err(KnowledgeError::plain(KnowledgeErrorKind::InvalidLimit))
        } else {
            Ok(Self {
                sections: max_sections,
                catalog_sources: max_catalog_sources,
                sources_per_section: max_sources_per_section,
                dependencies_per_section: max_dependencies_per_section,
            })
        }
    }

    /// Maximum sections in one role snapshot.
    #[must_use]
    pub const fn max_sections(self) -> usize { self.sections }

    /// Maximum distinct sources in the complete current-state catalog.
    #[must_use]
    pub const fn max_catalog_sources(self) -> usize { self.catalog_sources }

    /// Maximum exact source digests bound to one section.
    #[must_use]
    pub const fn max_sources_per_section(self) -> usize { self.sources_per_section }

    /// Maximum direct section dependencies.
    #[must_use]
    pub const fn max_dependencies_per_section(self) -> usize {
        self.dependencies_per_section
    }
}

} // verus!
