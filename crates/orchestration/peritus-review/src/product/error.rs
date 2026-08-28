//! Product review validation failures.

use core::fmt;

/// Invalid typed reviewer observation or finding-ledger transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductReviewError {
    detail: &'static str,
}

impl ProductReviewError {
    pub(super) const fn new(detail: &'static str) -> Self {
        Self { detail }
    }

    /// Stable safe diagnostic.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl fmt::Display for ProductReviewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.detail)
    }
}

impl std::error::Error for ProductReviewError {}
