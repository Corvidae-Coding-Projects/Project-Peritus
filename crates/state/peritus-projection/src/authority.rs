//! Authority catalog projection over canonical policy and acceptance records.

use crate::encoding::{Decoder, decode_error, put_digest, put_key, put_u16, put_u64};
use crate::lifecycle::{invalid_frame, invariant, schema};
use crate::{
    FoldContext, Projection, ProjectionError, ProjectionErrorKind, ProjectionSchema,
    ProjectionState, RecoveryClass,
};
use peritus_codec::{CodecLimits, decode_message, sha256};
use peritus_journal::{
    AggregateKey, AggregateKind, CREDENTIAL_REGISTRY_EVENT_FAMILY, JournalError,
    JournalErrorKind, decode_credential_registry_event,
};
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
    registry_generation: Option<u64>,
}

impl AuthorityEntry {
    /// Returns the last canonical family applied to this aggregate.
    #[must_use]
    pub const fn family(self) -> u16 {
        self.family
    }

    /// Returns the decoded credential-registry generation for a typed family-94 observation.
    #[must_use]
    pub const fn registry_generation(self) -> Option<u64> {
        self.registry_generation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AuthorityEncoding {
    LegacyV1,
    V2,
}

/// Deterministic authority catalog state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorityState {
    encoding: AuthorityEncoding,
    entries: BTreeMap<AggregateKey, AuthorityEntry>,
}

impl Default for AuthorityState {
    fn default() -> Self {
        Self { encoding: AuthorityEncoding::V2, entries: BTreeMap::new() }
    }
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
        let mut bytes = match self.encoding {
            AuthorityEncoding::LegacyV1 => b"peritus-authority-projection-v1\0".to_vec(),
            AuthorityEncoding::V2 => b"peritus-authority-projection-v2\0".to_vec(),
        };
        put_u64(&mut bytes, self.entries.len() as u64);
        for (key, entry) in &self.entries {
            put_key(&mut bytes, *key);
            put_u64(&mut bytes, entry.last_position);
            put_u64(&mut bytes, entry.sequence);
            put_u16(&mut bytes, entry.family);
            put_digest(&mut bytes, entry.frame_digest);
            put_digest(&mut bytes, entry.revision_digest);
            if self.encoding == AuthorityEncoding::V2 {
                put_u64(&mut bytes, entry.registry_generation.unwrap_or(0));
            }
        }
        bytes
    }

    fn decode(payload: &[u8]) -> Result<Self, ProjectionError> {
        let encoding = if payload.starts_with(b"peritus-authority-projection-v1\0") {
            AuthorityEncoding::LegacyV1
        } else {
            AuthorityEncoding::V2
        };
        let domain: &[u8] = match encoding {
            AuthorityEncoding::LegacyV1 => b"peritus-authority-projection-v1\0",
            AuthorityEncoding::V2 => b"peritus-authority-projection-v2\0",
        };
        let mut decoder = Decoder::new(payload, domain)?;
        let count = decoder.count(if encoding == AuthorityEncoding::V2 { 108 } else { 100 })?;
        let mut entries = BTreeMap::new();
        for _ in 0..count {
            let key = decoder.key()?;
            let last_position = decoder.u64()?;
            let sequence = decoder.u64()?;
            let family = decoder.u16()?;
            let frame_digest = decoder.digest()?;
            let revision_digest = decoder.digest()?;
            let registry_generation = if encoding == AuthorityEncoding::V2 {
                match decoder.u64()? {
                    0 => None,
                    generation => Some(generation),
                }
            } else {
                None
            };
            let entry = AuthorityEntry {
                last_position,
                sequence,
                family,
                frame_digest,
                revision_digest,
                registry_generation,
            };
            entries.insert(key, entry);
        }
        decoder.finish()?;
        let state = Self { encoding, entries };
        state.validate()?;
        if state.encode() != payload {
            return Err(decode_error("authority checkpoint is not canonical"));
        }
        Ok(state)
    }

