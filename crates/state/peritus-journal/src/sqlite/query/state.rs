//! Current and historical journal-owned state reads.

use rusqlite::{Connection, OptionalExtension, params};

use super::{corrupt, digest_from_blob, positive_u64};
use crate::{
    DurableStateRecord, JournalError, JournalErrorKind, MAX_STATE_BYTES, MAX_STATE_KEY_BYTES,
    SqliteJournal,
};
use peritus_types::Sha256Digest;

type HistoryRow = (Vec<u8>, Vec<u8>, i64, bool);

/// Maximum number of state-record metadata rows read by one physical query window.
pub const MAX_STATE_RECORD_METADATA_PAGE: usize = 4_096;

/// Checked discovery metadata for one current journal-owned state row.
///
/// The value digest has the correct stored representation, but this metadata view does not read
/// the value and therefore is not authority for its bytes. Callers must use
/// [`SqliteJournal::state_record`] before consuming the selected value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateRecordMetadata {
    namespace: u16,
    key: Vec<u8>,
    revision: u64,
    digest: Sha256Digest,
    value_bytes: u64,
    producing_position: u64,
}

impl StateRecordMetadata {
    /// Returns the nonzero state namespace.
    #[must_use]
    pub const fn namespace(&self) -> u16 {
        self.namespace
    }

    /// Borrows the bounded binary key.
    #[must_use]
    pub fn key(&self) -> &[u8] {
        &self.key
    }

    /// Returns the positive current state revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the well-formed stored value digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Returns the stored value length without loading the value.
    #[must_use]
    pub const fn value_bytes(&self) -> u64 {
        self.value_bytes
    }

    /// Returns the event position that installed the current revision.
    #[must_use]
    pub const fn producing_position(&self) -> u64 {
        self.producing_position
    }
}

/// One bounded keyset page of current state-record discovery metadata.
#[derive(Debug, Eq, PartialEq)]
pub struct StateRecordMetadataPage {
    records: Vec<StateRecordMetadata>,
    next_after: Option<Vec<u8>>,
}

impl StateRecordMetadataPage {
    /// Borrows metadata rows in strict binary-key order.
    #[must_use]
    pub fn records(&self) -> &[StateRecordMetadata] {
        &self.records
    }

    /// Borrows the exclusive binary-key cursor for the next physical page.
    #[must_use]
    pub fn next_after(&self) -> Option<&[u8]> {
        self.next_after.as_deref()
    }

    /// Consumes the page into its ordered rows and exact continuation cursor.
    #[must_use]
    pub fn into_parts(self) -> (Vec<StateRecordMetadata>, Option<Vec<u8>>) {
        (self.records, self.next_after)
    }
}

