//! Deterministic sequential suite execution.

use crate::unwind::{GuardedFuture, callback};
use crate::{
    CaseDescriptor, CaseFailure, CaseReport, ConformanceCase, ConformanceSuite,
    DuplicateCaseIdFailure, FailurePhase, SubjectFactory, SuiteFailure, SuiteReport,
    TeardownFailure,
};

/// Stateless deterministic conformance-suite runner.
pub struct ConformanceRunner;

impl ConformanceRunner {
    /// Runs a suite sequentially against fresh subjects from `factory`.
    ///
    /// Case definitions are cloned, sorted bytewise by stable case ID, and checked for duplicates
    /// before subject creation. The returned future does not require a particular async runtime.
    ///
    /// # Cancellation
    ///
    /// When this future is polled to `Ready`, every subject returned by a completed setup future
    /// whose destruction did not panic is passed to [`SubjectFactory::teardown`] exactly once.
    /// Dropping this future while an operation is pending instead drops the in-flight operation and
    /// subject in place. Cancellation before teardown begins does not invoke it; cancellation of an
    /// already-pending teardown future drops it without awaiting completion. A pending future's
    /// destructor panic unwinds from the caller's drop and cannot be included in a report that is
    /// never produced. Subjects must be cancellation-safe through synchronous RAII; production
    /// supervisors must poll qualification runs to a terminal report or classify external
    /// cancellation as an infrastructure failure.
    ///
    /// ```
    /// use peritus_conformance::{
    ///     CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    ///     ConformanceRunner, ReportText, StaticSuite, SubjectDescriptor, SubjectFactory,
    ///     SubjectFailure, SuiteDescriptor, SuiteId, SuiteStatus,
    /// };
    ///
    /// struct Subject;
    ///
    /// struct PassingCase(CaseDescriptor);
    ///
    /// impl ConformanceCase<Subject> for PassingCase {
    ///     fn descriptor(&self) -> &CaseDescriptor {
    ///         &self.0
    ///     }
    ///
    ///     fn run<'a>(
    ///         &'a self,
    ///         _subject: &'a mut Subject,
    ///     ) -> ConformanceFuture<'a, CaseResult> {
    ///         Box::pin(async { CaseResult::passed(Vec::new()) })
    ///     }
    /// }
    ///
    /// struct Factory(SubjectDescriptor);
    ///
    /// impl SubjectFactory<Subject> for Factory {
    ///     fn descriptor(&self) -> &SubjectDescriptor {
    ///         &self.0
    ///     }
    ///
    ///     fn create<'a>(
    ///         &'a self,
    ///         _case: &'a CaseDescriptor,
    ///     ) -> ConformanceFuture<'a, Result<Subject, SubjectFailure>> {
    ///         Box::pin(async { Ok(Subject) })
    ///     }
    ///
    ///     fn teardown<'a>(
    ///         &'a self,
    ///         _case: &'a CaseDescriptor,
    ///         _subject: Subject,
    ///     ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
    ///         Box::pin(async { Ok(()) })
    ///     }
    /// }
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let suite = StaticSuite::new(
    ///     SuiteDescriptor::new(SuiteId::new("example.suite")?, ReportText::new("example")?),
    ///     vec![Box::new(PassingCase(CaseDescriptor::new(
    ///         CaseId::new("example.passes")?,
    ///         ReportText::new("passes")?,
    ///     )))],
    /// );
    /// let factory = Factory(SubjectDescriptor::new(
    ///     ReportText::new("example")?,
    ///     ReportText::new("v1")?,
    /// ));
    /// let report = ConformanceRunner::run(&suite, &factory).await;
    /// assert_eq!(report.status(), SuiteStatus::Passed);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn run<S, Suite, Factory>(suite: &Suite, factory: &Factory) -> SuiteReport
    where
        S: Send,
        Suite: ConformanceSuite<S> + ?Sized,
        Factory: SubjectFactory<S> + ?Sized,
    {
        let suite_descriptor =
            match callback(FailurePhase::SuiteDefinition, || suite.descriptor().clone()) {
                Ok(descriptor) => descriptor,
                Err(failure) => {
                    return SuiteReport::invalid(None, None, SuiteFailure::Panic(failure));
                }
            };
        let subject_descriptor =
            match callback(FailurePhase::SubjectDefinition, || factory.descriptor().clone()) {
                Ok(descriptor) => descriptor,
                Err(failure) => {
                    return SuiteReport::invalid(
                        Some(suite_descriptor),
                        None,
                        SuiteFailure::Panic(failure),
                    );
                }
            };
        let registered = match callback(FailurePhase::SuiteDefinition, || suite.cases()) {
            Ok(cases) => cases,
            Err(failure) => {
                return SuiteReport::invalid(
                    Some(suite_descriptor),
                    Some(subject_descriptor),
                    SuiteFailure::Panic(failure),
                );
            }
        };
        let mut cases = Vec::with_capacity(registered.len());
        for case in registered {
            let descriptor =
                match callback(FailurePhase::CaseDefinition, || case.descriptor().clone()) {
                    Ok(descriptor) => descriptor,
                    Err(failure) => {
                        return SuiteReport::invalid(
                            Some(suite_descriptor),
                            Some(subject_descriptor),
                            SuiteFailure::Panic(failure),
                        );
                    }
                };
            cases.push((descriptor, case.as_ref()));
        }
        cases.sort_by(|left, right| left.0.id().cmp(right.0.id()));
        if let Some(duplicate) = cases.windows(2).find(|pair| pair[0].0.id() == pair[1].0.id()) {
            return SuiteReport::invalid(
                Some(suite_descriptor),
                Some(subject_descriptor),
                SuiteFailure::DuplicateCaseId(DuplicateCaseIdFailure::new(
                    duplicate[0].0.id().clone(),
                )),
            );
        }

        let mut reports = Vec::with_capacity(cases.len());
        for (descriptor, case) in cases {
            reports.push(run_case(case, descriptor, factory).await);
        }
        SuiteReport::complete(suite_descriptor, subject_descriptor, reports)
    }
}

async fn run_case<S, Factory>(
    case: &dyn ConformanceCase<S>,
    descriptor: CaseDescriptor,
    factory: &Factory,
) -> CaseReport
where
    S: Send,
    Factory: SubjectFactory<S> + ?Sized,
{
    let setup_future = match callback(FailurePhase::Setup, || factory.create(&descriptor)) {
        Ok(future) => future,
        Err(failure) => {
            return CaseReport::new(
                descriptor.clone(),
                false,
                Vec::new(),
                Some(CaseFailure::Panic(failure)),
                None,
            );
        }
    };
    let mut subject = match GuardedFuture::new(setup_future, FailurePhase::Setup).await {
        Ok(Ok(subject)) => subject,
        Ok(Err(failure)) => {
            return CaseReport::new(
                descriptor.clone(),
                false,
                Vec::new(),
                Some(CaseFailure::Setup(failure)),
                None,
            );
        }
        Err(failure) => {
            return CaseReport::new(
                descriptor.clone(),
                false,
                Vec::new(),
                Some(CaseFailure::Panic(failure)),
                None,
            );
        }
    };

    let (observations, primary_failure) =
        match callback(FailurePhase::Exercise, || case.run(&mut subject)) {
            Ok(future) => match GuardedFuture::new(future, FailurePhase::Exercise).await {
                Ok(result) => {
                    let (observations, failure) = result.into_parts();
                    (observations, failure.map(CaseFailure::Assertion))
                }
                Err(failure) => (Vec::new(), Some(CaseFailure::Panic(failure))),
            },
            Err(failure) => (Vec::new(), Some(CaseFailure::Panic(failure))),
        };

    let teardown_failure =
        match callback(FailurePhase::Teardown, || factory.teardown(&descriptor, subject)) {
            Ok(future) => match GuardedFuture::new(future, FailurePhase::Teardown).await {
                Ok(Ok(())) => None,
                Ok(Err(failure)) => Some(TeardownFailure::Subject(failure)),
                Err(failure) => Some(TeardownFailure::Panic(failure)),
            },
            Err(failure) => Some(TeardownFailure::Panic(failure)),
        };

    CaseReport::new(descriptor, true, observations, primary_failure, teardown_failure)
}
