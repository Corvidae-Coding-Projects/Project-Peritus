//! A2 workspace conformance against production Git, patch, filesystem, and artifact effects.

mod support;

use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    thread,
};

use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, ConformanceRunner, FailureCode, ReportText,
    SubjectDescriptor, SubjectFactory, SubjectFailure, SuiteStatus, workspace_suite,
};

use support::ProductionWorkspaceSubject;

struct Factory(SubjectDescriptor);

impl Factory {
    fn new() -> Self {
        Self(SubjectDescriptor::new(
            ReportText::new("peritus-workspace").expect("fixed subject name"),
            ReportText::new("production Git/patch/artifact adapter")
                .expect("fixed subject summary"),
        ))
    }
}

impl SubjectFactory<ProductionWorkspaceSubject> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.0
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ProductionWorkspaceSubject, SubjectFailure>> {
        Box::pin(async {
            ProductionWorkspaceSubject::new().map_err(|_| {
                SubjectFailure::new(
                    FailureCode::new("C1-WORKSPACE-SETUP").expect("fixed failure code"),
                    ReportText::new("production workspace setup failed")
                        .expect("fixed setup failure"),
                )
            })
        })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ProductionWorkspaceSubject,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn production_workspace_passes_a2_git_patch_snapshot_and_recovery_suite() {
    let report = block_on(ConformanceRunner::run(
        &workspace_suite::<ProductionWorkspaceSubject>(),
        &Factory::new(),
    ));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 6);
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
