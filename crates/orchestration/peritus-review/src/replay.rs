//! Checked durable replay observations and semantic checkpoint comparison.

use core::fmt;

use peritus_journal::StoreId;

use crate::wire::ReviewStateFrame;
use crate::{ReviewError, ReviewEvent, ReviewRunState};

/// Contiguous canonical D2 events and their atomically installed cache checkpoint.
pub struct ReviewReplay {
    store_id: StoreId,
    events: Vec<ReviewEvent>,
    checkpoint: Option<ReviewStateFrame>,
}

impl ReviewReplay {
    pub(crate) const fn from_parts(
        store_id: StoreId,
        events: Vec<ReviewEvent>,
        checkpoint: Option<ReviewStateFrame>,
    ) -> Self {
        Self { store_id, events, checkpoint }
    }

    /// Returns the durable journal store that produced this observation.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }

    /// Borrows contiguous canonical review events.
    #[must_use]
    pub fn events(&self) -> &[ReviewEvent] {
        &self.events
    }

    /// Rebuilds from genesis and requires every checkpoint field to match semantic replay.
    ///
    /// # Errors
    /// Rejects any illegal event chain or absent, ahead, behind, or different checkpoint.
    pub fn rebuild(&self) -> Result<Option<ReviewRunState>, ReviewError> {
        if self.events.is_empty() {
            return if self.checkpoint.is_none() {
                Ok(None)
            } else {
                Err(crate::durability::inconsistent("review checkpoint exists without events"))
            };
        }
        let state = crate::replay(&self.events)?;
        if !self.checkpoint.as_ref().is_some_and(|checkpoint| checkpoint.matches_state(&state)) {
            return Err(crate::durability::inconsistent(
                "review checkpoint differs from deterministic genesis replay",
            ));
        }
        Ok(Some(state))
    }
}

impl fmt::Debug for ReviewReplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReviewReplay")
            .field("store_id", &self.store_id)
            .field("events", &self.events.len())
            .field("checkpoint_sequence", &self.checkpoint.as_ref().map(ReviewStateFrame::sequence))
            .finish_non_exhaustive()
    }
}