impl SqliteJournal {
    /// Reads one physical keyset page of current state-record metadata in a namespace.
    ///
    /// `max_records` is a logical request bound. Values above
    /// [`MAX_STATE_RECORD_METADATA_PAGE`] are served through successive physical pages using the
    /// returned continuation key; they are not rejected as inadmissible work. This metadata is
    /// suitable for discovery only. Read a selected row through [`Self::state_record`] before
    /// treating its digest or value as authoritative.
    ///
    /// # Errors
    ///
    /// Returns invalid input for namespace zero, an invalid cursor, or an empty page request, and
    /// a storage or integrity error for malformed stored metadata.
    pub fn state_record_metadata_page(
        &self,
        namespace: u16,
        after: Option<&[u8]>,
        max_records: usize,
    ) -> Result<StateRecordMetadataPage, JournalError> {
        if namespace == 0
            || max_records == 0
            || after.is_some_and(|key| key.is_empty() || key.len() > MAX_STATE_KEY_BYTES)
        {
            return Err(invalid_metadata("state metadata page identity or bound is invalid"));
        }
        let page_records = max_records.min(MAX_STATE_RECORD_METADATA_PAGE);
        let query_limit = page_records
            .checked_add(1)
            .ok_or_else(|| invalid_metadata("state metadata physical page overflowed"))?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT record_key, revision, value_digest,
                        COALESCE(value_bytes, length(value)), producing_position,
                        EXISTS(SELECT 1 FROM events
                                WHERE global_position = state_records.producing_position)
                   FROM state_records
                  WHERE namespace = ?1 AND (?2 IS NULL OR record_key > ?2)
                  ORDER BY record_key
                  LIMIT ?3",
            )
            .map_err(|error| JournalError::sqlite("prepare state metadata page", error))?;
        let rows = statement
            .query_map(
                params![
                    i64::from(namespace),
                    after,
                    i64::try_from(query_limit).map_err(|_| {
                        invalid_metadata("state metadata physical page is not representable")
                    })?,
                ],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, bool>(5)?,
                    ))
                },
            )
            .map_err(|error| JournalError::sqlite("query state metadata page", error))?;
        let mut records = rows
            .map(|row| {
                let (key, revision, digest, value_bytes, producing_position, producer_exists) =
                    row.map_err(|error| JournalError::sqlite("read state metadata page", error))?;
                if key.is_empty() || key.len() > MAX_STATE_KEY_BYTES || !producer_exists {
                    return Err(corrupt("state metadata row has an invalid key or producer"));
                }
                let value_bytes = u64::try_from(value_bytes)
                    .map_err(|_| corrupt("state metadata value length is negative"))?;
                if value_bytes > MAX_STATE_BYTES as u64 {
                    return Err(corrupt("state metadata value length exceeds its durable bound"));
                }
                Ok(StateRecordMetadata {
                    namespace,
                    key,
                    revision: positive_u64(revision, "state metadata revision")?,
                    digest: digest_from_blob(&digest, "state metadata value digest")?,
                    value_bytes,
                    producing_position: positive_u64(
                        producing_position,
                        "state metadata producing position",
                    )?,
                })
            })
            .collect::<Result<Vec<_>, JournalError>>()?;
        let has_more = records.len() > page_records;
        if has_more {
            records.truncate(page_records);
        }
        let next_after = if has_more {
            Some(
                records
                    .last()
                    .ok_or_else(|| corrupt("state metadata continuation is missing"))?
                    .key
                    .clone(),
            )
        } else {
            None
        };
        Ok(StateRecordMetadataPage { records, next_after })
    }

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
        load_state_record(&self.connection, namespace, key)
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
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| JournalError::sqlite("begin state history read", error))?;
        let row: Option<HistoryRow> = transaction
            .query_row(
                "SELECT value_digest, root_digest, producing_position,
                        EXISTS(SELECT 1 FROM events
                               WHERE global_position = producing_position)
                   FROM state_record_history
                  WHERE namespace = ?1 AND record_key = ?2 AND revision = ?3",
                params![
                    i64::from(namespace),
                    key,
                    super::super::append::to_i64(revision, "state history revision")?,
                ],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(|error| JournalError::sqlite("read state history", error))?;
        let record = row
            .map(|(stored_digest, root, producing_position, producer_exists)| {
                if !producer_exists {
                    return Err(corrupt("state history has no exact producing event"));
                }
                let bytes = super::super::history::restore(&transaction, &root)?;
                let producing_position =
                    positive_u64(producing_position, "state producing position")?;
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
            .transpose()?;
        transaction
            .commit()
            .map_err(|error| JournalError::sqlite("finish state history read", error))?;
        Ok(record)
    }
}

const fn invalid_metadata(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::InvalidInput, "query state metadata", detail)
}

pub(crate) fn load_state_record(
    connection: &Connection,
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
    let row: Option<(i64, Vec<u8>, Vec<u8>, Option<Vec<u8>>, Option<i64>, i64)> = connection
        .query_row(
            "SELECT revision, value_digest, value, root_digest, value_bytes, producing_position
               FROM state_records WHERE namespace = ?1 AND record_key = ?2",
            params![i64::from(namespace), key],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()
        .map_err(|error| JournalError::sqlite("read state record", error))?;
    row.map(|(revision, stored_digest, inline, root, value_bytes, producing_position)| {
        let revision = positive_u64(revision, "state record revision")?;
        let producing_position = positive_u64(producing_position, "state producing position")?;
        let digest = digest_from_blob(&stored_digest, "state value digest")?;
        let bytes = match (root, value_bytes) {
            (None, None) => inline,
            (Some(root), Some(value_bytes)) => {
                if !inline.is_empty() {
                    return Err(corrupt("paged state record also retains inline bytes"));
                }
                let expected = usize::try_from(value_bytes)
                    .map_err(|_| corrupt("state value length is invalid"))?;
                if expected > MAX_STATE_BYTES {
                    return Err(corrupt("state value length exceeds its durable bound"));
                }
                let bytes = super::super::history::restore(connection, &root)?;
                if bytes.len() != expected {
                    return Err(corrupt("state history root differs from current value length"));
                }
                bytes
            }
            _ => return Err(corrupt("state value paging metadata is partial")),
        };
        if bytes.len() > MAX_STATE_BYTES {
            return Err(corrupt("state value length exceeds its durable bound"));
        }
        if peritus_codec::sha256(&bytes) != digest {
            return Err(corrupt("state value digest does not match exact bytes"));
        }
        let event_exists: i64 = connection
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
