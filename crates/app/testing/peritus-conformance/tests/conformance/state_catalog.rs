use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, ConformanceRunner, JournalAppendDisposition,
    JournalAppendFixture, JournalAppendObservation, JournalConformanceError,
    JournalConformanceSubject, JournalSnapshot, ReplayConformanceError, ReplayConformanceSubject,
    ReplayObservation, SubjectDescriptor, SubjectFactory, SubjectFailure, SuiteStatus,
    journal_suite, replay_suite,
};

use super::harness::{block_on, text};

#[derive(Default)]
struct JournalSubject {
    committed: Option<JournalAppendFixture>,
}

impl JournalConformanceSubject for JournalSubject {
    fn append_absent(
        &mut self,
        fixture: &JournalAppendFixture,
    ) -> Result<JournalAppendObservation, JournalConformanceError> {
        match &self.committed {
            None => {
                self.committed = Some(fixture.clone());
                Ok(JournalAppendObservation::new(JournalAppendDisposition::Committed, 1))
            }
            Some(committed) if committed == fixture => {
                Ok(JournalAppendObservation::new(JournalAppendDisposition::Idempotent, 1))
            }
            Some(_) => Err(JournalConformanceError::StaleCas),
        }
    }

    fn restart(&mut self) -> Result<(), JournalConformanceError> {
        Ok(())
    }

    fn snapshot(&self) -> Result<JournalSnapshot, JournalConformanceError> {
        self.committed
            .as_ref()
            .map(|fixture| JournalSnapshot::new(1, 1, fixture.frame().to_vec()))
            .ok_or(JournalConformanceError::Infrastructure)
    }
}

#[derive(Default)]
struct ReplaySubject {
    frames: Vec<Vec<u8>>,
}

impl ReplayConformanceSubject for ReplaySubject {
    fn seed(&mut self, frames: &[Vec<u8>]) -> Result<(), ReplayConformanceError> {
        self.frames = frames.to_vec();
        Ok(())
    }

    fn replay_from_genesis(&mut self) -> Result<ReplayObservation, ReplayConformanceError> {
        let mut state = Vec::new();
        for frame in &self.frames {
            state.extend_from_slice(&(frame.len() as u64).to_be_bytes());
            state.extend_from_slice(frame);
        }
        let positions = (1..=self.frames.len()).map(|value| value as u64).collect();
        Ok(ReplayObservation::new(positions, state, 0))
    }

    fn restart(&mut self) -> Result<(), ReplayConformanceError> {
        Ok(())
    }
}

struct Factory<S> {
    descriptor: SubjectDescriptor,
    create: fn() -> S,
}

impl<S: Send> SubjectFactory<S> for Factory<S> {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<S, SubjectFailure>> {
        let subject = (self.create)();
        Box::pin(async move { Ok(subject) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: S,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn journal_catalog_executes_three_authoritative_contract_cases() {
    let suite = journal_suite::<JournalSubject>();
    let factory = Factory {
        descriptor: SubjectDescriptor::new(text("journal"), text("reference adapter")),
        create: JournalSubject::default,
    };
    let report = block_on(ConformanceRunner::run(&suite, &factory));
    assert_eq!(report.status(), SuiteStatus::Passed);
    assert_eq!(report.summary().total(), 3);
}

#[test]
fn replay_catalog_executes_determinism_and_restart_cases() {
    let suite = replay_suite::<ReplaySubject>();
    let factory = Factory {
        descriptor: SubjectDescriptor::new(text("replay"), text("reference adapter")),
        create: ReplaySubject::default,
    };
    let report = block_on(ConformanceRunner::run(&suite, &factory));
    assert_eq!(report.status(), SuiteStatus::Passed);
    assert_eq!(report.summary().total(), 2);
}
