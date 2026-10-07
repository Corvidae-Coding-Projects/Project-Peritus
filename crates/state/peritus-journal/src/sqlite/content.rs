//! Content-addressed bounded physical rows for exact journal payloads.

use peritus_types::Sha256Digest;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::{JournalError, JournalErrorKind};

pub(super) const CHUNK_BYTES: usize = 64 * 1024;
pub(super) const PAGED_INLINE_VALUE: &[u8] = &[0];
const MAX_CONTENT_BYTES: usize = 1_073_741_823;

/// Installs exact bytes as bounded content-addressed rows in the caller's transaction.
pub(super) fn install(
    transaction: &Transaction<'_>,
    bytes: &[u8],
    digest: Sha256Digest,
    maximum_bytes: usize,
) -> Result<i64, JournalError> {
    validate_length(bytes.len(), maximum_bytes)?;
    if peritus_codec::sha256(bytes) != digest {
        return Err(corrupt("journal content digest differs from its exact bytes"));
    }
    let byte_length = to_i64(bytes.len(), "journal content length is not representable")?;
    let chunk_count = chunk_count(bytes.len());
    let chunk_count_i64 = to_i64(chunk_count, "journal content chunk count is not representable")?;
    let existing: Option<(i64, i64)> = transaction
        .query_row(
            "SELECT byte_length, chunk_count FROM journal_content WHERE content_digest = ?1",
            [digest.as_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| JournalError::sqlite("read journal content header", error))?;
    let content_exists = match existing {
        Some(header) if header != (byte_length, chunk_count_i64) => {
            return Err(corrupt("journal content identity is bound to different metadata"));
        }
        Some(_) => true,
        None => {
            transaction
                .execute(
                    "INSERT INTO journal_content(content_digest, byte_length, chunk_count)
                     VALUES (?1, ?2, ?3)",
                    params![digest.as_bytes().as_slice(), byte_length, chunk_count_i64],
                )
                .map_err(|error| JournalError::sqlite("insert journal content header", error))?;
            false
        }
    };
    for (index, chunk) in bytes.chunks(CHUNK_BYTES).enumerate() {
        let index = to_i64(index, "journal content chunk index is not representable")?;
        if !content_exists {
            transaction
                .execute(
                    "INSERT INTO journal_content_chunks(content_digest, chunk_index, payload)
                     VALUES (?1, ?2, ?3)",
                    params![digest.as_bytes().as_slice(), index, chunk],
                )
                .map_err(|error| JournalError::sqlite("insert journal content chunk", error))?;
        }
        let stored: Option<Vec<u8>> = transaction
            .query_row(
                "SELECT payload FROM journal_content_chunks
                  WHERE content_digest = ?1 AND chunk_index = ?2",
                params![digest.as_bytes().as_slice(), index],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| JournalError::sqlite("verify journal content chunk", error))?;
        if stored.as_deref() != Some(chunk) {
            return Err(corrupt("journal content chunk identity is bound to different bytes"));
        }
    }
    let stored_chunks: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM journal_content_chunks WHERE content_digest = ?1",
            [digest.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| JournalError::sqlite("count journal content chunks", error))?;
    if stored_chunks != chunk_count_i64 {
        return Err(corrupt("journal content retains a noncanonical chunk set"));
    }
    Ok(byte_length)
}

/// Restores and authenticates exact content from bounded physical rows.
pub(super) fn restore(
    connection: &Connection,
    digest: Sha256Digest,
    byte_length: i64,
    maximum_bytes: usize,
) -> Result<Vec<u8>, JournalError> {
    let byte_length = usize::try_from(byte_length)
        .map_err(|_| corrupt("journal content length is negative or unrepresentable"))?;
    validate_length(byte_length, maximum_bytes)?;
    let expected_chunks = chunk_count(byte_length);
    let header: Option<(i64, i64)> = connection
        .query_row(
            "SELECT byte_length, chunk_count FROM journal_content WHERE content_digest = ?1",
            [digest.as_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| JournalError::sqlite("read journal content header", error))?;
    let expected_header = (
        to_i64(byte_length, "journal content length is not representable")?,
        to_i64(expected_chunks, "journal content chunk count is not representable")?,
    );
    if header != Some(expected_header) {
        return Err(corrupt("journal content header differs from its owner metadata"));
    }
    let mut statement = connection
        .prepare(
            "SELECT chunk_index, payload FROM journal_content_chunks
              WHERE content_digest = ?1 ORDER BY chunk_index",
        )
        .map_err(|error| JournalError::sqlite("prepare journal content chunks", error))?;
    let rows = statement
        .query_map([digest.as_bytes().as_slice()], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(|error| JournalError::sqlite("query journal content chunks", error))?;
    let mut bytes = Vec::with_capacity(byte_length);
    let mut observed_chunks = 0_usize;
    for row in rows {
        let (stored_index, chunk) =
            row.map_err(|error| JournalError::sqlite("read journal content chunk", error))?;
        if usize::try_from(stored_index).ok() != Some(observed_chunks) {
            return Err(corrupt("journal content chunk indexes are not contiguous"));
        }
        let remaining = byte_length.saturating_sub(bytes.len());
        let expected_bytes = remaining.min(CHUNK_BYTES);
        if chunk.len() != expected_bytes || expected_bytes == 0 {
            return Err(corrupt("journal content chunk has a noncanonical length"));
        }
        bytes.extend_from_slice(&chunk);
        observed_chunks = observed_chunks
            .checked_add(1)
            .ok_or_else(|| corrupt("journal content chunk count overflowed"))?;
    }
    if observed_chunks != expected_chunks || bytes.len() != byte_length {
        return Err(corrupt("journal content chunks do not cover their declared length"));
    }
    if peritus_codec::sha256(&bytes) != digest {
        return Err(corrupt("restored journal content does not match its digest"));
    }
    Ok(bytes)
}

/// Decodes a version-four content reference or one accepted legacy inline value.
pub(super) fn restore_or_inline(
    connection: &Connection,
    inline: Vec<u8>,
    digest: Sha256Digest,
    byte_length: Option<i64>,
    maximum_bytes: usize,
) -> Result<Vec<u8>, JournalError> {
    match byte_length {
        Some(byte_length) => {
            if inline != PAGED_INLINE_VALUE {
                return Err(corrupt("paged journal content also retains inline bytes"));
            }
            restore(connection, digest, byte_length, maximum_bytes)
        }
        None => {
            validate_length(inline.len(), maximum_bytes)?;
            if peritus_codec::sha256(&inline) != digest {
                return Err(corrupt("legacy inline journal content does not match its digest"));
            }
            Ok(inline)
        }
    }
}

fn validate_length(length: usize, maximum_bytes: usize) -> Result<(), JournalError> {
    if maximum_bytes > MAX_CONTENT_BYTES || length > maximum_bytes {
        return Err(corrupt("journal content length exceeds its owner bound"));
    }
    Ok(())
}

const fn chunk_count(byte_length: usize) -> usize {
    if byte_length == 0 { 0 } else { byte_length.div_ceil(CHUNK_BYTES) }
}

fn to_i64(value: usize, detail: &'static str) -> Result<i64, JournalError> {
    i64::try_from(value).map_err(|_| corrupt(detail))
}

const fn corrupt(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::CorruptJournal, "validate journal content", detail)
}
