//! Immutable run ordering and durable admission-frontier persistence.

use super::super::{ProductRunServiceError, RunRecord};
use peritus_app_protocol::{ProductRunPageCursor, ProductRunPageQuery, ProductRunStoreId};
use peritus_types::RunId;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::product_run) struct RunCatalogKey {
    sequence: u64,
    run: RunId,
}

impl RunCatalogKey {
    const fn new(sequence: u64, run: RunId) -> Self {
        Self { sequence, run }
    }

    pub(in crate::product_run) const fn sequence(self) -> u64 {
        self.sequence
    }

    pub(in crate::product_run) const fn run(self) -> RunId {
        self.run
    }
}

#[derive(Default)]
pub(in crate::product_run) struct RunCatalog {
    by_run: BTreeMap<RunId, u64>,
    by_sequence: BTreeMap<u64, RunId>,
    frontier: u64,
}

pub(in crate::product_run) struct RunCatalogSelection {
    pub(in crate::product_run) keys: Vec<RunCatalogKey>,
    pub(in crate::product_run) next: Option<ProductRunPageCursor>,
}

/// Assigns legacy records their immutable catalog membership before the service accepts work.
/// Records are persisted oldest-first so an interrupted migration resumes from the same frontier.
pub(in crate::product_run) fn migrate_sequences(
    directory: &std::path::Path,
    records: &mut BTreeMap<RunId, RunRecord>,
) -> Result<u64, ProductRunServiceError> {
    let mut used = BTreeSet::new();
    let mut frontier = 0_u64;
    for record in records.values().filter(|record| record.progress.catalog_sequence != 0) {
        if !used.insert(record.progress.catalog_sequence) {
            return Err(ProductRunServiceError::InvalidState);
        }
        frontier = frontier.max(record.progress.catalog_sequence);
    }
    let mut frontier = load_frontier(directory, frontier)?;
    let mut legacy = records
        .iter()
        .filter(|(_, record)| record.progress.catalog_sequence == 0)
        .map(|(run, record)| (record.progress.started_unix_millis, *run))
        .collect::<Vec<_>>();
    legacy.sort_unstable();
    for (_, run) in legacy {
        frontier = advance_frontier(directory, frontier)?;
        let record = records.get_mut(&run).ok_or(ProductRunServiceError::InvalidState)?;
        record.progress.catalog_sequence = frontier;
        super::super::publication::persist_startup_record(
            directory,
            record,
            peritus_codec::sha256(b"migrate-product-run-catalog-sequence"),
        )?;
    }
    Ok(frontier)
}

impl RunCatalog {
    pub(in crate::product_run) fn from_records(
        records: &BTreeMap<RunId, RunRecord>,
        frontier: u64,
    ) -> Result<Self, ProductRunServiceError> {
        let mut catalog = Self { frontier, ..Self::default() };
        for record in records.values() {
            catalog.replace(record)?;
        }
        Ok(catalog)
    }

    pub(in crate::product_run) fn replace(
        &mut self,
        record: &RunRecord,
    ) -> Result<(), ProductRunServiceError> {
        let run = record.snapshot.run_id();
        let sequence = record.progress.catalog_sequence;
        if sequence == 0
            || sequence > self.frontier
            || self.by_run.get(&run).is_some_and(|previous| *previous != sequence)
            || self.by_sequence.get(&sequence).is_some_and(|existing| *existing != run)
        {
            return Err(ProductRunServiceError::InvalidState);
        }
        if let Some(previous) = self.by_run.remove(&run) {
            self.by_sequence.remove(&previous);
        }
        self.by_run.insert(run, sequence);
        self.by_sequence.insert(sequence, run);
        Ok(())
    }

    pub(in crate::product_run) fn remove(&mut self, run: RunId) {
        if let Some(previous) = self.by_run.remove(&run) {
            self.by_sequence.remove(&previous);
        }
    }

    pub(in crate::product_run) fn allocate_sequence(
        &mut self,
        directory: &std::path::Path,
    ) -> Result<u64, ProductRunServiceError> {
        let next = advance_frontier(directory, self.frontier)?;
        self.frontier = next;
        Ok(next)
    }

