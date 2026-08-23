//! Lifecycle projection over canonical kernel-event records.

#![allow(
    clippy::redundant_pub_crate,
    reason = "the private module deliberately shares schema and error helpers with sibling folds"
)]

use crate::encoding::{put_digest, put_key, put_u16, put_u64};
use crate::{
    FoldContext, Projection, ProjectionError, ProjectionErrorKind, ProjectionIdentity,
    ProjectionName, ProjectionSchema, ProjectionState, ProjectionVersion, RecoveryClass,
};
use peritus_codec::{CodecLimits, decode_message, sha256};
use peritus_journal::{AggregateKey, AggregateKind};
use peritus_kernel::KernelEventKind;
use peritus_protocol::KernelEventDto;
use peritus_types::Sha256Digest;
use std::{collections::BTreeMap, num::NonZeroU64};

const FAMILY: u16 = 3;

/// Latest lifecycle observation for one kernel aggregate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LifecycleEntry {
    last_position: u64,
    sequence: u64,
    event_kind: u16,
    frame_digest: Sha256Digest,
    revision_digest: Sha256Digest,
}

impl LifecycleEntry {
    /// Returns the last journal position applied to the aggregate.
    #[must_use]
    pub const fn last_position(self) -> u64 {
        self.last_position
    }

    /// Returns the stable kernel-event discriminant.
    #[must_use]
    pub const fn event_kind(self) -> u16 {
        self.event_kind
    }
}

/// Deterministic lifecycle projection state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LifecycleState {
    entries: BTreeMap<AggregateKey, LifecycleEntry>,
}

impl LifecycleState {
    /// Returns the number of observed lifecycle aggregates.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the projection contains no lifecycle aggregates.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Looks up one aggregate's latest lifecycle observation.
    #[must_use]
    pub fn get(&self, key: AggregateKey) -> Option<LifecycleEntry> {
        self.entries.get(&key).copied()
    }
}

impl ProjectionState for LifecycleState {
    fn encode(&self) -> Vec<u8> {
        let mut bytes = b"peritus-lifecycle-projection-v1\0".to_vec();
        put_u64(&mut bytes, self.entries.len() as u64);
        for (key, entry) in &self.entries {
            put_key(&mut bytes, *key);
            put_u64(&mut bytes, entry.last_position);
            put_u64(&mut bytes, entry.sequence);
            put_u16(&mut bytes, entry.event_kind);
            put_digest(&mut bytes, entry.frame_digest);
            put_digest(&mut bytes, entry.revision_digest);
        }
        bytes
    }

    fn validate(&self) -> Result<(), ProjectionError> {
        if self.entries.iter().any(|(key, entry)| {
            key.kind() != AggregateKind::Kernel
                || entry.last_position == 0
                || entry.sequence == 0
                || entry.event_kind == 0
        }) {
            return Err(invariant("invalid lifecycle entry"));
        }
        Ok(())
    }

    fn invariant_digest(&self) -> Sha256Digest {
        let mut bytes = b"peritus-lifecycle-invariants-v1\0".to_vec();
        bytes.extend_from_slice(&self.encode());
        sha256(&bytes)
    }
}

/// Version-one lifecycle projection.
#[derive(Clone, Debug)]
pub struct LifecycleProjection {
    schema: ProjectionSchema,
}

impl LifecycleProjection {
    /// Creates the frozen version-one lifecycle schema.
    ///
    /// # Errors
    ///
    /// Returns an identity error only if the built-in schema constants are invalid.
    pub fn new() -> Result<Self, ProjectionError> {
        schema("lifecycle", b"kernel-event:v1;latest-kind;exact-revision")
            .map(|schema| Self { schema })
    }
}

impl Projection for LifecycleProjection {
    type State = LifecycleState;

    fn schema(&self) -> &ProjectionSchema {
        &self.schema
    }

    fn genesis(&self) -> Self::State {
        LifecycleState::default()
    }

    fn fold(&self, state: &mut Self::State, input: FoldContext<'_>) -> Result<(), ProjectionError> {
        if input.family() != FAMILY {
            return Ok(());
        }
        let record = input.record();
        if record.aggregate().kind() != AggregateKind::Kernel {
            return Err(invariant("kernel-event frame belongs to a non-kernel aggregate"));
        }
        let event = decode_message::<KernelEventDto>(input.frame_bytes(), CodecLimits::PRODUCTION)
            .map_err(|_| invalid_frame("decode kernel event"))?;
        if event.id != record.event_id()
            || event.command_id != record.command_id()
            || event.sequence != record.sequence()
            || event.previous_event_id != record.previous_event_id()
        {
            return Err(invariant("kernel-event payload disagrees with journal envelope"));
        }
        state.entries.insert(
            record.aggregate(),
            LifecycleEntry {
                last_position: record.global_position(),
                sequence: record.sequence().get(),
                event_kind: event_tag(event.kind),
                frame_digest: record.frame_digest(),
                revision_digest: record.revision_digest(),
            },
        );
        Ok(())
    }
}

pub(super) fn schema(name: &str, descriptor: &[u8]) -> Result<ProjectionSchema, ProjectionError> {
    let name = ProjectionName::new(name)?;
    let version = ProjectionVersion::new(NonZeroU64::MIN);
    ProjectionSchema::new(ProjectionIdentity::new(name, version), descriptor)
}

pub(super) fn invariant(detail: &'static str) -> ProjectionError {
    ProjectionError::new(
        ProjectionErrorKind::FoldInvariant,
        RecoveryClass::RepairJournal,
        "fold projection",
        detail,
    )
}

pub(super) fn invalid_frame(operation: &'static str) -> ProjectionError {
    ProjectionError::new(
        ProjectionErrorKind::InvalidFrame,
        RecoveryClass::RepairJournal,
        operation,
        "typed canonical payload is invalid",
    )
}

const fn event_tag(kind: KernelEventKind) -> u16 {
    use KernelEventKind as K;
    match kind {
        K::SessionOpened => 1,
        K::SessionPaused => 2,
        K::SessionResumed => 3,
        K::SessionClosed => 4,
        K::RunStarted => 5,
        K::RunPaused => 6,
        K::RunResumed => 7,
        K::RunCancelled => 8,
        K::RunFailed => 9,
        K::RunExhausted => 10,
        K::RunRejected => 11,
        K::AttemptStarted => 12,
        K::AttemptResumed => 13,
        K::AttemptSubmitted => 14,
        K::AttemptFailed => 15,
        K::AttemptExhausted => 16,
        K::TurnStarted => 17,
        K::TurnCompleted => 18,
        K::TurnFailed => 19,
        K::TurnCancelled => 20,
        K::ActionProposed => 21,
        K::ActionAuthorized => 22,
        K::ActionDispatched => 23,
        K::ActionCompleted => 24,
        K::ActionFailed => 25,
        K::ActionCancelled => 26,
        K::ReviewRequested => 27,
        K::ReviewBegun => 28,
        K::ReviewSubmitted => 29,
        K::ReviewInvalidated => 30,
        K::WaiverRequested => 31,
        K::WaiverGranted => 32,
        K::WaiverDenied => 33,
        K::WaiverInvalidated => 34,
        K::AcceptanceBegun => 35,
        K::AcceptanceAccepted => 36,
        K::AcceptanceNeedsChanges => 37,
    }
}
