//! A2 journal conformance executed against the production `SQLite` implementation.

use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    thread,
};

use peritus_codec::sha256;
use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, ConformanceRunner, JournalAppendDisposition,
    JournalAppendFixture, JournalAppendObservation, JournalConformanceError,
    JournalConformanceSubject, JournalSnapshot, ReportText, SubjectDescriptor, SubjectFactory,
    SubjectFailure, SuiteStatus, journal_suite,
};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, CommandResolution, EventDraft,
    ExactFrame, HeadExpectation, JournalErrorKind, SqliteJournal, SqliteJournalOptions, StoreId,
};
use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};
use tempfile::TempDir;

struct ProductionJournalSubject {
    _temp: TempDir,
    path: PathBuf,
    journal: Option<SqliteJournal>,
}

impl ProductionJournalSubject {
    fn new() -> Result<Self, JournalConformanceError> {
        let temp = TempDir::new().map_err(|_| JournalConformanceError::Infrastructure)?;
        let path = temp.path().join("conformance.sqlite3");
        let journal = open_journal(&path)?;
        Ok(Self { _temp: temp, path, journal: Some(journal) })
    }

    fn journal(&self) -> Result<&SqliteJournal, JournalConformanceError> {
        self.journal.as_ref().ok_or(JournalConformanceError::Infrastructure)
    }

    fn journal_mut(&mut self) -> Result<&mut SqliteJournal, JournalConformanceError> {
        self.journal.as_mut().ok_or(JournalConformanceError::Infrastructure)
    }
}

impl JournalConformanceSubject for ProductionJournalSubject {
    fn append_absent(
        &mut self,
        fixture: &JournalAppendFixture,
    ) -> Result<JournalAppendObservation, JournalConformanceError> {
        let command = CommandId::new(fixture.command_id())
            .map_err(|_| JournalConformanceError::Infrastructure)?;
        let event = EventId::new(fixture.event_id())
            .map_err(|_| JournalConformanceError::Infrastructure)?;
        let request_digest = sha256(fixture.frame());
        let existed = matches!(
            self.journal().and_then(|journal| {
                journal
                    .resolve_command(command, request_digest)
                    .map_err(|_| JournalConformanceError::Infrastructure)
            })?,
            CommandResolution::Committed(_)
        );
        let aggregate = aggregate();
        let draft = EventDraft::new(
            aggregate,
            EventSequence::first(),
            event,
            None,
            ExactFrame::new(fixture.frame().to_vec())
                .map_err(|_| JournalConformanceError::Infrastructure)?,
            Sha256Digest::new([0x61; 32]),
            Vec::new(),
        )
        .map_err(|_| JournalConformanceError::Infrastructure)?;
        let plan = AppendRequest::new(
            store_id(),
            command,
            request_digest,
            vec![HeadExpectation::Absent(aggregate)],
            vec![draft],
            Vec::new(),
            Vec::new(),
            None,
            None,
            Vec::new(),
        )
        .plan()
        .map_err(|_| JournalConformanceError::Infrastructure)?;
        match self.journal_mut()?.append(plan) {
            Ok(_) => Ok(JournalAppendObservation::new(
                if existed {
                    JournalAppendDisposition::Idempotent
                } else {
                    JournalAppendDisposition::Committed
                },
                1,
            )),
            Err(error) if error.kind() == JournalErrorKind::StaleHead => {
                Err(JournalConformanceError::StaleCas)
            }
            Err(_) => Err(JournalConformanceError::Infrastructure),
        }
    }

    fn restart(&mut self) -> Result<(), JournalConformanceError> {
        self.journal = None;
        self.journal = Some(open_journal(&self.path)?);
        Ok(())
    }

    fn snapshot(&self) -> Result<JournalSnapshot, JournalConformanceError> {
        let records = self
            .journal()?
            .records_for_aggregate(aggregate())
            .map_err(|_| JournalConformanceError::Infrastructure)?;
        let head = self
            .journal()?
            .head(aggregate())
            .map_err(|_| JournalConformanceError::Infrastructure)?
            .ok_or(JournalConformanceError::Infrastructure)?;
        let count =
            u64::try_from(records.len()).map_err(|_| JournalConformanceError::Infrastructure)?;
        let frame = records
            .first()
            .map(|record| record.frame_bytes().to_vec())
            .ok_or(JournalConformanceError::Infrastructure)?;
        Ok(JournalSnapshot::new(count, head.sequence().get(), frame))
    }
}

struct Factory {
    descriptor: SubjectDescriptor,
}

impl Factory {
    fn new() -> Self {
        Self {
            descriptor: SubjectDescriptor::new(
                text("peritus-journal"),
                text("production SQLite journal"),
            ),
        }
    }
}

impl SubjectFactory<ProductionJournalSubject> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ProductionJournalSubject, SubjectFailure>> {
        Box::pin(async {
            ProductionJournalSubject::new().map_err(|_| {
                SubjectFailure::new(code("C0-JOURNAL-SETUP"), text("journal setup failed"))
            })
        })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ProductionJournalSubject,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn production_journal_passes_a2_commit_restart_and_cas_conformance() {
    let report = block_on(ConformanceRunner::run(
        &journal_suite::<ProductionJournalSubject>(),
        &Factory::new(),
    ));
    assert_eq!(report.status(), SuiteStatus::Passed);
    assert_eq!(report.summary().total(), 3);
}

fn open_journal(path: &Path) -> Result<SqliteJournal, JournalConformanceError> {
    SqliteJournal::open(path, store_id(), SqliteJournalOptions::default())
        .map_err(|_| JournalConformanceError::Infrastructure)
}

fn store_id() -> StoreId {
    StoreId::new([0x62; 16]).expect("nonzero fixed store ID")
}

fn aggregate() -> AggregateKey {
    AggregateKey::new(
        AggregateKind::Kernel,
        AggregateId::new([0x63; 16]).expect("nonzero fixed aggregate ID"),
    )
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
