//! Append-only durable publication for source-backed obligation page roots.

use std::{path::Path, thread, time::Duration};

use peritus_obligations::{SourceObligationLedgerRoot, SourceObligationPage};
use peritus_types::Sha256Digest;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::{ProductRunInput, check_cancelled};
use crate::{ProductRunnerError, ProductRunnerErrorKind};

const RETRY_DELAY: Duration = Duration::from_millis(20);
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS obligation_pages_v2 (
    digest BLOB PRIMARY KEY NOT NULL CHECK(length(digest) = 32),
    canonical BLOB NOT NULL,
    CHECK(length(canonical) > 0)
);
CREATE TABLE IF NOT EXISTS obligation_roots_v2 (
    catalog_binding BLOB PRIMARY KEY NOT NULL CHECK(length(catalog_binding) = 32),
    canonical BLOB NOT NULL,
    digest BLOB NOT NULL CHECK(length(digest) = 32)
);
CREATE TRIGGER IF NOT EXISTS obligation_pages_v2_no_update
BEFORE UPDATE ON obligation_pages_v2 BEGIN SELECT RAISE(ABORT, 'immutable obligation page'); END;
CREATE TRIGGER IF NOT EXISTS obligation_pages_v2_no_delete
BEFORE DELETE ON obligation_pages_v2 BEGIN SELECT RAISE(ABORT, 'immutable obligation page'); END;
CREATE TRIGGER IF NOT EXISTS obligation_roots_v2_no_update
BEFORE UPDATE ON obligation_roots_v2 BEGIN SELECT RAISE(ABORT, 'immutable obligation root'); END;
CREATE TRIGGER IF NOT EXISTS obligation_roots_v2_no_delete
BEFORE DELETE ON obligation_roots_v2 BEGIN SELECT RAISE(ABORT, 'immutable obligation root'); END;
";

pub(super) struct ObligationStore {
    connection: Connection,
}

