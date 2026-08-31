//! Narrow `SQLite` durability adapter.

mod append;
mod apply_rows;
mod authority_store;
mod connection;
mod outbox_store;
mod preconditions;
pub mod query;
mod schema;

#[cfg(test)]
mod tests;

use crate::CommittedBatch;
use peritus_types::{CommandId, Sha256Digest};

pub use connection::{SqliteJournal, SqliteJournalOptions, SqliteSettings, SqliteStoragePages};

/// Durable resolution of one exact command identity.
#[derive(Debug, Eq, PartialEq)]
pub enum CommandResolution {
    /// No command row exists at the completed read transaction.
    DefinitelyAbsent,
    /// The command and request digest identify an exact committed result.
    Committed(CommittedBatch),
    /// The command identity exists but is bound to a different request digest.
    Conflict {
        /// Requested command identity.
        command_id: CommandId,
        /// Digest already bound to the command.
        stored_digest: Sha256Digest,
    },
}
