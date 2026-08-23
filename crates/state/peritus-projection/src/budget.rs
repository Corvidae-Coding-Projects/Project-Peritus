//! Budget projection over immutable snapshot and receipt families.

use crate::encoding::{put_digest, put_key, put_u16, put_u64};
use crate::lifecycle::{invalid_frame, invariant, schema};
use crate::{FoldContext, Projection, ProjectionError, ProjectionSchema, ProjectionState};
use peritus_codec::{CodecLimits, decode_message, sha256};
use peritus_journal::{AggregateKey, AggregateKind};
use peritus_protocol::{BudgetReceiptDto, BudgetSnapshotDto, ReservationSnapshotDto};
use peritus_types::Sha256Digest;
use std::collections::{BTreeMap, BTreeSet};

/// Latest exact observation for one budget aggregate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BudgetEntry {
    last_position: u64,
    sequence: u64,
    frame_family: u16,
    frame_digest: Sha256Digest,
    revision_digest: Sha256Digest,
}

/// Deterministic budget projection state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BudgetState {
    entries: BTreeMap<AggregateKey, BudgetEntry>,
    evidence: BTreeSet<Sha256Digest>,
}

impl BudgetState {
    /// Returns the number of observed budget aggregates.
    #[must_use]
    pub fn account_count(&self) -> usize {
        self.entries.len()
    }

    /// Returns the number of distinct evidence digests referenced by budget records.
    #[must_use]
    pub fn evidence_count(&self) -> usize {
        self.evidence.len()
    }
}

impl ProjectionState for BudgetState {
    fn encode(&self) -> Vec<u8> {
        let mut bytes = b"peritus-budget-projection-v1\0".to_vec();
        put_u64(&mut bytes, self.entries.len() as u64);
        for (key, entry) in &self.entries {
            put_key(&mut bytes, *key);
            put_u64(&mut bytes, entry.last_position);
            put_u64(&mut bytes, entry.sequence);
            put_u16(&mut bytes, entry.frame_family);
            put_digest(&mut bytes, entry.frame_digest);
            put_digest(&mut bytes, entry.revision_digest);
        }
        put_u64(&mut bytes, self.evidence.len() as u64);
        for digest in &self.evidence {
            put_digest(&mut bytes, *digest);
        }
        bytes
    }

    fn validate(&self) -> Result<(), ProjectionError> {
        if self.entries.iter().any(|(key, entry)| {
            key.kind() != AggregateKind::Budget
                || entry.last_position == 0
                || entry.sequence == 0
                || !matches!(entry.frame_family, 12..=14)
        }) {
            return Err(invariant("invalid budget projection entry"));
        }
        Ok(())
    }

    fn invariant_digest(&self) -> Sha256Digest {
        let mut bytes = b"peritus-budget-invariants-v1\0".to_vec();
        bytes.extend_from_slice(&self.encode());
        sha256(&bytes)
    }
}

/// Version-one budget projection.
#[derive(Clone, Debug)]
pub struct BudgetProjection {
    schema: ProjectionSchema,
}

impl BudgetProjection {
    /// Creates the frozen version-one budget schema.
    ///
    /// # Errors
    ///
    /// Returns an identity error only if built-in schema constants are invalid.
    pub fn new() -> Result<Self, ProjectionError> {
        schema("budget", b"budget-snapshot:v1;reservation:v1;receipt:v1;evidence")
            .map(|schema| Self { schema })
    }
}

impl Projection for BudgetProjection {
    type State = BudgetState;

    fn schema(&self) -> &ProjectionSchema {
        &self.schema
    }

    fn genesis(&self) -> Self::State {
        BudgetState::default()
    }

    fn fold(&self, state: &mut Self::State, input: FoldContext<'_>) -> Result<(), ProjectionError> {
        let evidence = match input.family() {
            12 => {
                let snapshot = decode_message::<BudgetSnapshotDto>(
                    input.frame_bytes(),
                    CodecLimits::PRODUCTION,
                )
                .map_err(|_| invalid_frame("decode budget snapshot"))?;
                if snapshot.id.as_bytes() != input.record().aggregate().id().as_bytes() {
                    return Err(invariant("budget snapshot identity disagrees with aggregate"));
                }
                Vec::new()
            }
            13 => {
                let snapshot = decode_message::<ReservationSnapshotDto>(
                    input.frame_bytes(),
                    CodecLimits::PRODUCTION,
                )
                .map_err(|_| invalid_frame("decode reservation snapshot"))?;
                [
                    snapshot.activation_evidence,
                    snapshot.observation_evidence,
                    snapshot.final_evidence,
                ]
                .into_iter()
                .flatten()
                .collect()
            }
            14 => {
                let receipt = decode_message::<BudgetReceiptDto>(
                    input.frame_bytes(),
                    CodecLimits::PRODUCTION,
                )
                .map_err(|_| invalid_frame("decode budget receipt"))?;
                receipt.evidence_digest.into_iter().collect()
            }
            _ => return Ok(()),
        };
        let record = input.record();
        if record.aggregate().kind() != AggregateKind::Budget {
            return Err(invariant("budget frame belongs to a non-budget aggregate"));
        }
        state.evidence.extend(evidence);
        state.entries.insert(
            record.aggregate(),
            BudgetEntry {
                last_position: record.global_position(),
                sequence: record.sequence().get(),
                frame_family: input.family(),
                frame_digest: record.frame_digest(),
                revision_digest: record.revision_digest(),
            },
        );
        Ok(())
    }
}
