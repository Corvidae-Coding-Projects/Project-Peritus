//! Evidence digest catalog projection with no effect on unrelated records.

use crate::encoding::{put_digest, put_u64};
use crate::lifecycle::{invalid_frame, invariant, schema};
use crate::{FoldContext, Projection, ProjectionError, ProjectionSchema, ProjectionState};
use peritus_codec::{CodecLimits, decode_message, sha256};
use peritus_journal::AggregateKind;
use peritus_protocol::{BudgetReceiptDto, ReservationSnapshotDto};
use peritus_types::Sha256Digest;
use std::collections::BTreeMap;

/// Usage summary for one evidence digest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvidenceEntry {
    first_position: u64,
    last_position: u64,
    reference_count: u64,
}

/// Deterministic evidence catalog state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EvidenceCatalogState {
    entries: BTreeMap<Sha256Digest, EvidenceEntry>,
}

impl EvidenceCatalogState {
    /// Returns the number of distinct referenced evidence digests.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether no evidence digest has been referenced.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl ProjectionState for EvidenceCatalogState {
    fn encode(&self) -> Vec<u8> {
        let mut bytes = b"peritus-evidence-catalog-v1\0".to_vec();
        put_u64(&mut bytes, self.entries.len() as u64);
        for (digest, entry) in &self.entries {
            put_digest(&mut bytes, *digest);
            put_u64(&mut bytes, entry.first_position);
            put_u64(&mut bytes, entry.last_position);
            put_u64(&mut bytes, entry.reference_count);
        }
        bytes
    }

    fn validate(&self) -> Result<(), ProjectionError> {
        if self.entries.values().any(|entry| {
            entry.first_position == 0
                || entry.last_position < entry.first_position
                || entry.reference_count == 0
        }) {
            return Err(invariant("invalid evidence catalog range"));
        }
        Ok(())
    }

    fn invariant_digest(&self) -> Sha256Digest {
        let mut bytes = b"peritus-evidence-catalog-invariants-v1\0".to_vec();
        bytes.extend_from_slice(&self.encode());
        sha256(&bytes)
    }
}

/// Version-one evidence catalog projection.
#[derive(Clone, Debug)]
pub struct EvidenceCatalogProjection {
    schema: ProjectionSchema,
}

impl EvidenceCatalogProjection {
    /// Creates the frozen evidence catalog schema.
    ///
    /// # Errors
    ///
    /// Returns an identity error only if built-in constants are invalid.
    pub fn new() -> Result<Self, ProjectionError> {
        schema("evidence-catalog", b"budget-evidence-digests:v1;first-last-count")
            .map(|schema| Self { schema })
    }
}

impl Projection for EvidenceCatalogProjection {
    type State = EvidenceCatalogState;

    fn schema(&self) -> &ProjectionSchema {
        &self.schema
    }

    fn genesis(&self) -> Self::State {
        EvidenceCatalogState::default()
    }

    fn fold(&self, state: &mut Self::State, input: FoldContext<'_>) -> Result<(), ProjectionError> {
        let digests: Vec<_> = match input.family() {
            13 => {
                let snapshot = decode_message::<ReservationSnapshotDto>(
                    input.frame_bytes(),
                    CodecLimits::PRODUCTION,
                )
                .map_err(|_| invalid_frame("decode evidence reservation snapshot"))?;
                [
                    snapshot.activation_evidence,
                    snapshot.observation_evidence,
                    snapshot.final_evidence,
                ]
                .into_iter()
                .flatten()
                .collect()
            }
            14 => decode_message::<BudgetReceiptDto>(input.frame_bytes(), CodecLimits::PRODUCTION)
                .map_err(|_| invalid_frame("decode evidence budget receipt"))?
                .evidence_digest
                .into_iter()
                .collect(),
            _ => return Ok(()),
        };
        if input.record().aggregate().kind() != AggregateKind::Budget {
            return Err(invariant("budget evidence frame belongs to a non-budget aggregate"));
        }
        let position = input.record().global_position();
        for digest in digests {
            let entry = state.entries.entry(digest).or_insert(EvidenceEntry {
                first_position: position,
                last_position: position,
                reference_count: 0,
            });
            entry.last_position = position;
            entry.reference_count = entry
                .reference_count
                .checked_add(1)
                .ok_or_else(|| invariant("evidence reference count overflow"))?;
        }
        Ok(())
    }
}