impl ObligationStore {
    pub(super) fn open(
        path: &Path,
        input: &ProductRunInput,
    ) -> Result<Self, ProductRunnerError> {
        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(repository)?;
        }
        let connection = Connection::open(path).map_err(repository)?;
        connection.busy_timeout(Duration::ZERO).map_err(repository)?;
        let mut store = Self { connection };
        store.retry(input, |connection| {
            connection.pragma_update(None, "synchronous", "EXTRA")?;
            connection.pragma_update(None, "foreign_keys", true)?;
            connection.execute_batch(SCHEMA)
        })?;
        Ok(store)
    }

    pub(super) fn load_root(
        &mut self,
        input: &ProductRunInput,
        catalog_binding: Sha256Digest,
    ) -> Result<Option<SourceObligationLedgerRoot>, ProductRunnerError> {
        let row = self.retry(input, |connection| {
            connection
                .query_row(
                    "SELECT canonical, digest FROM obligation_roots_v2
                     WHERE catalog_binding = ?1",
                    params![catalog_binding.as_bytes().as_slice()],
                    |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
                )
                .optional()
        })?;
        let Some((canonical, digest)) = row else { return Ok(None) };
        let digest = decode_digest(&digest, "obligation root digest")?;
        let root = SourceObligationLedgerRoot::restore(&canonical, digest).map_err(corrupt)?;
        self.verify_pages(input, catalog_binding, root)?;
        Ok(Some(root))
    }

    pub(super) fn append_page(
        &mut self,
        input: &ProductRunInput,
        page: SourceObligationPage,
    ) -> Result<(), ProductRunnerError> {
        let canonical = page.canonical_bytes();
        loop {
            check_cancelled(input)?;
            let transaction = match self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
            {
                Ok(transaction) => transaction,
                Err(error) if recoverable(&error) => {
                    thread::sleep(RETRY_DELAY);
                    continue;
                }
                Err(error) => return Err(repository(error)),
            };
            let existing = match transaction
                .query_row(
                    "SELECT canonical, digest FROM obligation_pages_v2
                     WHERE digest = ?1",
                    params![page.digest().as_bytes().as_slice()],
                    |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
                )
                .optional()
            {
                Ok(existing) => existing,
                Err(error) if recoverable(&error) => {
                    drop(transaction);
                    thread::sleep(RETRY_DELAY);
                    continue;
                }
                Err(error) => return Err(repository(error)),
            };
            if let Some((stored, digest)) = existing {
                if stored != canonical || digest.as_slice() != page.digest().as_bytes() {
                    return Err(invariant("a durable obligation page conflicts with its identity"));
                }
                return Ok(());
            }
            if let Err(error) = transaction
                .execute(
                    "INSERT INTO obligation_pages_v2(
                        digest, canonical
                     ) VALUES (?1, ?2)",
                    params![
                        page.digest().as_bytes().as_slice(),
                        canonical.as_slice(),
                    ],
                )
            {
                if recoverable(&error) {
                    drop(transaction);
                    thread::sleep(RETRY_DELAY);
                    continue;
                }
                return Err(repository(error));
            }
            match transaction.commit() {
                Ok(()) => return Ok(()),
                Err(error) => {
                    let exact = self.page_exact(input, page)?;
                    if exact {
                        return Ok(());
                    }
                    if recoverable(&error) {
                        thread::sleep(RETRY_DELAY);
                        continue;
                    }
                    return Err(repository(error));
                }
            }
        }
    }

    /// Publishes the complete root after every page is durable. `true` means this call adopted
    /// the first V2 root for the catalog, including an acknowledged-lost commit it reconciled.
    pub(super) fn publish_root(
        &mut self,
        input: &ProductRunInput,
        root: SourceObligationLedgerRoot,
    ) -> Result<bool, ProductRunnerError> {
        self.verify_pages(input, root.catalog_binding(), root)?;
        let canonical = root.canonical_bytes();
        loop {
            check_cancelled(input)?;
            let transaction = match self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
            {
                Ok(transaction) => transaction,
                Err(error) if recoverable(&error) => {
                    thread::sleep(RETRY_DELAY);
                    continue;
                }
                Err(error) => return Err(repository(error)),
            };
            let existing = match transaction
                .query_row(
                    "SELECT canonical, digest FROM obligation_roots_v2
                     WHERE catalog_binding = ?1",
                    params![root.catalog_binding().as_bytes().as_slice()],
                    |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
                )
                .optional()
            {
                Ok(existing) => existing,
                Err(error) if recoverable(&error) => {
                    drop(transaction);
                    thread::sleep(RETRY_DELAY);
                    continue;
                }
                Err(error) => return Err(repository(error)),
            };
            if let Some((stored, digest)) = existing {
                if stored != canonical || digest.as_slice() != root.digest().as_bytes() {
                    return Err(invariant("a durable obligation root conflicts with its catalog"));
                }
                return Ok(false);
            }
            if let Err(error) = transaction
                .execute(
                    "INSERT INTO obligation_roots_v2(catalog_binding, canonical, digest)
                     VALUES (?1, ?2, ?3)",
                    params![
                        root.catalog_binding().as_bytes().as_slice(),
                        canonical.as_slice(),
                        root.digest().as_bytes().as_slice(),
                    ],
                )
            {
                if recoverable(&error) {
                    drop(transaction);
                    thread::sleep(RETRY_DELAY);
                    continue;
                }
                return Err(repository(error));
            }
            match transaction.commit() {
                Ok(()) => return Ok(true),
                Err(error) => {
                    if self.root_exact(input, root)? {
                        return Ok(true);
                    }
                    if recoverable(&error) {
                        thread::sleep(RETRY_DELAY);
                        continue;
                    }
                    return Err(repository(error));
                }
            }
        }
    }

    fn verify_pages(
        &mut self,
        input: &ProductRunInput,
        catalog_binding: Sha256Digest,
        root: SourceObligationLedgerRoot,
    ) -> Result<(), ProductRunnerError> {
        if root.catalog_binding() != catalog_binding {
            return Err(invariant("the durable obligation root has the wrong catalog binding"));
        }
        let mut expected_index = root.page_count();
        let mut expected_digest = root.final_page_digest();
        while expected_index > 0 {
            let row = self.retry(input, |connection| {
                connection
                    .query_row(
                        "SELECT canonical, digest FROM obligation_pages_v2 WHERE digest = ?1",
                        params![expected_digest.as_bytes().as_slice()],
                        |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
                    )
                    .optional()
            })?;
            let Some((canonical, digest)) = row else {
                return Err(invariant("the durable obligation root references a missing page"));
            };
            let digest = decode_digest(&digest, "obligation page digest")?;
            let page = SourceObligationPage::restore(&canonical, digest).map_err(corrupt)?;
            if page.digest() != expected_digest
                || page.page_index() != expected_index
                || page.conversation_revision() != root.conversation_revision()
            {
                return Err(invariant("the durable obligation page chain is not contiguous"));
            }
            expected_digest = page.previous_page_digest();
            expected_index -= 1;
        }
        if expected_digest != Sha256Digest::new([0; 32]) {
            return Err(invariant("the durable obligation root does not match its page chain"));
        }
        Ok(())
    }

    fn page_exact(
        &mut self,
        input: &ProductRunInput,
        page: SourceObligationPage,
    ) -> Result<bool, ProductRunnerError> {
        let expected = page.canonical_bytes();
        self.retry(input, |connection| {
            connection
                .query_row(
                    "SELECT canonical, digest FROM obligation_pages_v2
                     WHERE digest = ?1",
                    params![page.digest().as_bytes().as_slice()],
                    |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
                )
                .optional()
        })
        .map(|value| {
            value.is_some_and(|(canonical, digest)| {
                canonical == expected && digest.as_slice() == page.digest().as_bytes()
            })
        })
    }

    fn root_exact(
        &mut self,
        input: &ProductRunInput,
        root: SourceObligationLedgerRoot,
    ) -> Result<bool, ProductRunnerError> {
        let expected = root.canonical_bytes();
        self.retry(input, |connection| {
            connection
                .query_row(
                    "SELECT canonical, digest FROM obligation_roots_v2
                     WHERE catalog_binding = ?1",
                    params![root.catalog_binding().as_bytes().as_slice()],
                    |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
                )
                .optional()
        })
        .map(|value| {
            value.is_some_and(|(canonical, digest)| {
                canonical == expected && digest.as_slice() == root.digest().as_bytes()
            })
        })
    }

    fn retry<T>(
        &mut self,
        input: &ProductRunInput,
        mut operation: impl FnMut(&mut Connection) -> Result<T, rusqlite::Error>,
    ) -> Result<T, ProductRunnerError> {
        loop {
            check_cancelled(input)?;
            match operation(&mut self.connection) {
                Ok(value) => return Ok(value),
                Err(error) if recoverable(&error) => thread::sleep(RETRY_DELAY),
                Err(error) => return Err(repository(error)),
            }
        }
    }
}

fn decode_digest(bytes: &[u8], subject: &str) -> Result<Sha256Digest, ProductRunnerError> {
    let exact: [u8; 32] = bytes
        .try_into()
        .map_err(|_| invariant(format!("{subject} has the wrong width")))?;
    Ok(Sha256Digest::new(exact))
}

fn recoverable(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if matches!(
                failure.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            )
    )
}

fn repository(error: impl std::fmt::Display) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::Repository,
        "persist source-backed obligation ledger",
        error.to_string(),
    )
}

fn corrupt(error: impl std::fmt::Display) -> ProductRunnerError {
    invariant(error.to_string())
}

fn invariant(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InternalInvariant,
        "restore source-backed obligation ledger",
        detail,
    )
}
