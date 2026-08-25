//! C0 compare-and-append storage for canonical trace observations.

use peritus_codec::{CodecLimits, encode_message, sha256};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, ArtifactDependency, CommandResolution,
    EventDraft, ExactFrame, HeadExpectation, SqliteJournal,
};
use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};

use crate::{Observation, TraceError, TraceErrorKind, TraceProjectionState, trace_schema_digest};

/// Successful durable observation receipt with no execution authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordedObservation {
    event_id: EventId,
    global_position: u64,
    event_hash: Sha256Digest,
    batch_hash: Sha256Digest,
    exact_replay: bool,
}

impl RecordedObservation {
    /// Returns the committed event identity.
    #[must_use]
    pub const fn event_id(self) -> EventId {
        self.event_id
    }
    /// Returns the one-based global C0 position.
    #[must_use]
    pub const fn global_position(self) -> u64 {
        self.global_position
    }
    /// Returns the C0 event-chain hash.
    #[must_use]
    pub const fn event_hash(self) -> Sha256Digest {
        self.event_hash
    }
    /// Returns the atomic C0 batch hash.
    #[must_use]
    pub const fn batch_hash(self) -> Sha256Digest {
        self.batch_hash
    }
    /// Returns whether this call observed an earlier exact command result.
    #[must_use]
    pub const fn exact_replay(self) -> bool {
        self.exact_replay
    }
}

/// Narrow C0-backed trace recorder.
pub struct JournalTraceStore<'journal> {
    journal: &'journal mut SqliteJournal,
}

impl<'journal> JournalTraceStore<'journal> {
    /// Borrows the single-owner C0 journal connection.
    #[must_use]
    pub const fn new(journal: &'journal mut SqliteJournal) -> Self {
        Self { journal }
    }

    /// Validates prior causal state and commits one observation idempotently.
    ///
    /// Exact command replay returns the original position and hashes. Changed command reuse,
    /// missing causal predecessors, stale heads, missing vault artifacts, and indeterminate commits
    /// remain explicit typed failures.
    ///
    /// # Errors
    ///
    /// Returns trace-domain, codec, or C0 storage failures without observation content.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "a record call transfers the immutable observation into the durable append operation"
    )]
    pub fn record(
        &mut self,
        command_id: CommandId,
        observation: Observation,
    ) -> Result<RecordedObservation, TraceError> {
        let frame_bytes = encode_message(&observation, CodecLimits::PRODUCTION)
            .map_err(|_| TraceError::codec("encode trace observation"))?;
        let artifact_dependencies = artifact_dependencies(&observation);
        let request_digest = request_digest(&frame_bytes, &artifact_dependencies);
        match self
            .journal
            .resolve_command(command_id, request_digest)
            .map_err(|error| TraceError::journal("resolve trace command", &error))?
        {
            CommandResolution::Committed(batch) => {
                return receipt(&batch, &observation, true);
            }
            CommandResolution::Conflict { .. } => {
                return Err(TraceError::static_error(
                    TraceErrorKind::DuplicateConflict,
                    "record trace observation",
                    "command identity is bound to another observation",
                ));
            }
            CommandResolution::DefinitelyAbsent => {}
        }

        let key = trace_key(observation.trace_id())?;
        let head = self
            .journal
            .head(key)
            .map_err(|error| TraceError::journal("read trace head", &error))?;
        let records = self
            .journal
            .records_for_aggregate(key)
            .map_err(|error| TraceError::journal("read trace history", &error))?;
        let mut projection = TraceProjectionState::default();
        for record in &records {
            projection.apply_record(record)?;
        }
        let prospective_position = projection
            .last_journal_position()
            .checked_add(1)
            .ok_or_else(|| sequence("trace validation position overflow"))?;
        projection.apply(observation.clone(), prospective_position)?;

        let sequence = match head {
            Some(value) => value
                .sequence()
                .checked_next()
                .map_err(|_| sequence("trace aggregate sequence overflow"))?,
            None => EventSequence::first(),
        };
        let draft = EventDraft::new(
            key,
            sequence,
            observation.event_id(),
            head.map(peritus_journal::AggregateHead::event_id),
            ExactFrame::new(frame_bytes)
                .map_err(|error| TraceError::journal("validate trace frame", &error))?,
            trace_schema_digest(),
            observation.causal_events().to_vec(),
        )
        .map_err(|error| TraceError::journal("plan trace event", &error))?;
        let expectation = head.map_or(HeadExpectation::Absent(key), HeadExpectation::Present);
        let plan = AppendRequest::new(
            self.journal.store_id(),
            command_id,
            request_digest,
            vec![expectation],
            vec![draft],
            Vec::new(),
            artifact_dependencies,
            None,
            None,
            Vec::new(),
        )
        .plan()
        .map_err(|error| TraceError::journal("plan trace append", &error))?;
        let batch = self
            .journal
            .append(plan)
            .map_err(|error| TraceError::journal("commit trace observation", &error))?;
        receipt(&batch, &observation, false)
    }
}

fn artifact_dependencies(observation: &Observation) -> Vec<ArtifactDependency> {
    let mut digests = observation
        .vault_references()
        .into_iter()
        .map(|reference| reference.digest().sha256())
        .collect::<Vec<_>>();
    digests.sort_unstable();
    digests.dedup();
    digests.into_iter().map(ArtifactDependency::new).collect()
}

fn request_digest(frame: &[u8], artifacts: &[ArtifactDependency]) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(48 + frame.len() + artifacts.len() * 32);
    bytes.extend_from_slice(b"PERITUS-C7-TRACE-APPEND-V1\0");
    bytes.extend_from_slice(&(frame.len() as u64).to_be_bytes());
    bytes.extend_from_slice(frame);
    bytes.extend_from_slice(&(artifacts.len() as u64).to_be_bytes());
    for artifact in artifacts {
        bytes.extend_from_slice(artifact.digest().as_bytes());
    }
    sha256(&bytes)
}

fn trace_key(trace_id: crate::TraceId) -> Result<AggregateKey, TraceError> {
    let id = AggregateId::new(trace_id.into_bytes())
        .map_err(|error| TraceError::journal("derive trace aggregate", &error))?;
    Ok(AggregateKey::new(AggregateKind::Trace, id))
}

fn receipt(
    batch: &peritus_journal::CommittedBatch,
    expected: &Observation,
    exact_replay: bool,
) -> Result<RecordedObservation, TraceError> {
    let [record] = batch.records() else {
        return Err(integrity("trace command did not commit exactly one event"));
    };
    if record.event_id() != expected.event_id()
        || record.aggregate().kind() != AggregateKind::Trace
        || record.aggregate().id().as_bytes() != expected.trace_id().as_bytes()
    {
        return Err(integrity("committed trace receipt disagrees with the observation"));
    }
    Ok(RecordedObservation {
        event_id: record.event_id(),
        global_position: record.global_position(),
        event_hash: record.event_hash(),
        batch_hash: batch.batch_hash(),
        exact_replay,
    })
}

const fn sequence(detail: &'static str) -> TraceError {
    TraceError::static_error(TraceErrorKind::Sequence, "record trace observation", detail)
}

const fn integrity(detail: &'static str) -> TraceError {
    TraceError::static_error(TraceErrorKind::Integrity, "observe trace commit", detail)
}
