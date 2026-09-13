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
    /// Exact maximum number of snapshot sections.
    pub closed spec fn spec_max_sections(&self) -> usize { self.sections }
    /// Exact maximum number of catalog sources.
    pub closed spec fn spec_max_catalog_sources(&self) -> usize { self.catalog_sources }
    /// Exact maximum number of sources on one binding.
    pub closed spec fn spec_max_sources_per_section(&self) -> usize { self.sources_per_section }
    /// Exact maximum number of direct section dependencies.
    pub closed spec fn spec_max_dependencies_per_section(&self) -> usize {
        self.dependencies_per_section
    }

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
    ) -> (result: Result<Self, KnowledgeError>)
        ensures result.is_ok() == (max_sections > 0 && max_catalog_sources > 0
            && max_sources_per_section > 0 && max_dependencies_per_section > 0),
            match result {
                Ok(value) => value.spec_max_sections() == max_sections
                    && value.spec_max_catalog_sources() == max_catalog_sources
                    && value.spec_max_sources_per_section() == max_sources_per_section
                    && value.spec_max_dependencies_per_section() == max_dependencies_per_section,
                Err(error) => error.spec_plain(KnowledgeErrorKind::InvalidLimit),
            },
    {
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
    pub const fn max_sections(self) -> (value: usize)
        ensures value == self.spec_max_sections(),
    { self.sections }

    /// Maximum distinct sources in the complete current-state catalog.
    #[must_use]
    pub const fn max_catalog_sources(self) -> (value: usize)
        ensures value == self.spec_max_catalog_sources(),
    { self.catalog_sources }

    /// Maximum exact source digests bound to one section.
    #[must_use]
    pub const fn max_sources_per_section(self) -> (value: usize)
        ensures value == self.spec_max_sources_per_section(),
    { self.sources_per_section }

    /// Maximum direct section dependencies.
    #[must_use]
    pub const fn max_dependencies_per_section(self) -> (value: usize)
        ensures value == self.spec_max_dependencies_per_section(),
    {
        self.dependencies_per_section
    }
}

} // verus!
