//! Lossless, bounded, content-addressed immutable state history.

mod node;

#[cfg(test)]
mod tests;

use rusqlite::{Connection, OptionalExtension, Statement, Transaction, params};

use crate::{JournalError, JournalErrorKind};
use peritus_types::Sha256Digest;

use node::{FANOUT, Node, root_level};

/// Installs every exact node in the append transaction, checking reused bytes before accepting it.
pub(super) fn install(
    transaction: &Transaction<'_>,
    bytes: &[u8],
) -> Result<Sha256Digest, JournalError> {
    let level = root_level(bytes.len())?;
    let mut read = prepare_node_read(transaction)?;
    let mut insert = transaction
        .prepare(
            "INSERT INTO state_history_nodes(node_digest, level, byte_length, payload)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .map_err(|error| JournalError::sqlite("prepare history node insert", error))?;
    install_subtree(&mut read, &mut insert, bytes, level)
}

fn install_subtree(
    read: &mut Statement<'_>,
    insert: &mut Statement<'_>,
    bytes: &[u8],
    level: u8,
) -> Result<Sha256Digest, JournalError> {
    let payload = if level == 0 {
        bytes.to_vec()
    } else {
        let child_capacity = node::capacity(level - 1)?;
        let mut payload = Vec::with_capacity(FANOUT * 32);
        for chunk in bytes.chunks(child_capacity) {
            payload.extend_from_slice(install_subtree(read, insert, chunk, level - 1)?.as_bytes());
        }
        payload
    };
    let node = Node::new(level, bytes.len(), payload)?;
    let digest = node.digest();
    if let Some(stored) = load_node(read, digest)? {
        if stored != node {
            return Err(corrupt("existing history node differs from exact proposed bytes"));
        }
    } else {
        insert
            .execute(params![
                digest.as_bytes().as_slice(),
                i64::from(level),
                super::append::to_i64(bytes.len() as u64, "history node byte length")?,
                node.payload()
            ])
            .map_err(|error| JournalError::sqlite("insert immutable history node", error))?;
    }
    Ok(digest)
}

/// Reconstructs a complete checked tree in the caller's read snapshot.
pub fn restore(connection: &Connection, root: &[u8]) -> Result<Vec<u8>, JournalError> {
    let digest = super::query::digest_from_blob(root, "history root digest")?;
    let mut read = prepare_node_read(connection)?;
    let node = required_node(&mut read, digest)?;
    if node.level() != root_level(node.byte_length())? {
        return Err(corrupt("history root does not have its canonical minimum height"));
    }
    let mut bytes = Vec::with_capacity(node.byte_length());
    restore_subtree(&mut read, &node, &mut bytes)?;
    Ok(bytes)
}

fn restore_subtree(
    read: &mut Statement<'_>,
    node: &Node,
    bytes: &mut Vec<u8>,
) -> Result<(), JournalError> {
    if node.level() == 0 {
        bytes.extend_from_slice(node.payload());
        return Ok(());
    }
    let capacity = node::capacity(node.level() - 1)?;
    let mut remaining = node.byte_length();
    for digest in node.payload().chunks_exact(32) {
        let child =
            required_node(read, super::query::digest_from_blob(digest, "history child digest")?)?;
        let expected_bytes = remaining.min(capacity);
        if child.level() != node.level() - 1 || child.byte_length() != expected_bytes {
            return Err(corrupt(
                "history child height or byte length differs from canonical shape",
            ));
        }
        restore_subtree(read, &child, bytes)?;
        remaining -= expected_bytes;
    }
    if remaining != 0 {
        return Err(corrupt("history tree did not reconstruct its declared length"));
    }
    Ok(())
}

fn required_node(read: &mut Statement<'_>, digest: Sha256Digest) -> Result<Node, JournalError> {
    load_node(read, digest)?.ok_or_else(|| corrupt("history tree references a missing node"))
}

// Statements live for one tree operation in its existing transaction. Every node is still read
// and checked on every visit; no row bytes or validation result are cached across visits.
fn prepare_node_read(connection: &Connection) -> Result<Statement<'_>, JournalError> {
    connection
        .prepare(
            "SELECT level, byte_length, payload FROM state_history_nodes WHERE node_digest = ?1",
        )
        .map_err(|error| JournalError::sqlite("prepare history node read", error))
}

fn load_node(read: &mut Statement<'_>, digest: Sha256Digest) -> Result<Option<Node>, JournalError> {
    let row: Option<(i64, i64, Vec<u8>)> = read
        .query_row([digest.as_bytes().as_slice()], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .optional()
        .map_err(|error| JournalError::sqlite("read history node", error))?;
    row.map(|(level, length, payload)| {
        let level = u8::try_from(level).map_err(|_| corrupt("history node height is invalid"))?;
        let length =
            usize::try_from(length).map_err(|_| corrupt("history node length is invalid"))?;
        let node = Node::new(level, length, payload)?;
        if node.digest() != digest {
            return Err(corrupt("history node digest does not match exact encoded node"));
        }
        Ok(node)
    })
    .transpose()
}

const fn corrupt(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::CorruptJournal, "validate immutable state history", detail)
}
