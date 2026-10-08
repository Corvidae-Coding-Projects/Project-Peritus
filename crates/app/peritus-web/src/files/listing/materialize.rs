//! Detached directory enumeration and durable SQLite index publication.

use super::{
    Entry, ListingStore, MATERIALIZATION_BATCH, MaterializationPhase, MaterializationRequest,
    MaterializationState, PAGE_SIZE, SCHEMA_VERSION, path_identity, save_state,
};
use crate::{
    error::{Result, problem, uncertain},
    git::effects,
    state::hex,
};
use rusqlite::{Connection, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    path::Path,
};

struct Owner(File);

impl Drop for Owner {
    fn drop(&mut self) {
        if let Err(error) = fs4::FileExt::unlock(&self.0) {
            eprintln!("peritus web: directory-index ownership unlock failed: {error}");
        }
    }
}

pub(super) fn run(request_path: &Path) -> Result<()> {
    let request_path = request_path.canonicalize()?;
    let request: MaterializationRequest =
        serde_json::from_slice(&std::fs::read(&request_path)?)?;
    let store = ListingStore::from_request(&request_path, &request)?;
    let activation = store.activation(&request.materialization)?;
    fs4::FileExt::lock(&activation).map_err(problem)?;
    let mut state = store.state(&request.materialization)?;
    super::validate_state(&request, &state)?;
    let retained_owner = state
        .owner
        .as_ref()
        .ok_or_else(|| uncertain("The directory-index owner has no durable launch identity"))?;
    let current_owner = effects::detached_launcher_binding(std::process::id())?;
    if &current_owner != retained_owner {
        return Err(uncertain(
            "The directory-index process differs from its durable launch identity",
        ));
    }
    let owner = store.owner(&request.materialization)?;
    fs4::FileExt::try_lock(&owner).map_err(|error| {
        uncertain(format!(
            "The directory materialization already has an owner: {error}"
        ))
    })?;
    let _owner = Owner(owner);
    fs4::FileExt::unlock(&activation).map_err(problem)?;
    let directory = store.materialization_directory(&request.materialization)?;
    state.phase = MaterializationPhase::Indexing;
    save_state(&directory, &state)?;

    match create(&request, &directory, &mut state) {
        Ok(digest) => {
            state.phase = MaterializationPhase::Completed;
            state.digest = Some(digest);
            save_state(&directory, &state)
        }
        Err(error) => {
            state.phase = MaterializationPhase::Failed;
            state.error = Some(error.0.clone());
            save_state(&directory, &state).map_err(|publish| {
                uncertain(format!(
                    "{}; additionally failed to publish directory-index failure: {}",
                    error.0, publish.0
                ))
            })?;
            Err(error)
        }
    }
}

fn create(
    request: &MaterializationRequest,
    directory: &Path,
    state: &mut MaterializationState,
) -> Result<String> {
    validate_source(request)?;
    let path = directory.join("entries.sqlite3");
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let mut connection = Connection::open(&path).map_err(problem)?;
    connection.pragma_update(None, "journal_mode", "DELETE").map_err(problem)?;
    connection.pragma_update(None, "synchronous", "FULL").map_err(problem)?;
    connection
        .execute_batch(
            "CREATE TABLE entries (
                 directory_rank INTEGER NOT NULL,
                 folded_name TEXT NOT NULL,
                 name TEXT NOT NULL,
                 path TEXT NOT NULL UNIQUE,
                 directory INTEGER NOT NULL,
                 directory_identity TEXT,
                 symlink INTEGER NOT NULL,
                 bytes TEXT NOT NULL
             ) STRICT;
             CREATE TABLE boundaries (
                 position INTEGER PRIMARY KEY,
                 directory_rank INTEGER NOT NULL,
                 folded_name TEXT NOT NULL,
                 name TEXT NOT NULL,
                 path TEXT NOT NULL
             ) STRICT;",
        )
        .map_err(problem)?;
    scan(request, directory, state, &mut connection)?;
    validate_source(request)?;
    state.phase = MaterializationPhase::Ordering;
    save_state(directory, state)?;
    connection
        .execute_batch(
            "CREATE INDEX entries_order
                 ON entries(directory_rank,folded_name,name,path);",
        )
        .map_err(problem)?;
    let digest = snapshot_digest(request, state.matched, &connection)?;
    let page = i64::try_from(PAGE_SIZE).map_err(problem)?;
    connection
        .execute(
            "INSERT INTO boundaries(position,directory_rank,folded_name,name,path)
             SELECT position,directory_rank,folded_name,name,path
             FROM (
                 SELECT row_number() OVER (
                            ORDER BY directory_rank,folded_name,name,path
                        ) AS position,
                        directory_rank,folded_name,name,path
                 FROM entries
             )
             WHERE position % ?1 = 0",
            [page],
        )
        .map_err(problem)?;
    connection
        .execute_batch(
            "CREATE TABLE metadata (
                 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
                 schema_version INTEGER NOT NULL,
                 workspace TEXT NOT NULL,
                 store TEXT NOT NULL,
                 materialization TEXT NOT NULL,
                 directory_identity TEXT NOT NULL,
                 filter TEXT NOT NULL,
                 matched TEXT NOT NULL,
                 digest TEXT NOT NULL
             ) STRICT;",
        )
        .map_err(problem)?;
    connection
        .execute(
            "INSERT INTO metadata(
                 singleton,schema_version,workspace,store,materialization,
                 directory_identity,filter,matched,digest
             ) VALUES (1,?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                i64::from(SCHEMA_VERSION),
                request.workspace,
                request.store,
                request.materialization,
                request.directory_identity,
                request.filter,
                state.matched.to_string(),
                digest,
            ],
        )
        .map_err(problem)?;
    connection
        .execute_batch("PRAGMA optimize;")
        .map_err(problem)?;
    drop(connection);
    File::open(&path)?.sync_all()?;
    Ok(digest)
}

