//! One bounded page of an exact rewind preview.

use super::super::{WorkbenchRewindConfirmation, WorkbenchRewindPath, invalid};
use super::{
    WorkbenchCoverageCursor, WorkbenchCoverageSection, section_total, valid_next, valid_page_text,
};
use crate::AppProtocolError;

/// One transfer page of an exact rewind preview.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchRewindCoveragePage {
    confirmation: WorkbenchRewindConfirmation,
    total_paths: u64,
    total_exclusions: u64,
    total_external_effects: u64,
    section: WorkbenchCoverageSection,
    offset: u64,
    paths: Vec<WorkbenchRewindPath>,
    exclusions: Vec<String>,
    external_effects: Vec<String>,
    next: Option<WorkbenchCoverageCursor>,
}
impl WorkbenchRewindCoveragePage {
    /// Constructs one bounded page with a full-manifest confirmation binding.
    ///
    /// # Errors
    /// Rejects mixed sections, invalid ranges, or a continuation for another snapshot.
    #[allow(
        clippy::too_many_arguments,
        reason = "page identity and exact continuation remain explicit"
    )]
    pub fn new(
        confirmation: WorkbenchRewindConfirmation,
        total_paths: u64,
        total_exclusions: u64,
        total_external_effects: u64,
        section: WorkbenchCoverageSection,
        offset: u64,
        paths: Vec<WorkbenchRewindPath>,
        exclusions: Vec<String>,
        external_effects: Vec<String>,
        next: Option<WorkbenchCoverageCursor>,
    ) -> Result<Self, AppProtocolError> {
        let totals = [total_paths, total_exclusions, total_external_effects];
        let empty_terminal = totals == [0; 3]
            && section == WorkbenchCoverageSection::Paths
            && offset == 0
            && paths.is_empty()
            && exclusions.is_empty()
            && external_effects.is_empty()
            && next.is_none();
        let page_len = match section {
            WorkbenchCoverageSection::Paths => {
                if (!empty_terminal && paths.is_empty())
                    || !exclusions.is_empty()
                    || !external_effects.is_empty()
                {
                    return Err(invalid());
                }
                paths.len()
            }
            WorkbenchCoverageSection::Exclusions => {
                if !paths.is_empty() || exclusions.is_empty() || !external_effects.is_empty() {
                    return Err(invalid());
                }
                exclusions.len()
            }
            WorkbenchCoverageSection::ExternalEffects => {
                if !paths.is_empty() || !exclusions.is_empty() || external_effects.is_empty() {
                    return Err(invalid());
                }
                external_effects.len()
            }
        };
        let end = offset
            .checked_add(u64::try_from(page_len).map_err(|_| invalid())?)
            .ok_or_else(invalid)?;
        if end > section_total(section, total_paths, total_exclusions, total_external_effects)
            || paths.windows(2).any(|pair| pair[0].path() >= pair[1].path())
            || exclusions.iter().any(|value| !valid_page_text(value, 4_610))
            || external_effects.iter().any(|value| !valid_page_text(value, 512))
            || !valid_next(section, end, totals, confirmation.preview_digest(), next)
        {
            return Err(invalid());
        }
        Ok(Self {
            confirmation,
            total_paths,
            total_exclusions,
            total_external_effects,
            section,
            offset,
            paths,
            exclusions,
            external_effects,
            next,
        })
    }
    /// Returns the compact confirmation for the complete checkpoint and request.
    #[must_use]
    pub const fn confirmation(&self) -> WorkbenchRewindConfirmation {
        self.confirmation
    }
    /// Returns the complete covered-path count.
    #[must_use]
    pub const fn total_paths(&self) -> u64 {
        self.total_paths
    }
    /// Returns the complete exclusion count.
    #[must_use]
    pub const fn total_exclusions(&self) -> u64 {
        self.total_exclusions
    }
    /// Returns the complete external-effect count.
    #[must_use]
    pub const fn total_external_effects(&self) -> u64 {
        self.total_external_effects
    }
    /// Returns the section carried by this page.
    #[must_use]
    pub const fn section(&self) -> WorkbenchCoverageSection {
        self.section
    }
    /// Returns the zero-based section offset.
    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }
    /// Borrows exact checkpoint and owned-current preimages on this page.
    #[must_use]
    pub fn paths(&self) -> &[WorkbenchRewindPath] {
        &self.paths
    }
    /// Borrows exclusions on this page.
    #[must_use]
    pub fn exclusions(&self) -> &[String] {
        &self.exclusions
    }
    /// Borrows external effects on this page.
    #[must_use]
    pub fn external_effects(&self) -> &[String] {
        &self.external_effects
    }
    /// Returns the exact continuation, if coverage remains.
    #[must_use]
    pub const fn next(&self) -> Option<WorkbenchCoverageCursor> {
        self.next
    }
}
