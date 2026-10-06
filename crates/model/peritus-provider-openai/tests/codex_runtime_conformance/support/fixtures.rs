//! Obtain the canonical fixture through the public suite rather than forging its private fields.

use std::sync::mpsc::{Sender, channel};

use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, ConformanceRunner, ProviderConformanceError,
    ProviderConformanceFixture, ProviderConformanceObservation, ProviderConformanceSubject,
    ProviderScenario, ReportText, SubjectDescriptor, SubjectFactory, SubjectFailure,
    provider_suite,
};

struct Capture {
    wanted: ProviderScenario,
    sender: Sender<ProviderConformanceFixture>,
}

impl ProviderConformanceSubject for Capture {
    fn exercise(
        &mut self,
        fixture: &ProviderConformanceFixture,
    ) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
        if fixture.scenario() == self.wanted {
            self.sender.send(*fixture).expect("owned fixture receiver");
        }
        Err(ProviderConformanceError::Infrastructure)
    }
}

struct Factory {
    descriptor: SubjectDescriptor,
    wanted: ProviderScenario,
    sender: Sender<ProviderConformanceFixture>,
}

impl SubjectFactory<Capture> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<Capture, SubjectFailure>> {
        let subject = Capture { wanted: self.wanted, sender: self.sender.clone() };
        Box::pin(async move { Ok(subject) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: Capture,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

pub(super) fn fixture(wanted: ProviderScenario) -> ProviderConformanceFixture {
    let (sender, receiver) = channel();
    let factory = Factory {
        descriptor: SubjectDescriptor::new(
            ReportText::new("fixture capture").unwrap(),
            ReportText::new("canonical suite inputs only; no provider qualification").unwrap(),
        ),
        wanted,
        sender,
    };
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let report = runtime.block_on(ConformanceRunner::run(&provider_suite::<Capture>(), &factory));
    assert_eq!(report.summary().infrastructure_failure_cases(), 14);
    assert_eq!(report.summary().contract_violation_cases(), 0);
    let fixture = receiver.try_recv().expect("selected canonical fixture was supplied");
    assert!(receiver.try_recv().is_err(), "exactly one selected fixture");
    fixture
}
