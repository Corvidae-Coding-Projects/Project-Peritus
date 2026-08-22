//! Runtime-neutral extension contracts for suites, cases, and subject factories.

use std::future::Future;
use std::pin::Pin;

use crate::{CaseDescriptor, CaseResult, SubjectDescriptor, SubjectFailure, SuiteDescriptor};

/// A boxed, runtime-neutral asynchronous operation used by conformance contracts.
pub type ConformanceFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A boxed heterogeneous case for a specific subject type.
pub type BoxedCase<S> = Box<dyn ConformanceCase<S>>;

/// One independently runnable contract check against a fresh subject.
pub trait ConformanceCase<S>: Send + Sync {
    /// Returns immutable case metadata.
    fn descriptor(&self) -> &CaseDescriptor;

    /// Exercises the subject and returns observations plus an optional assertion failure.
    ///
    /// If the enclosing runner future is cancelled, this future is dropped in place. Its
    /// destructor must not depend on asynchronous cleanup.
    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult>;
}

/// An extensible collection of conformance cases for one subject type.
pub trait ConformanceSuite<S>: Send + Sync {
    /// Returns immutable suite metadata.
    fn descriptor(&self) -> &SuiteDescriptor;

    /// Returns the registered cases. The runner validates and sorts this collection by case ID.
    fn cases(&self) -> &[BoxedCase<S>];
}

/// Creates and tears down an isolated subject for every conformance case.
pub trait SubjectFactory<S>: Send + Sync {
    /// Returns immutable metadata for the implementation under test.
    fn descriptor(&self) -> &SubjectDescriptor;

    /// Creates a fresh subject for `case`.
    ///
    /// Returned subjects must perform cancellation-safe synchronous cleanup through RAII because
    /// dropping a pending runner before teardown begins cannot call [`Self::teardown`].
    fn create<'a>(
        &'a self,
        case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<S, SubjectFailure>>;

    /// Tears down a successfully created subject after its case has finished or panicked.
    ///
    /// This method is called exactly once only when the runner is polled to completion after setup
    /// completes and the completed setup future is destroyed without panic. Cancelling a pending
    /// runner before teardown begins drops the subject in place and does not call this method. If
    /// its returned future is already pending, cancellation drops that future without awaiting it.
    fn teardown<'a>(
        &'a self,
        case: &'a CaseDescriptor,
        subject: S,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>>;
}

/// An owned suite implementation suitable for static and dynamically assembled case catalogs.
pub struct StaticSuite<S> {
    descriptor: SuiteDescriptor,
    cases: Vec<BoxedCase<S>>,
}

impl<S> StaticSuite<S> {
    /// Creates an empty suite.
    #[must_use]
    pub const fn empty(descriptor: SuiteDescriptor) -> Self {
        Self { descriptor, cases: Vec::new() }
    }

    /// Creates a suite from cases in arbitrary registration order.
    ///
    /// The runner sorts by case ID and rejects duplicates before subject creation.
    #[must_use]
    pub const fn new(descriptor: SuiteDescriptor, cases: Vec<BoxedCase<S>>) -> Self {
        Self { descriptor, cases }
    }
}

impl<S> ConformanceSuite<S> for StaticSuite<S> {
    fn descriptor(&self) -> &SuiteDescriptor {
        &self.descriptor
    }

    fn cases(&self) -> &[BoxedCase<S>] {
        &self.cases
    }
}
