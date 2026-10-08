//! Integrity-bound durable suffix-rebuild progress.

use super::ProjectionStore;
use super::contention;
use super::store::{digest, u64_to_i64};
use crate::{CatalogGeneration, ProjectionError, ProjectionSchema};
use peritus_codec::sha256;
use peritus_journal::StoreId;
use peritus_types::Sha256Digest;
use rusqlite::{OptionalExtension, params};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkPhase {
    Folding,
    Ready,
}

impl WorkPhase {
    const fn tag(self) -> i64 {
        match self {
            Self::Folding => 1,
            Self::Ready => 2,
        }
    }

    const fn from_tag(tag: i64) -> Option<Self> {
        match tag {
            1 => Some(Self::Folding),
            2 => Some(Self::Ready),
            _ => None,
        }
    }
}

pub(crate) struct StoredProgress {
    pub(crate) schema_digest: Sha256Digest,
    pub(crate) owner_store_id: StoreId,
    pub(crate) source_generation: Option<CatalogGeneration>,
    pub(crate) phase: WorkPhase,
    pub(crate) cursor_position: u64,
    pub(crate) journal_head_digest: Option<Sha256Digest>,
    pub(crate) payload_digest: Sha256Digest,
    pub(crate) invariant_digest: Sha256Digest,
    pub(crate) frontier_digest: Sha256Digest,
    pub(crate) record_count: u64,
    pub(crate) payload: Vec<u8>,
    pub(crate) frontier: Vec<u8>,
}

impl ProjectionStore {
    pub(crate) fn load_progress(
        &self,
        schema: &ProjectionSchema,
    ) -> Result<Option<StoredProgress>, ProjectionError> {
        contention::run(self.cancellation.as_ref(), || {
            let identity = schema.identity();
            let raw = self
                .connection
                .query_row(
                    "SELECT schema_digest, owner_store_id, source_generation, phase, cursor_position, journal_head_digest, payload_digest, invariant_digest, frontier_digest, work_digest, record_count, payload, frontier FROM peritus_projection_work WHERE projection_name = ?1 AND projection_version = ?2",
                    params![
                        identity.name().as_str(),
                        u64_to_i64(identity.version().get(), "projection version")?,
                    ],
                    |row| {
                        Ok(RawProgress {
                            schema_digest: row.get(0)?,
                            owner_store_id: row.get(1)?,
                            source_generation: row.get(2)?,
                            phase: row.get(3)?,
                            cursor_position: row.get(4)?,
                            journal_head_digest: row.get(5)?,
                            payload_digest: row.get(6)?,
                            invariant_digest: row.get(7)?,
                            frontier_digest: row.get(8)?,
                            work_digest: row.get(9)?,
                            record_count: row.get(10)?,
                            payload: row.get(11)?,
                            frontier: row.get(12)?,
                        })
                    },
                )
                .optional()
                .map_err(|error| ProjectionError::sqlite("load projection rebuild progress", error))?;
            Ok(raw.and_then(|raw| parse_progress(schema, raw)))
        })
    }

    pub(crate) fn store_progress(
        &self,
        schema: &ProjectionSchema,
        progress: &StoredProgress,
    ) -> Result<(), ProjectionError> {
        let identity = schema.identity();
        let version = u64_to_i64(identity.version().get(), "projection version")?;
        let source_generation = progress
            .source_generation
            .map(CatalogGeneration::get)
            .map(|value| u64_to_i64(value, "source generation"))
            .transpose()?;
        let cursor = u64_to_i64(progress.cursor_position, "progress cursor")?;
        let record_count = u64_to_i64(progress.record_count, "progress record count")?;
        let journal_head =
            progress.journal_head_digest.map(|digest| digest.as_bytes().to_vec());
        let work_digest = work_digest(identity.name().as_str(), identity.version().get(), progress);
        contention::run(self.cancellation.as_ref(), || {
            self.connection
                .execute(
                    "INSERT INTO peritus_projection_work(projection_name, projection_version, schema_digest, owner_store_id, source_generation, phase, cursor_position, journal_head_digest, payload_digest, invariant_digest, frontier_digest, work_digest, record_count, payload, frontier) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15) ON CONFLICT(projection_name, projection_version) DO UPDATE SET schema_digest = excluded.schema_digest, owner_store_id = excluded.owner_store_id, source_generation = excluded.source_generation, phase = excluded.phase, cursor_position = excluded.cursor_position, journal_head_digest = excluded.journal_head_digest, payload_digest = excluded.payload_digest, invariant_digest = excluded.invariant_digest, frontier_digest = excluded.frontier_digest, work_digest = excluded.work_digest, record_count = excluded.record_count, payload = excluded.payload, frontier = excluded.frontier",
                    params![
                        identity.name().as_str(),
                        version,
                        progress.schema_digest.as_bytes().as_slice(),
                        progress.owner_store_id.as_bytes().as_slice(),
                        source_generation,
                        progress.phase.tag(),
                        cursor,
                        journal_head,
                        progress.payload_digest.as_bytes().as_slice(),
                        progress.invariant_digest.as_bytes().as_slice(),
                        progress.frontier_digest.as_bytes().as_slice(),
                        work_digest.as_bytes().as_slice(),
                        record_count,
                        &progress.payload,
                        &progress.frontier,
                    ],
                )
                .map(|_| ())
                .map_err(|error| ProjectionError::sqlite("store projection rebuild progress", error))
        })
    }

