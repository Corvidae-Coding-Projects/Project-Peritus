//! A2 replay conformance executed against the production `SQLite` journal and pure replay engine.

use std::{
    future::Future,
    num::NonZeroU64,
    path::{Path, PathBuf},
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    thread,
};

use peritus_codec::{CodecLimits, decode_frame, encode_frame, sha256};
use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, ConformanceRunner, ReplayConformanceError,
    ReplayConformanceSubject, ReplayObservation, ReportText, SubjectDescriptor, SubjectFactory,
    SubjectFailure, SuiteStatus, replay_suite,
};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, EventDraft, ExactFrame,
    HeadExpectation, SqliteJournal, SqliteJournalOptions, StoreId,
};
use peritus_projection::{
    FoldContext, Projection, ProjectionError, ProjectionIdentity, ProjectionName, ProjectionSchema,
    ProjectionState, ProjectionVersion, replay_from_genesis,
};
use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};
use tempfile::TempDir;

struct ProductionReplaySubject {
    _temp: TempDir,
    path: PathBuf,
    journal: Option<SqliteJournal>,
}

impl ProductionReplaySubject {
    fn new() -> Result<Self, ReplayConformanceError> {
        let temp = TempDir::new().map_err(|_| ReplayConformanceError::Infrastructure)?;
        let path = temp.path().join("conformance.sqlite3");
        let journal = open_journal(&path)?;
        Ok(Self { _temp: temp, path, journal: Some(journal) })
    }

    fn journal(&mut self) -> Result<&mut SqliteJournal, ReplayConformanceError> {
        self.journal.as_mut().ok_or(ReplayConformanceError::Infrastructure)
    }
}

impl ReplayConformanceSubject for ProductionReplaySubject {
    fn seed(&mut self, frames: &[Vec<u8>]) -> Result<(), ReplayConformanceError> {
        let aggregate = aggregate();
        let mut events = Vec::with_capacity(frames.len());
        for (index, payload) in frames.iter().enumerate() {
            let sequence = u64::try_from(index)
                .ok()
                .and_then(|value| value.checked_add(1))
                .ok_or(ReplayConformanceError::Infrastructure)?;
            let sequence =
                EventSequence::new(sequence).map_err(|_| ReplayConformanceError::Infrastructure)?;
            let event_id = event(index)?;
            let previous = index.checked_sub(1).map(event).transpose()?;
            let bytes = encode_frame(5, 1, payload, CodecLimits::PRODUCTION)
                .map_err(|_| ReplayConformanceError::Infrastructure)?;
            events.push(
                EventDraft::new(
                    aggregate,
                    sequence,
                    event_id,
                    previous,
                    ExactFrame::new(bytes).map_err(|_| ReplayConformanceError::Infrastructure)?,
                    Sha256Digest::new([0x41; 32]),
                    Vec::new(),
                )
                .map_err(|_| ReplayConformanceError::Infrastructure)?,
            );
        }
        let request = AppendRequest::new(
            store_id(),
            CommandId::new([0x52; 16]).map_err(|_| ReplayConformanceError::Infrastructure)?,
            Sha256Digest::new([0x53; 32]),
            vec![HeadExpectation::Absent(aggregate)],
            events,
            Vec::new(),
            Vec::new(),
            None,
            None,
            Vec::new(),
        )
        .plan()
        .map_err(|_| ReplayConformanceError::Infrastructure)?;
        self.journal().and_then(|journal| {
            journal.append(request).map_err(|_| ReplayConformanceError::Infrastructure)
        })?;
        Ok(())
    }

    fn replay_from_genesis(&mut self) -> Result<ReplayObservation, ReplayConformanceError> {
        let export = self
            .journal()?
            .integrity_export()
            .map_err(|_| ReplayConformanceError::Infrastructure)?;
        let replay = replay_from_genesis(&LengthPrefixedProjection::new()?, &export)
            .map_err(|_| ReplayConformanceError::Infrastructure)?;
        let positions = export
            .records()
            .iter()
            .map(peritus_journal::CommittedRecord::global_position)
            .collect();
        Ok(ReplayObservation::new(positions, replay.payload().to_vec(), 0))
    }

