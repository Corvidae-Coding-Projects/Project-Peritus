//! Checked requests and provider-role selection for product runs.

use peritus_types::ProviderProfileId;

/// Checked provider roles selected for one writer-reviewer-fixer loop.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProductProviderSelection {
    writer: ProviderProfileId,
    reviewer: ProviderProfileId,
    fixer: ProviderProfileId,
}

impl ProductProviderSelection {
    /// Creates an explicit role selection. Reusing a provider is permitted.
    #[must_use]
    pub const fn new(
        writer: ProviderProfileId,
        reviewer: ProviderProfileId,
        fixer: ProviderProfileId,
    ) -> Self {
        Self { writer, reviewer, fixer }
    }

    /// Writer profile identity.
    #[must_use]
    pub const fn writer(self) -> ProviderProfileId {
        self.writer
    }
    /// Reviewer profile identity.
    #[must_use]
    pub const fn reviewer(self) -> ProviderProfileId {
        self.reviewer
    }
    /// Fixer profile identity.
    #[must_use]
    pub const fn fixer(self) -> ProviderProfileId {
        self.fixer
    }
}
