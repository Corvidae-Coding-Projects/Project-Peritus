//! Exact interaction lookup for a durable product run.

use peritus_types::RunId;

/// Query for one exact governed product-run interaction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProductInteractionQuery {
    run_id: RunId,
}

impl ProductInteractionQuery {
    /// Creates an exact interaction query.
    #[must_use]
    pub const fn new(run_id: RunId) -> Self {
        Self { run_id }
    }

    /// Target run.
    #[must_use]
    pub const fn run_id(self) -> RunId {
        self.run_id
    }
}
