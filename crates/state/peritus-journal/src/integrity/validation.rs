//! Referential and exact-byte validation for mutable journal catalogs.

use peritus_codec::sha256;
use peritus_types::{CommandId, Sha256Digest};
use rusqlite::{OptionalExtension, Transaction};
use std::collections::BTreeMap;

use super::corrupt;
use crate::{CommandResolution, JournalError, StoreId};

pub(super) fn validate_commands(
    transaction: &Transaction<'_>,
    store_id: StoreId,
) -> Result<(), JournalError> {
    let commands = {
        let mut statement = transaction
            .prepare("SELECT command_id, request_digest FROM commands ORDER BY first_position")
            .map_err(|error| JournalError::sqlite("prepare command integrity", error))?;
        let rows = statement
            .query_map([], |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)))
            .map_err(|error| JournalError::sqlite("query command integrity", error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| JournalError::sqlite("read command integrity", error))?
    };
    for (command, digest) in commands {
        let command =
            CommandId::new(crate::sqlite::query::array_from_blob(&command, "command identity")?)
                .map_err(|_| corrupt("invalid stored command identity"))?;
        let digest = crate::sqlite::query::digest_from_blob(&digest, "command digest")?;
        if !matches!(
            crate::sqlite::query::resolve_command(transaction, store_id, command, digest)?,
            CommandResolution::Committed(_)
        ) {
            return Err(corrupt("command resolution changed during integrity scan"));
        }
    }
    let orphan_events: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM events e LEFT JOIN commands c ON c.command_id = e.command_id WHERE c.command_id IS NULL)",
            [],
            |row| row.get(0),
        )
        .map_err(|error| JournalError::sqlite("check orphan event commands", error))?;
    if orphan_events { Err(corrupt("event refers to an absent command result")) } else { Ok(()) }
}

