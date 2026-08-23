//! Authority catalog projection over canonical policy and acceptance records.

use crate::encoding::{put_digest, put_key, put_u16, put_u64};
use crate::lifecycle::{invalid_frame, invariant, schema};
use crate::{FoldContext, Projection, ProjectionError, ProjectionSchema, ProjectionState};
use peritus_codec::{CodecLimits, decode_message, sha256};
use peritus_journal::{AggregateKey, AggregateKind};
use peritus_protocol::{
    AcceptanceContractDto, ActionIntentDto, PolicyAmendmentDto, PolicyDefinitionDto,
};
use peritus_types::Sha256Digest;
use std::collections::BTreeMap;

/// Latest immutable authority observation for one aggregate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorityEntry {
    last_position: u64,
    sequence: u64,
    family: u16,
    frame_digest: Sha256Digest,
    revision_digest: Sha256Digest,
}

impl AuthorityEntry {
    /// Returns the last canonical family applied to this aggregate.
    #[must_use]
    pub const fn family(self) -> u16 {
        self.family
    }
}

/// Deterministic authority catalog state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AuthorityState {
    entries: BTreeMap<AggregateKey, AuthorityEntry>,
}

impl AuthorityState {
    /// Returns the number of cataloged authority aggregates.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether no authority aggregates were observed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl ProjectionState for AuthorityState {
    fn encode(&self) -> Vec<u8> {
        let mut bytes = b"peritus-authority-projection-v1\0".to_vec();
        put_u64(&mut bytes, self.entries.len() as u64);
        for (key, entry) in &self.entries {
            put_key(&mut bytes, *key);
            put_u64(&mut bytes, entry.last_position);
            put_u64(&mut bytes, entry.sequence);
            put_u16(&mut bytes, entry.family);
            put_digest(&mut bytes, entry.frame_digest);
            put_digest(&mut bytes, entry.revision_digest);
        }
        bytes
    }

    fn validate(&self) -> Result<(), ProjectionError> {
        if self.entries.iter().any(|(key, entry)| {
            !matches!(key.kind(), AggregateKind::Approval | AggregateKind::CredentialRegistry)
                || entry.last_position == 0
                || entry.sequence == 0
                || !matches!(entry.family, 20 | 21 | 23 | 31)
        }) {
            return Err(invariant("invalid authority projection entry"));
        }
        Ok(())
    }

    fn invariant_digest(&self) -> Sha256Digest {
        let mut bytes = b"peritus-authority-invariants-v1\0".to_vec();
        bytes.extend_from_slice(&self.encode());
        sha256(&bytes)
    }
}

/// Version-one authority catalog projection.
#[derive(Clone, Debug)]
pub struct AuthorityProjection {
    schema: ProjectionSchema,
}

impl AuthorityProjection {
    /// Creates the frozen version-one authority schema.
    ///
    /// # Errors
    ///
    /// Returns an identity error only if built-in constants are invalid.
    pub fn new() -> Result<Self, ProjectionError> {
        schema("authority", b"action-intent:v1;policy:v1;amendment:v1;acceptance:v1")
            .map(|schema| Self { schema })
    }
}

impl Projection for AuthorityProjection {
    type State = AuthorityState;

    fn schema(&self) -> &ProjectionSchema {
        &self.schema
    }

    fn genesis(&self) -> Self::State {
        AuthorityState::default()
    }

    fn fold(&self, state: &mut Self::State, input: FoldContext<'_>) -> Result<(), ProjectionError> {
        match input.family() {
            20 => decode_message::<ActionIntentDto>(input.frame_bytes(), CodecLimits::PRODUCTION)
                .map(|_| ())
                .map_err(|_| invalid_frame("decode action intent"))?,
            21 => {
                decode_message::<PolicyDefinitionDto>(input.frame_bytes(), CodecLimits::PRODUCTION)
                    .map(|_| ())
                    .map_err(|_| invalid_frame("decode policy definition"))?;
            }
            23 => {
                decode_message::<PolicyAmendmentDto>(input.frame_bytes(), CodecLimits::PRODUCTION)
                    .map(|_| ())
                    .map_err(|_| invalid_frame("decode policy amendment"))?;
            }
            31 => {
                decode_message::<AcceptanceContractDto>(
                    input.frame_bytes(),
                    CodecLimits::PRODUCTION,
                )
                .map(|_| ())
                .map_err(|_| invalid_frame("decode acceptance contract"))?;
            }
            _ => return Ok(()),
        }
        let record = input.record();
        if !matches!(
            record.aggregate().kind(),
            AggregateKind::Approval | AggregateKind::CredentialRegistry
        ) {
            return Err(invariant("authority frame belongs to an unrelated aggregate"));
        }
        state.entries.insert(
            record.aggregate(),
            AuthorityEntry {
                last_position: record.global_position(),
                sequence: record.sequence().get(),
                family: input.family(),
                frame_digest: record.frame_digest(),
                revision_digest: record.revision_digest(),
            },
        );
        Ok(())
    }
}