    pub(crate) fn clear_progress(
        &self,
        schema: &ProjectionSchema,
    ) -> Result<(), ProjectionError> {
        let identity = schema.identity();
        contention::run(self.cancellation.as_ref(), || {
            self.connection
                .execute(
                    "DELETE FROM peritus_projection_work WHERE projection_name = ?1 AND projection_version = ?2",
                    params![
                        identity.name().as_str(),
                        u64_to_i64(identity.version().get(), "projection version")?,
                    ],
                )
                .map(|_| ())
                .map_err(|error| ProjectionError::sqlite("clear projection rebuild progress", error))
        })
    }
}

struct RawProgress {
    schema_digest: Vec<u8>,
    owner_store_id: Vec<u8>,
    source_generation: Option<i64>,
    phase: i64,
    cursor_position: i64,
    journal_head_digest: Option<Vec<u8>>,
    payload_digest: Vec<u8>,
    invariant_digest: Vec<u8>,
    frontier_digest: Vec<u8>,
    work_digest: Vec<u8>,
    record_count: i64,
    payload: Vec<u8>,
    frontier: Vec<u8>,
}

fn parse_progress(schema: &ProjectionSchema, raw: RawProgress) -> Option<StoredProgress> {
    let schema_digest = digest(&raw.schema_digest, "progress schema digest").ok()?;
    let owner_store_id = StoreId::new(raw.owner_store_id.try_into().ok()?).ok()?;
    let source_generation = match raw.source_generation {
        Some(value) => Some(CatalogGeneration::from_u64(u64::try_from(value).ok()?).ok()?),
        None => None,
    };
    let phase = WorkPhase::from_tag(raw.phase)?;
    let cursor_position = u64::try_from(raw.cursor_position).ok()?;
    let record_count = u64::try_from(raw.record_count).ok()?;
    let journal_head_digest = raw
        .journal_head_digest
        .as_deref()
        .map(|value| digest(value, "progress journal head"))
        .transpose()
        .ok()?;
    if matches!(phase, WorkPhase::Folding) != journal_head_digest.is_none()
        || record_count != cursor_position
    {
        return None;
    }
    let payload_digest = digest(&raw.payload_digest, "progress payload digest").ok()?;
    let invariant_digest = digest(&raw.invariant_digest, "progress invariant digest").ok()?;
    let frontier_digest = digest(&raw.frontier_digest, "progress frontier digest").ok()?;
    let stored_work_digest = digest(&raw.work_digest, "progress work digest").ok()?;
    if sha256(&raw.payload) != payload_digest || sha256(&raw.frontier) != frontier_digest {
        return None;
    }
    let progress = StoredProgress {
        schema_digest,
        owner_store_id,
        source_generation,
        phase,
        cursor_position,
        journal_head_digest,
        payload_digest,
        invariant_digest,
        frontier_digest,
        record_count,
        payload: raw.payload,
        frontier: raw.frontier,
    };
    let identity = schema.identity();
    if work_digest(identity.name().as_str(), identity.version().get(), &progress)
        != stored_work_digest
    {
        return None;
    }
    Some(progress)
}

fn work_digest(name: &str, version: u64, progress: &StoredProgress) -> Sha256Digest {
    let mut bytes = b"peritus-projection-work-v1\0".to_vec();
    bytes.extend_from_slice(&(name.len() as u64).to_be_bytes());
    bytes.extend_from_slice(name.as_bytes());
    bytes.extend_from_slice(&version.to_be_bytes());
    bytes.extend_from_slice(progress.schema_digest.as_bytes());
    bytes.extend_from_slice(progress.owner_store_id.as_bytes());
    match progress.source_generation {
        Some(generation) => {
            bytes.push(1);
            bytes.extend_from_slice(&generation.get().to_be_bytes());
        }
        None => bytes.push(0),
    }
    bytes.extend_from_slice(&progress.phase.tag().to_be_bytes());
    bytes.extend_from_slice(&progress.cursor_position.to_be_bytes());
    match progress.journal_head_digest {
        Some(digest) => {
            bytes.push(1);
            bytes.extend_from_slice(digest.as_bytes());
        }
        None => bytes.push(0),
    }
    bytes.extend_from_slice(progress.payload_digest.as_bytes());
    bytes.extend_from_slice(progress.invariant_digest.as_bytes());
    bytes.extend_from_slice(progress.frontier_digest.as_bytes());
    bytes.extend_from_slice(&progress.record_count.to_be_bytes());
    sha256(&bytes)
}
