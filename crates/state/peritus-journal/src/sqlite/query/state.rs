//! Current and historical journal-owned state reads.

use rusqlite::{OptionalExtension, params};

use super::{corrupt, digest_from_blob, positive_u64};
use crate::{DurableStateRecord, JournalError, JournalErrorKind, SqliteJournal};

impl SqliteJournal {
    /// Reads and digest-checks the current revision of one durable state row.
    ///
    /// # Errors
    ///
    /// Returns a storage or terminal integrity error for malformed state. Absence is represented
    /// as `None` so callers can perform an explicit compare-and-install decision.
    pub fn state_record(
        &self,
        namespace: u16,
        key: &[u8],
    ) -> Result<Option<DurableStateRecord>, JournalError> {
        if namespace == 0 || key.is_empty() || key.len() > crate::record::MAX_STATE_KEY_BYTES {
            return Err(JournalError::new(
                JournalErrorKind::InvalidInput,
                "read state record",
                "state namespace or key is outside its canonical bounds",
            ));
        }
        let row: Option<(i64, Vec<u8>, Vec<u8>, i64)> = self
            .connection
            .query_row(
                "SELECT revision, value_digest, value, producing_position
                   FROM state_records WHERE namespace = ?1 AND record_key = ?2",
                params![i64::from(namespace), key],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(|error| JournalError::sqlite("read state record", error))?;
        row.map(|(revision, stored_digest, bytes, producing_position)| {
            let revision = positive_u64(revision, "state record revision")?;
            let producing_position = positive_u64(producing_position, "state producing position")?;
            let digest = digest_from_blob(&stored_digest, "state value digest")?;
            if peritus_codec::sha256(&bytes) != digest {
                return Err(corrupt("state value digest does not match exact bytes"));
            }
            let event_exists: i64 = self
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM events WHERE global_position = ?1",
                    [super::super::append::to_i64(producing_position, "state producing position")?],
                    |row| row.get(0),
                )
                .map_err(|error| JournalError::sqlite("validate state producer", error))?;
            if event_exists != 1 {
                return Err(corrupt("state record has no exact producing event"));
            }
            Ok(DurableStateRecord {
                namespace,
                key: key.to_vec(),
                revision,
                bytes,
                digest,
                producing_position,
            })
        })
        .transpose()
    }

    /// Reads one immutable historical revision of a journal-owned state row.
    ///
    /// # Errors
    ///
    /// Returns a storage or terminal integrity error for malformed history. Absence is `None`.
    pub fn state_record_revision(
        &self,
        namespace: u16,
        key: &[u8],
        revision: u64,
    ) -> Result<Option<DurableStateRecord>, JournalError> {
        if namespace == 0
            || key.is_empty()
            || key.len() > crate::record::MAX_STATE_KEY_BYTES
            || revision == 0
        {
            return Err(JournalError::new(
                JournalErrorKind::InvalidInput,
                "read state history",
                "state namespace, key, or revision is outside its canonical bounds",
            ));
        }
        let row: Option<(Vec<u8>, Vec<u8>, i64)> = self
            .connection
            .query_row(
                "SELECT value_digest, value, producing_position
                   FROM state_record_history
                  WHERE namespace = ?1 AND record_key = ?2 AND revision = ?3",
                params![
                    i64::from(namespace),
                    key,
                    super::super::append::to_i64(revision, "state history revision")?,
                ],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|error| JournalError::sqlite("read state history", error))?;
        row.map(|(stored_digest, bytes, producing_position)| {
            let producing_position = positive_u64(producing_position, "state producing position")?;
            let digest = digest_from_blob(&stored_digest, "state history value digest")?;
            if peritus_codec::sha256(&bytes) != digest {
                return Err(corrupt("state history digest does not match exact bytes"));
            }
            Ok(DurableStateRecord {
                namespace,
                key: key.to_vec(),
                revision,
                bytes,
                digest,
                producing_position,
            })
        })
        .transpose()
    }
}
