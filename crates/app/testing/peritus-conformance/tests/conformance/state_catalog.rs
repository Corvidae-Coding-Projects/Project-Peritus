use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, ConformanceRunner, JournalAppendDisposition,
    JournalAppendFixture, JournalAppendObservation, JournalConformanceError,
    JournalConformanceSubject, JournalSnapshot, ReplayConformanceError, ReplayConformanceSubject,
    ReplayObservation, SubjectDescriptor, SubjectFactory, SubjectFailure, SuiteStatus,
    WorkspaceConformanceError, WorkspaceConformanceSubject, WorkspaceMutationDisposition,
    WorkspaceMutationObservation, WorkspacePatchFixture, WorkspaceReconciliationDisposition,
    WorkspaceSnapshot, journal_suite, replay_suite, workspace_suite,
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

struct WorkspaceSubject {
    snapshot: WorkspaceSnapshot,
    dirty: bool,
    indeterminate: bool,
}

impl Default for WorkspaceSubject {
    fn default() -> Self {
        Self {
            snapshot: WorkspaceSnapshot::new(1, 1, vec![1; 20], None, None, true, false, false),
            dirty: false,
            indeterminate: false,
        }
    }
}

impl WorkspaceConformanceSubject for WorkspaceSubject {
    fn snapshot(&self) -> Result<WorkspaceSnapshot, WorkspaceConformanceError> {
        Ok(self.snapshot.clone())
    }

    fn apply(
        &mut self,
        fixture: &WorkspacePatchFixture,
    ) -> Result<WorkspaceMutationObservation, WorkspaceConformanceError> {
        let disposition = if fixture.resource_id() != [2; 16] {
            WorkspaceMutationDisposition::Unauthorized
        } else if fixture.generation() != self.snapshot.generation()
            || fixture.revision() != self.snapshot.revision()
        {
            WorkspaceMutationDisposition::Stale
        } else {
            self.snapshot = WorkspaceSnapshot::new(
                self.snapshot.generation(),
                self.snapshot.revision() + 1,
                vec![2; 20],
                Some(fixture.first_contents().to_vec()),
                Some(fixture.second_contents().to_vec()),
                true,
                true,
                false,
            );
            WorkspaceMutationDisposition::Applied
        };
        Ok(WorkspaceMutationObservation::new(disposition, self.snapshot.clone()))
    }

    fn apply_read_only(
        &mut self,
        _fixture: &WorkspacePatchFixture,
    ) -> Result<WorkspaceMutationObservation, WorkspaceConformanceError> {
        Ok(WorkspaceMutationObservation::new(
            WorkspaceMutationDisposition::ReadOnly,
            self.snapshot.clone(),
        ))
    }

    fn rollback(&mut self) -> Result<WorkspaceSnapshot, WorkspaceConformanceError> {
        self.snapshot = WorkspaceSnapshot::new(
            self.snapshot.generation(),
            self.snapshot.revision() + 1,
            vec![1; 20],
            None,
            None,
            true,
            true,
            true,
        );
        Ok(self.snapshot.clone())
    }

    fn restart(&mut self) -> Result<(), WorkspaceConformanceError> {
        Ok(())
    }

    fn make_dirty(&mut self) -> Result<(), WorkspaceConformanceError> {
        self.dirty = true;
        Ok(())
    }

    fn make_indeterminate(&mut self) -> Result<(), WorkspaceConformanceError> {
        self.indeterminate = true;
        Ok(())
    }

    fn reconcile(
        &mut self,
        expected_generation: u64,
    ) -> Result<WorkspaceReconciliationDisposition, WorkspaceConformanceError> {
        Ok(if expected_generation != self.snapshot.generation() {
            WorkspaceReconciliationDisposition::Fenced
        } else if self.indeterminate {
            WorkspaceReconciliationDisposition::Indeterminate
        } else if self.dirty {
            WorkspaceReconciliationDisposition::Dirty
        } else {
            WorkspaceReconciliationDisposition::Clean
        })
    }
}

#[test]
fn workspace_catalog_executes_git_patch_authority_and_recovery_cases() {
    let suite = workspace_suite::<WorkspaceSubject>();
    let factory = Factory {
        descriptor: SubjectDescriptor::new(text("workspace"), text("reference C1 adapter")),
        create: WorkspaceSubject::default,
    };
    let report = block_on(ConformanceRunner::run(&suite, &factory));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 6);
}