    fn restart(&mut self) -> Result<(), ReplayConformanceError> {
        self.journal = None;
        self.journal = Some(open_journal(&self.path)?);
        Ok(())
    }
}

#[derive(Eq, PartialEq)]
struct LengthPrefixedState(Vec<u8>);

impl ProjectionState for LengthPrefixedState {
    fn encode(&self) -> Vec<u8> {
        self.0.clone()
    }

    fn validate(&self) -> Result<(), ProjectionError> {
        Ok(())
    }

    fn invariant_digest(&self) -> Sha256Digest {
        sha256(&self.0)
    }
}

struct LengthPrefixedProjection {
    schema: ProjectionSchema,
}

impl LengthPrefixedProjection {
    fn new() -> Result<Self, ReplayConformanceError> {
        let name = ProjectionName::new("a2-replay-conformance")
            .map_err(|_| ReplayConformanceError::Infrastructure)?;
        let identity = ProjectionIdentity::new(name, ProjectionVersion::new(NonZeroU64::MIN));
        let schema = ProjectionSchema::new(identity, b"length-prefixed-payload-v1")
            .map_err(|_| ReplayConformanceError::Infrastructure)?;
        Ok(Self { schema })
    }
}

impl Projection for LengthPrefixedProjection {
    type State = LengthPrefixedState;

    fn schema(&self) -> &ProjectionSchema {
        &self.schema
    }

    fn genesis(&self) -> Self::State {
        LengthPrefixedState(Vec::new())
    }

    fn fold(&self, state: &mut Self::State, input: FoldContext<'_>) -> Result<(), ProjectionError> {
        let frame = decode_frame(input.frame_bytes(), CodecLimits::PRODUCTION)
            .expect("replay passes only checked canonical frames to folds");
        state.0.extend_from_slice(&(frame.payload().len() as u64).to_be_bytes());
        state.0.extend_from_slice(frame.payload());
        Ok(())
    }
}

struct Factory {
    descriptor: SubjectDescriptor,
}

impl Factory {
    fn new() -> Self {
        Self {
            descriptor: SubjectDescriptor::new(
                text("peritus-projection"),
                text("SQLite journal plus production replay engine"),
            ),
        }
    }
}

impl SubjectFactory<ProductionReplaySubject> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ProductionReplaySubject, SubjectFailure>> {
        Box::pin(async {
            ProductionReplaySubject::new()
                .map_err(|_| SubjectFailure::new(code("C0-REPLAY-SETUP"), text("setup failed")))
        })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ProductionReplaySubject,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn production_replay_passes_a2_determinism_and_restart_conformance() {
    let report = block_on(ConformanceRunner::run(
        &replay_suite::<ProductionReplaySubject>(),
        &Factory::new(),
    ));
    assert_eq!(report.status(), SuiteStatus::Passed);
    assert_eq!(report.summary().total(), 2);
}

fn open_journal(path: &Path) -> Result<SqliteJournal, ReplayConformanceError> {
    SqliteJournal::open(path, store_id(), SqliteJournalOptions::default())
        .map_err(|_| ReplayConformanceError::Infrastructure)
}

fn store_id() -> StoreId {
    StoreId::new([0x51; 16]).expect("nonzero fixed store ID")
}

fn aggregate() -> AggregateKey {
    AggregateKey::new(
        AggregateKind::Kernel,
        AggregateId::new([0x54; 16]).expect("nonzero fixed aggregate ID"),
    )
}

fn event(index: usize) -> Result<EventId, ReplayConformanceError> {
    let byte = u8::try_from(index)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or(ReplayConformanceError::Infrastructure)?;
    EventId::new([byte; 16]).map_err(|_| ReplayConformanceError::Infrastructure)
}

fn code(value: &str) -> peritus_conformance::FailureCode {
    peritus_conformance::FailureCode::new(value).expect("fixed conformance code")
}

fn text(value: &str) -> ReportText {
    ReportText::new(value).expect("fixed conformance text")
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    struct ThreadWake(thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => thread::park(),
        }
    }
}
