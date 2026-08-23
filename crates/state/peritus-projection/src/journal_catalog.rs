//! Complete aggregate and family catalog projection.

use crate::encoding::{put_digest, put_key, put_u16, put_u64};
use crate::lifecycle::{invariant, schema};
use crate::{FoldContext, Projection, ProjectionError, ProjectionSchema, ProjectionState};
use peritus_codec::sha256;
use peritus_journal::AggregateKey;
use peritus_types::Sha256Digest;
use std::collections::BTreeMap;

/// Latest journal observation for one aggregate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JournalCatalogEntry {
    last_position: u64,
    sequence: u64,
    family: u16,
    event_hash: Sha256Digest,
    revision_digest: Sha256Digest,
}

/// Deterministic complete journal catalog state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct JournalCatalogState {
    aggregates: BTreeMap<AggregateKey, JournalCatalogEntry>,
    family_counts: BTreeMap<u16, u64>,
    event_count: u64,
}

impl JournalCatalogState {
    /// Returns the complete number of folded records.
    #[must_use]
    pub const fn event_count(&self) -> u64 {
        self.event_count
    }

    /// Returns the number of distinct aggregates.
    #[must_use]
    pub fn aggregate_count(&self) -> usize {
        self.aggregates.len()
    }

    /// Returns the count for one registered frame family.
    #[must_use]
    pub fn family_count(&self, family: u16) -> u64 {
        self.family_counts.get(&family).copied().unwrap_or(0)
    }
}

impl ProjectionState for JournalCatalogState {
    fn encode(&self) -> Vec<u8> {
        let mut bytes = b"peritus-journal-catalog-v1\0".to_vec();
        put_u64(&mut bytes, self.event_count);
        put_u64(&mut bytes, self.aggregates.len() as u64);
        for (key, entry) in &self.aggregates {
            put_key(&mut bytes, *key);
            put_u64(&mut bytes, entry.last_position);
            put_u64(&mut bytes, entry.sequence);
            put_u16(&mut bytes, entry.family);
            put_digest(&mut bytes, entry.event_hash);
            put_digest(&mut bytes, entry.revision_digest);
        }
        put_u64(&mut bytes, self.family_counts.len() as u64);
        for (family, count) in &self.family_counts {
            put_u16(&mut bytes, *family);
            put_u64(&mut bytes, *count);
        }
        bytes
    }

    fn validate(&self) -> Result<(), ProjectionError> {
        let counted =
            self.family_counts.values().try_fold(0_u64, |sum, count| sum.checked_add(*count));
        if counted != Some(self.event_count)
            || self
                .aggregates
                .values()
                .any(|entry| entry.last_position == 0 || entry.sequence == 0 || entry.family == 0)
        {
            return Err(invariant("journal catalog counts or entries are invalid"));
        }
        Ok(())
    }

    fn invariant_digest(&self) -> Sha256Digest {
        let mut bytes = b"peritus-journal-catalog-invariants-v1\0".to_vec();
        bytes.extend_from_slice(&self.encode());
        sha256(&bytes)
    }
}

/// Version-one complete journal catalog projection.
#[derive(Clone, Debug)]
pub struct JournalCatalogProjection {
    schema: ProjectionSchema,
}

impl JournalCatalogProjection {
    /// Creates the frozen journal catalog schema.
    ///
    /// # Errors
    ///
    /// Returns an identity error only if built-in constants are invalid.
    pub fn new() -> Result<Self, ProjectionError> {
        schema("journal-catalog", b"all-registered-families:v1;aggregate-heads;family-counts")
            .map(|schema| Self { schema })
    }
}

impl Projection for JournalCatalogProjection {
    type State = JournalCatalogState;

    fn schema(&self) -> &ProjectionSchema {
        &self.schema
    }

    fn genesis(&self) -> Self::State {
        JournalCatalogState::default()
    }

    fn fold(&self, state: &mut Self::State, input: FoldContext<'_>) -> Result<(), ProjectionError> {
        let record = input.record();
        state.event_count = state
            .event_count
            .checked_add(1)
            .ok_or_else(|| invariant("journal catalog event count overflow"))?;
        let count = state.family_counts.entry(input.family()).or_default();
        *count = count.checked_add(1).ok_or_else(|| invariant("journal family count overflow"))?;
        state.aggregates.insert(
            record.aggregate(),
            JournalCatalogEntry {
                last_position: record.global_position(),
                sequence: record.sequence().get(),
                family: input.family(),
                event_hash: record.event_hash(),
                revision_digest: record.revision_digest(),
            },
        );
        Ok(())
    }
}
