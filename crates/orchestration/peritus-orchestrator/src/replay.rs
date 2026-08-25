//! Checked E0 replay bundle reconstructed from C0 records.

use core::fmt;

use peritus_journal::StoreId;

use crate::wire::OrchestratorStateFrame;
use crate::{OrchestratorEvent, OrchestratorState};

/// Canonical E0 event chain and its atomically installed checkpoint.
pub struct OrchestratorReplay {
    store_id: StoreId,
    events: Vec<OrchestratorEvent>,
    checkpoint: Option<OrchestratorStateFrame>,
}

impl OrchestratorReplay {
    pub(crate) const fn from_parts(
        store_id: StoreId,
        events: Vec<OrchestratorEvent>,
        checkpoint: Option<OrchestratorStateFrame>,
    ) -> Self {
        Self { store_id, events, checkpoint }
    }

    #[must_use]
    /// Returns the C0 store that supplied the replay bundle.
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }

    #[must_use]
    /// Returns the canonical ordered E0 event chain.
    pub fn events(&self) -> &[OrchestratorEvent] {
        &self.events
    }

    /// Replays all events and requires exact equality with the installed checkpoint.
    ///
    /// # Errors
    ///
    /// Returns an error when event replay fails or differs from the installed checkpoint.
    pub fn rebuild(&self) -> Result<Option<OrchestratorState>, crate::OrchestratorError> {
        if self.events.is_empty() {
            return if self.checkpoint.is_none() {
                Ok(None)
            } else {
                Err(integrity("E0 checkpoint exists without events"))
            };
        }
        let state = crate::reducer::replay(&self.events)?;
        if !self.checkpoint.as_ref().is_some_and(|frame| frame.matches_state(&state)) {
            return Err(integrity("E0 checkpoint differs from deterministic replay"));
        }
        Ok(Some(state))
    }
}

impl fmt::Debug for OrchestratorReplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrchestratorReplay")
            .field("store_id", &self.store_id)
            .field("event_count", &self.events.len())
            .field(
                "checkpoint_sequence",
                &self.checkpoint.as_ref().map(OrchestratorStateFrame::sequence),
            )
            .finish_non_exhaustive()
    }
}

const fn integrity(detail: &'static str) -> crate::OrchestratorError {
    crate::OrchestratorError::new(
        crate::OrchestratorErrorKind::Integrity,
        crate::OrchestratorRecoveryAction::Quarantine,
        detail,
    )
}