pub(super) fn validate_state_records(transaction: &Transaction<'_>) -> Result<(), JournalError> {
    let history = load_state_history(transaction)?;
    let current = load_current_state(transaction)?;
    if current != history {
        return Err(corrupt(
            "current state records do not exactly match their latest immutable history rows",
        ));
    }
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
struct StateObservation {
    revision: u64,
    digest: Sha256Digest,
    bytes: Vec<u8>,
    producing_position: u64,
}

type StateCatalog = BTreeMap<(u16, Vec<u8>), StateObservation>;

fn load_state_history(transaction: &Transaction<'_>) -> Result<StateCatalog, JournalError> {
    let mut statement = transaction
        .prepare(
            "SELECT h.namespace, h.record_key, h.revision, h.value_digest, h.value,
                    h.producing_position, e.global_position
               FROM state_record_history AS h
          LEFT JOIN events AS e ON e.global_position = h.producing_position
           ORDER BY h.namespace, h.record_key, h.revision",
        )
        .map_err(|error| JournalError::sqlite("prepare state history integrity", error))?;
    let mut rows = statement
        .query([])
        .map_err(|error| JournalError::sqlite("query state history integrity", error))?;
    let mut latest = StateCatalog::new();
    while let Some(row) =
        rows.next().map_err(|error| JournalError::sqlite("read state history integrity", error))?
    {
        let namespace = state_namespace(
            row.get(0)
                .map_err(|error| JournalError::sqlite("decode state history namespace", error))?,
        )?;
        let key: Vec<u8> =
            row.get(1).map_err(|error| JournalError::sqlite("decode state history key", error))?;
        let revision = positive(
            row.get(2)
                .map_err(|error| JournalError::sqlite("decode state history revision", error))?,
        )?;
        let raw_digest: Vec<u8> = row
            .get(3)
            .map_err(|error| JournalError::sqlite("decode state history digest", error))?;
        let bytes: Vec<u8> = row
            .get(4)
            .map_err(|error| JournalError::sqlite("decode state history bytes", error))?;
        let producing_position = positive(row.get(5).map_err(|error| {
            JournalError::sqlite("decode state history producing position", error)
        })?)?;
        let producing_event: Option<i64> = row
            .get(6)
            .map_err(|error| JournalError::sqlite("decode state history producer", error))?;
        if producing_event.is_none() {
            return Err(corrupt("state history refers to an absent producing event"));
        }
        let digest = crate::sqlite::query::digest_from_blob(&raw_digest, "state history digest")?;
        if digest != sha256(&bytes) {
            return Err(corrupt("state history digest does not match exact bytes"));
        }
        let catalog_key = (namespace, key);
        let expected = match latest.get(&catalog_key) {
            Some(previous) => previous
                .revision
                .checked_add(1)
                .ok_or_else(|| corrupt("state history revision overflowed"))?,
            None => 1,
        };
        if revision != expected {
            return Err(corrupt("state history revisions are not contiguous from one"));
        }
        latest
            .insert(catalog_key, StateObservation { revision, digest, bytes, producing_position });
    }
    Ok(latest)
}

fn load_current_state(transaction: &Transaction<'_>) -> Result<StateCatalog, JournalError> {
    let mut statement = transaction
        .prepare(
            "SELECT namespace, record_key, revision, value_digest, value, producing_position
               FROM state_records
           ORDER BY namespace, record_key",
        )
        .map_err(|error| JournalError::sqlite("prepare current state integrity", error))?;
    let mut rows = statement
        .query([])
        .map_err(|error| JournalError::sqlite("query current state integrity", error))?;
    let mut current = StateCatalog::new();
    while let Some(row) =
        rows.next().map_err(|error| JournalError::sqlite("read current state integrity", error))?
    {
        let namespace = state_namespace(
            row.get(0).map_err(|error| JournalError::sqlite("decode state namespace", error))?,
        )?;
        let key: Vec<u8> =
            row.get(1).map_err(|error| JournalError::sqlite("decode state key", error))?;
        let revision = positive(
            row.get(2).map_err(|error| JournalError::sqlite("decode state revision", error))?,
        )?;
        let raw_digest: Vec<u8> =
            row.get(3).map_err(|error| JournalError::sqlite("decode state digest", error))?;
        let bytes: Vec<u8> =
            row.get(4).map_err(|error| JournalError::sqlite("decode state bytes", error))?;
        let producing_position =
            positive(row.get(5).map_err(|error| {
                JournalError::sqlite("decode state producing position", error)
            })?)?;
        let digest = crate::sqlite::query::digest_from_blob(&raw_digest, "state digest")?;
        if digest != sha256(&bytes) {
            return Err(corrupt("state record digest does not match exact bytes"));
        }
        if current
            .insert(
                (namespace, key),
                StateObservation { revision, digest, bytes, producing_position },
            )
            .is_some()
        {
            return Err(corrupt("state catalog contains a duplicate current key"));
        }
    }
    Ok(current)
}

fn state_namespace(value: i64) -> Result<u16, JournalError> {
    let value = u16::try_from(value)
        .map_err(|_| corrupt("state namespace is outside its checked range"))?;
    if value == 0 { Err(corrupt("state namespace is reserved")) } else { Ok(value) }
}

fn positive(value: i64) -> Result<u64, JournalError> {
    let value = u64::try_from(value).map_err(|_| corrupt("state integer is negative"))?;
    if value == 0 { Err(corrupt("state integer is zero")) } else { Ok(value) }
}

pub(super) fn validate_registry(transaction: &Transaction<'_>) -> Result<(), JournalError> {
    let row: Option<(Vec<u8>, Vec<u8>)> = transaction
        .query_row(
            "SELECT snapshot_digest, snapshot FROM credential_registry WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| JournalError::sqlite("query registry integrity", error))?;
    if let Some((digest, bytes)) = row {
        let snapshot = crate::ExactFrame::new(bytes)
            .map_err(|_| corrupt("credential registry snapshot is not a canonical frame"))?;
        let payload_digest = crate::authority::credential_registry_payload_digest(&snapshot)
            .map_err(|_| corrupt("credential registry snapshot uses an unsupported schema"))?;
        if crate::sqlite::query::digest_from_blob(&digest, "registry digest")? != payload_digest {
            return Err(corrupt(
                "credential registry digest does not match the canonical snapshot payload",
            ));
        }
    }
    Ok(())
}