fn scan(
    request: &MaterializationRequest,
    state_directory: &Path,
    state: &mut MaterializationState,
    connection: &mut Connection,
) -> Result<()> {
    let folded_filter = request.filter.to_lowercase();
    let mut batch = Vec::with_capacity(MATERIALIZATION_BATCH);
    let mut scanned_in_batch = 0_usize;
    for entry in std::fs::read_dir(&request.directory)? {
        let entry = entry?;
        state.scanned = state.scanned.checked_add(1).ok_or_else(|| {
            problem("The directory source entry count exceeds its durable representation")
        })?;
        scanned_in_batch += 1;
        let metadata = entry.metadata()?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let folded_name = name.to_lowercase();
        if folded_filter.is_empty() || folded_name.contains(&folded_filter) {
            let path = entry
                .path()
                .strip_prefix(&request.project_root)
                .map_err(problem)?
                .to_string_lossy()
                .into_owned();
            let is_directory = metadata.is_dir();
            let directory_identity = if is_directory {
                entry
                    .path()
                    .canonicalize()
                    .ok()
                    .filter(|target| target.starts_with(&request.project_root))
                    .map(|target| path_identity(&target))
            } else {
                None
            };
            batch.push((
                Entry {
                    name,
                    path,
                    directory: is_directory,
                    directory_identity,
                    symlink: entry.file_type()?.is_symlink(),
                    bytes: metadata.len(),
                },
                folded_name,
            ));
        }
        if scanned_in_batch == MATERIALIZATION_BATCH {
            publish_batch(connection, &mut batch, state)?;
            scanned_in_batch = 0;
            save_state(state_directory, state)?;
        }
    }
    if scanned_in_batch > 0 {
        publish_batch(connection, &mut batch, state)?;
        save_state(state_directory, state)?;
    }
    Ok(())
}

fn publish_batch(
    connection: &mut Connection,
    batch: &mut Vec<(Entry, String)>,
    state: &mut MaterializationState,
) -> Result<()> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(problem)?;
    {
        let mut insert = transaction
            .prepare(
                "INSERT INTO entries(
                     directory_rank,folded_name,name,path,directory,
                     directory_identity,symlink,bytes
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            )
            .map_err(problem)?;
        for (entry, folded_name) in batch.drain(..) {
            insert
                .execute(params![
                    if entry.directory { 0_i64 } else { 1_i64 },
                    folded_name,
                    entry.name,
                    entry.path,
                    entry.directory,
                    entry.directory_identity,
                    entry.symlink,
                    entry.bytes.to_string(),
                ])
                .map_err(problem)?;
            state.matched = state.matched.checked_add(1).ok_or_else(|| {
                problem("The filtered directory count exceeds its durable representation")
            })?;
        }
    }
    transaction.commit().map_err(problem)
}

fn snapshot_digest(
    request: &MaterializationRequest,
    expected_count: u64,
    connection: &Connection,
) -> Result<String> {
    let mut digest = Sha256::new();
    digest.update(b"peritus-web-directory-snapshot-v1\0");
    digest.update(serde_json::to_vec(&(
        &request.directory_identity,
        &request.filter,
    ))?);
    let mut statement = connection
        .prepare(
            "SELECT name,path,directory,directory_identity,symlink,bytes
             FROM entries
             ORDER BY directory_rank,folded_name,name,path",
        )
        .map_err(problem)?;
    let mut rows = statement.query([]).map_err(problem)?;
    let mut count = 0_u64;
    while let Some(row) = rows.next().map_err(problem)? {
        let bytes = row.get::<_, String>(5).map_err(problem)?.parse::<u64>().map_err(problem)?;
        let entry = Entry {
            name: row.get(0).map_err(problem)?,
            path: row.get(1).map_err(problem)?,
            directory: row.get(2).map_err(problem)?,
            directory_identity: row.get(3).map_err(problem)?,
            symlink: row.get(4).map_err(problem)?,
            bytes,
        };
        digest.update(serde_json::to_vec(&entry)?);
        count = count.checked_add(1).ok_or_else(|| {
            problem("The directory snapshot count exceeds its durable representation")
        })?;
    }
    if count != expected_count {
        return Err(uncertain(
            "The directory index row count differs from its durable progress state",
        ));
    }
    Ok(hex(&digest.finalize()))
}

fn validate_source(request: &MaterializationRequest) -> Result<()> {
    if request.project_root.canonicalize()? != request.project_root
        || request.directory.canonicalize()? != request.directory
        || !request.project_root.is_dir()
        || !request.directory.is_dir()
        || !request.directory.starts_with(&request.project_root)
        || path_identity(&request.directory) != request.directory_identity
    {
        return Err(problem(
            "The directory source changed identity while it was being indexed",
        ));
    }
    Ok(())
}
