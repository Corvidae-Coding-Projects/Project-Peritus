//! Runtime-neutral A2 adapter backed by production sandbox values.

mod exercise;
mod plan_fixture;
mod preparation;

use peritus_conformance::{
    SandboxConformanceError, SandboxConformanceFixture, SandboxConformanceObservation,
    SandboxConformanceSubject, SandboxPreparationFixture, SandboxPreparationObservation,
};
use peritus_sandbox::ReferenceBackend;

/// Fresh per-case subject using the executable production reference backend.
pub struct ProductionSandboxSubject {
    backend: ReferenceBackend,
}

impl ProductionSandboxSubject {
    /// Creates a fresh fault-free subject.
    #[must_use]
    pub fn new() -> Self {
        Self { backend: ReferenceBackend::default() }
    }
}

impl SandboxConformanceSubject for ProductionSandboxSubject {
    fn exercise(
        &mut self,
        fixture: &SandboxConformanceFixture,
    ) -> Result<SandboxConformanceObservation, SandboxConformanceError> {
        exercise::exercise(&self.backend, fixture)
            .map_err(|_| SandboxConformanceError::Infrastructure)
    }

    fn prepare(
        &mut self,
        fixture: &SandboxPreparationFixture,
    ) -> Result<SandboxPreparationObservation, SandboxConformanceError> {
        preparation::prepare(fixture).map_err(|_| SandboxConformanceError::Infrastructure)
    }
}