    fn validate(&self) -> Result<(), ProjectionError> {
        if self.entries.iter().any(|(key, entry)| {
            entry.last_position == 0
                || entry.sequence == 0
                || match self.encoding {
                    AuthorityEncoding::LegacyV1 => {
                        !matches!(
                            key.kind(),
                            AggregateKind::Approval | AggregateKind::CredentialRegistry
                        ) || !matches!(entry.family, 20 | 21 | 23 | 31 | 94)
                            || entry.registry_generation.is_some()
                    }
                    AuthorityEncoding::V2 => match key.kind() {
                        AggregateKind::Approval => {
                            !matches!(entry.family, 20 | 21 | 23 | 31)
                                || entry.registry_generation.is_some()
                        }
                        AggregateKind::CredentialRegistry => {
                            entry.family != CREDENTIAL_REGISTRY_EVENT_FAMILY
                                || entry.registry_generation.is_none()
                        }
                        _ => true,
                    },
                }
        }) {
            return Err(invariant("invalid authority projection entry"));
        }
        Ok(())
    }

    fn invariant_digest(&self) -> Sha256Digest {
        let mut bytes = match self.encoding {
            AuthorityEncoding::LegacyV1 => b"peritus-authority-invariants-v1\0".to_vec(),
            AuthorityEncoding::V2 => b"peritus-authority-invariants-v2\0".to_vec(),
        };
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
        schema(
            "authority",
            b"action-intent:v1;policy:v1;amendment:v1;acceptance:v1;credential-registry-install:v2",
        )
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
        if state.encoding != AuthorityEncoding::V2 {
            return Err(invariant("legacy authority checkpoint requires a genesis rebuild"));
        }
        let record = input.record();
        let registry_generation = match input.family() {
            20 => decode_message::<ActionIntentDto>(input.frame_bytes(), CodecLimits::PRODUCTION)
                .map(|_| ())
                .map_err(|_| invalid_frame("decode action intent"))
                .map(|()| None)?,
            21 => {
                decode_message::<PolicyDefinitionDto>(input.frame_bytes(), CodecLimits::PRODUCTION)
                    .map(|_| ())
                    .map_err(|_| invalid_frame("decode policy definition"))?;
                None
            }
            23 => {
                decode_message::<PolicyAmendmentDto>(input.frame_bytes(), CodecLimits::PRODUCTION)
                    .map(|_| ())
                    .map_err(|_| invalid_frame("decode policy amendment"))?;
                None
            }
            31 => {
                decode_message::<AcceptanceContractDto>(
                    input.frame_bytes(),
                    CodecLimits::PRODUCTION,
                )
                .map(|_| ())
                .map_err(|_| invalid_frame("decode acceptance contract"))?;
                None
            }
            CREDENTIAL_REGISTRY_EVENT_FAMILY => {
                let event = decode_credential_registry_event(record, input.store_id())
                    .map_err(registry_event_error)?;
                let previous = state.entries.get(&record.aggregate());
                if event.expected_revision() != previous.map(|entry| entry.sequence)
                    || previous.is_some_and(|entry| {
                        entry.family != CREDENTIAL_REGISTRY_EVENT_FAMILY
                            || entry
                                .registry_generation
                                .is_none_or(|generation| generation >= event.generation())
                    })
                {
                    return Err(invariant(
                        "credential registry event does not extend its typed authority lineage",
                    ));
                }
                Some(event.generation())
            }
            _ => return Ok(()),
        };
        if registry_generation.is_none() && record.aggregate().kind() != AggregateKind::Approval {
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
                registry_generation,
            },
        );
        Ok(())
    }
}

fn registry_event_error(error: JournalError) -> ProjectionError {
    let kind = match error.kind() {
        JournalErrorKind::UnsupportedSchema => ProjectionErrorKind::UnsupportedSchema,
        JournalErrorKind::CorruptJournal => ProjectionErrorKind::FoldInvariant,
        _ => ProjectionErrorKind::InvalidFrame,
    };
    ProjectionError::new(
        kind,
        RecoveryClass::RepairJournal,
        "decode credential registry event",
        error.to_string(),
    )
}