    pub(in crate::product_run) fn select(
        &self,
        query: ProductRunPageQuery,
        store: ProductRunStoreId,
    ) -> Result<RunCatalogSelection, ProductRunServiceError> {
        let Some(highwater) = query
            .cursor()
            .map(|cursor| {
                if cursor.store() != store {
                    return Err(ProductRunServiceError::invalid_data(
                        "continue product-run history",
                        "cursor belongs to another durable store",
                    ));
                }
                if cursor.highwater_sequence() > self.frontier {
                    return Err(ProductRunServiceError::invalid_data(
                        "continue product-run history",
                        "cursor is newer than the durable admission frontier",
                    ));
                }
                Ok(RunCatalogKey::new(cursor.highwater_sequence(), cursor.highwater_run()))
            })
            .transpose()?
            .or_else(|| {
                self.by_sequence
                    .last_key_value()
                    .map(|(sequence, run)| RunCatalogKey::new(*sequence, *run))
            })
        else {
            return Ok(RunCatalogSelection { keys: Vec::new(), next: None });
        };

        let mut keys = Vec::with_capacity(peritus_app_protocol::MAX_PRODUCT_RUN_PAGE + 1);
        if let Some(cursor) = query.cursor() {
            validate_cursor_member(
                &self.by_sequence,
                cursor.highwater_sequence(),
                cursor.highwater_run(),
            )?;
            validate_cursor_member(&self.by_sequence, cursor.after_sequence(), cursor.after_run())?;
            keys.extend(
                self.by_sequence
                    .range(..cursor.after_sequence())
                    .rev()
                    .take(peritus_app_protocol::MAX_PRODUCT_RUN_PAGE + 1)
                    .map(|(sequence, run)| RunCatalogKey::new(*sequence, *run)),
            );
        } else {
            keys.extend(
                self.by_sequence
                    .range(..=highwater.sequence())
                    .rev()
                    .take(peritus_app_protocol::MAX_PRODUCT_RUN_PAGE + 1)
                    .map(|(sequence, run)| RunCatalogKey::new(*sequence, *run)),
            );
        }
        let has_more = keys.len() > peritus_app_protocol::MAX_PRODUCT_RUN_PAGE;
        keys.truncate(peritus_app_protocol::MAX_PRODUCT_RUN_PAGE);
        let next = if has_more {
            let after = *keys.last().ok_or(ProductRunServiceError::InvalidState)?;
            Some(
                ProductRunPageCursor::new(
                    store,
                    highwater.sequence,
                    highwater.run,
                    after.sequence,
                    after.run,
                )
                .map_err(|_| ProductRunServiceError::InvalidState)?,
            )
        } else {
            None
        };
        Ok(RunCatalogSelection { keys, next })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedCatalogFrontier {
    schema_version: u16,
    last_sequence: u64,
}

fn load_frontier(directory: &std::path::Path, minimum: u64) -> Result<u64, ProductRunServiceError> {
    let path = frontier_path(directory)?;
    let retained = match std::fs::read(&path) {
        Ok(bytes) => {
            let retained: PersistedCatalogFrontier =
                serde_json::from_slice(&bytes).map_err(|error| {
                    ProductRunServiceError::persistence("decode run-catalog frontier", error)
                })?;
            if retained.schema_version != 1 {
                return Err(ProductRunServiceError::InvalidState);
            }
            retained.last_sequence
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => {
            return Err(ProductRunServiceError::persistence("read run-catalog frontier", error));
        }
    };
    let frontier = retained.max(minimum);
    if retained != frontier || !path.exists() {
        persist_frontier(directory, frontier)?;
    }
    Ok(frontier)
}

fn advance_frontier(
    directory: &std::path::Path,
    frontier: u64,
) -> Result<u64, ProductRunServiceError> {
    let next = frontier.checked_add(1).ok_or(ProductRunServiceError::InvalidState)?;
    persist_frontier(directory, next)?;
    Ok(next)
}

fn persist_frontier(
    directory: &std::path::Path,
    frontier: u64,
) -> Result<(), ProductRunServiceError> {
    let path = frontier_path(directory)?;
    let parent = path.parent().ok_or(ProductRunServiceError::InvalidState)?;
    std::fs::create_dir_all(parent).map_err(|error| {
        ProductRunServiceError::persistence("create run-catalog directory", error)
    })?;
    let bytes = serde_json::to_vec_pretty(&PersistedCatalogFrontier {
        schema_version: 1,
        last_sequence: frontier,
    })
    .map_err(|error| ProductRunServiceError::persistence("encode run-catalog frontier", error))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
        ProductRunServiceError::persistence("create run-catalog frontier", error)
    })?;
    temporary.write_all(&bytes).map_err(|error| {
        ProductRunServiceError::persistence("write run-catalog frontier", error)
    })?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| ProductRunServiceError::persistence("sync run-catalog frontier", error))?;
    temporary.persist(&path).map_err(|error| {
        ProductRunServiceError::persistence("replace run-catalog frontier", error.error)
    })?;
    #[cfg(unix)]
    std::fs::File::open(parent).and_then(|directory| directory.sync_all()).map_err(|error| {
        ProductRunServiceError::persistence("sync run-catalog directory", error)
    })?;
    Ok(())
}

fn frontier_path(
    directory: &std::path::Path,
) -> Result<std::path::PathBuf, ProductRunServiceError> {
    let runs = super::super::persistence::record_directory(directory)?;
    Ok(runs.parent().ok_or(ProductRunServiceError::InvalidState)?.join("catalog-frontier.json"))
}

fn validate_cursor_member(
    ordered: &BTreeMap<u64, RunId>,
    sequence: u64,
    run: RunId,
) -> Result<(), ProductRunServiceError> {
    if ordered.get(&sequence).is_some_and(|retained| *retained != run) {
        return Err(ProductRunServiceError::invalid_data(
            "continue product-run history",
            "cursor sequence belongs to another run",
        ));
    }
    Ok(())
}
