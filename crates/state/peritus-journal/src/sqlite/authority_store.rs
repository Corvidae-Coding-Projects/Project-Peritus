//! Authority-clock allocation and current registry observations.

use crate::{
    AllocatedAuthorityEpoch, AuthorityEpoch, CurrentAuthorityEpoch, CurrentCredentialRegistry,
    ExactFrame, ExpectedAuthorityEpoch, JournalError, JournalErrorKind,
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::SqliteJournal;

type RegistryRow = (i64, i64, Vec<u8>, Vec<u8>, i64);

impl SqliteJournal {
    /// Atomically compares and allocates the next positive authority epoch.
    ///
    /// # Errors
    ///
    /// Returns a stale-CAS error when the durable clock differs, or terminal overflow when its
    /// representable range is exhausted.
    pub fn allocate_authority_epoch(
        &mut self,
        expected: ExpectedAuthorityEpoch,
    ) -> Result<AllocatedAuthorityEpoch, JournalError> {
        let expected_value = match expected {
            ExpectedAuthorityEpoch::Absent => None,
            ExpectedAuthorityEpoch::Current(epoch) => Some(epoch.get()),
        };
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| JournalError::sqlite("begin authority epoch allocation", error))?;
        let observed: Option<i64> = transaction
            .query_row("SELECT current_epoch FROM authority_clock WHERE singleton = 1", [], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|error| JournalError::sqlite("observe authority epoch", error))?;
        let next = match (expected, observed) {
            (ExpectedAuthorityEpoch::Absent, None) => AuthorityEpoch::new(1)?,
            (ExpectedAuthorityEpoch::Current(expected), Some(value))
                if u64::try_from(value).ok() == Some(expected.get()) =>
            {
                expected.checked_next()?
            }
            _ => {
                return Err(JournalError::new(
                    JournalErrorKind::StaleAuthorityEpoch,
                    "allocate authority epoch",
                    "stored authority epoch differs from CAS expectation",
                ));
            }
        };
        if !crate::verified::authority_epoch_successor(expected_value, next.get()) {
            return Err(JournalError::new(
                JournalErrorKind::CorruptJournal,
                "allocate authority epoch",
                "planned authority epoch is not the exact durable successor",
            ));
        }
        transaction
            .execute(
                "INSERT INTO authority_clock(singleton, current_epoch) VALUES (1, ?1) ON CONFLICT(singleton) DO UPDATE SET current_epoch = excluded.current_epoch",
                params![super::append::to_i64(next.get(), "authority epoch")?],
            )
            .map_err(|error| JournalError::sqlite("persist authority epoch", error))?;
        transaction
            .commit()
            .map_err(|error| JournalError::sqlite("commit authority epoch allocation", error))?;
        Ok(AllocatedAuthorityEpoch { epoch: next })
    }

    /// Observes the exact current durable authority-clock epoch.
    ///
    /// # Errors
    ///
    /// Returns a terminal integrity error for a nonpositive or malformed stored epoch. An
    /// uninitialized clock is represented as `None`.
    pub fn current_authority_epoch(&self) -> Result<Option<CurrentAuthorityEpoch>, JournalError> {
        let value: Option<i64> = self
            .connection
            .query_row("SELECT current_epoch FROM authority_clock WHERE singleton = 1", [], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|error| JournalError::sqlite("observe authority epoch", error))?;
        value
            .map(|value| {
                let value = super::query::positive_u64(value, "authority epoch")?;
                Ok(CurrentAuthorityEpoch { epoch: AuthorityEpoch::new(value)? })
            })
            .transpose()
    }

    /// Observes the exact durable current credential-registry snapshot.
    ///
    /// # Errors
    ///
    /// Returns a typed not-found or terminal integrity failure.
    pub fn current_credential_registry(&self) -> Result<CurrentCredentialRegistry, JournalError> {
        let row: Option<RegistryRow> = self
            .connection
            .query_row(
                "SELECT revision, generation, snapshot_digest, snapshot, producing_position FROM credential_registry WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()
            .map_err(|error| JournalError::sqlite("observe credential registry", error))?;
        let Some((revision, generation, stored_digest, snapshot, producing_position)) = row else {
            return Err(JournalError::new(
                JournalErrorKind::NotFound,
                "observe credential registry",
                "credential registry has not been installed",
            ));
        };
        let revision = super::query::positive_u64(revision, "credential registry revision")?;
        let generation = super::query::positive_u64(generation, "credential generation")?;
        let producing_position =
            super::query::positive_u64(producing_position, "registry producing position")?;
        let snapshot = ExactFrame::new(snapshot)
            .map_err(|_| super::query::corrupt("stored credential snapshot frame is invalid"))?;
        let stored_digest =
            super::query::digest_from_blob(&stored_digest, "credential snapshot digest")?;
        let payload_digest = crate::authority::credential_registry_payload_digest(&snapshot)
            .map_err(|_| {
                super::query::corrupt("stored credential snapshot uses an unsupported schema")
            })?;
        if stored_digest != payload_digest {
            return Err(super::query::corrupt(
                "credential snapshot digest does not match canonical payload bytes",
            ));
        }
        Ok(CurrentCredentialRegistry {
            revision,
            generation,
            digest: stored_digest,
            snapshot,
            producing_position,
        })
    }
}
