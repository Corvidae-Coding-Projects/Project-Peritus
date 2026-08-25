//! Checked durable replay observations and exact checkpoint comparison.

use core::fmt;

use peritus_journal::StoreId;

use crate::wire::CollaborationStateFrame;
use crate::{CollaborationError, CollaborationEvent, CollaborationState};

/// Contiguous canonical collaboration events and atomically installed checkpoint.
pub struct CollaborationReplay {
    store_id: StoreId,
    events: Vec<CollaborationEvent>,
    checkpoint: Option<CollaborationStateFrame>,
}

impl CollaborationReplay {
    pub(super) const fn from_parts(
        store_id: StoreId,
        events: Vec<CollaborationEvent>,
        checkpoint: Option<CollaborationStateFrame>,
    ) -> Self {
        Self { store_id, events, checkpoint }
    }
    /// Returns the durable source store.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }
    /// Borrows contiguous canonical events.
    #[must_use]
    pub fn events(&self) -> &[CollaborationEvent] {
        &self.events
    }
    /// Rebuilds from genesis and requires exact checkpoint equality.
    ///
    /// # Errors
    /// Rejects illegal event chains or absent/ahead/behind/different checkpoints.
    pub fn rebuild(&self) -> Result<Option<CollaborationState>, CollaborationError> {
        if self.events.is_empty() {
            return if self.checkpoint.is_none() {
                Ok(None)
            } else {
                Err(crate::durability::inconsistent(
                    "collaboration checkpoint exists without events",
                ))
            };
        }
        let state = crate::replay(&self.events)?;
        if !self.checkpoint.as_ref().is_some_and(|checkpoint| checkpoint.matches_state(&state)) {
            return Err(crate::durability::inconsistent(
                "collaboration checkpoint differs from deterministic replay",
            ));
        }
        Ok(Some(state))
    }
}

impl fmt::Debug for CollaborationReplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CollaborationReplay")
            .field("store_id", &self.store_id)
            .field("events", &self.events.len())
            .field(
                "checkpoint_sequence",
                &self.checkpoint.as_ref().map(CollaborationStateFrame::sequence),
            )
            .finish_non_exhaustive()
    }
}
