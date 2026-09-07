//! Connection-bound invalidation of reusable aggregate replay.

use std::sync::Arc;

use rusqlite::Connection;

use crate::{JournalError, JournalErrorKind, SqliteJournal};

/// A process-local observation of append-owned events and checkpoints.
///
/// The marker is valid only on its original journal instance, until another append attempt or
/// a commit by another `SQLite` connection. It does not certify domain replay correctness and
/// does not cover same-connection changes to mutable application ledgers or outbox leases.
/// Observe before reading replay, and use [`SqliteJournal::append_observed`] to guard its use.
#[derive(Clone, Debug)]
pub struct ReplayObservation {
    generation: Arc<()>,
    data_version: i64,
}

impl ReplayObservation {
    pub(super) fn same_generation(&self, generation: &Arc<()>) -> bool {
        Arc::ptr_eq(&self.generation, generation)
    }

    pub(super) fn verify_external_version(
        &self,
        connection: &Connection,
    ) -> Result<(), JournalError> {
        if self.data_version == data_version(connection)? { Ok(()) } else { Err(stale()) }
    }

    pub(super) fn after_append(&self, generation: &Arc<()>) -> Self {
        // Retain the version checked inside the write transaction, never a newer post-commit
        // observation which could accidentally bless an intervening external change.
        Self { generation: Arc::clone(generation), data_version: self.data_version }
    }
}

impl SqliteJournal {
    /// Observes the current connection/append generation and external-commit version.
    ///
    /// Take this observation before cold replay. A write between this read and that replay
    /// conservatively makes the marker stale at the eventual guarded append.
    ///
    /// # Errors
    /// Returns a storage error if `SQLite` cannot report its data version.
    pub fn observe_replay(&self) -> Result<ReplayObservation, JournalError> {
        Ok(ReplayObservation {
            generation: Arc::clone(&self.replay_generation),
            data_version: data_version(&self.connection)?,
        })
    }

    /// Checks whether a previously verified replay may still be reused for planning.
    ///
    /// This read is not a write fence: [`Self::append_observed`] checks again while holding
    /// `SQLite`'s write transaction. False requires discarding the cached replay.
    ///
    /// # Errors
    /// Returns a storage error if `SQLite` cannot report its data version.
    pub fn replay_observation_is_current(
        &self,
        observation: &ReplayObservation,
    ) -> Result<bool, JournalError> {
        Ok(observation.same_generation(&self.replay_generation)
            && observation.data_version == data_version(&self.connection)?)
    }
}

fn data_version(connection: &Connection) -> Result<i64, JournalError> {
    connection
        .pragma_query_value(None, "data_version", |row| row.get(0))
        .map_err(|error| JournalError::sqlite("observe external journal commits", error))
}

pub(super) const fn stale() -> JournalError {
    JournalError::new(
        JournalErrorKind::StaleHead,
        "compare replay observation",
        "journal instance, append generation, or external commit version changed; replay required",
    )
}
