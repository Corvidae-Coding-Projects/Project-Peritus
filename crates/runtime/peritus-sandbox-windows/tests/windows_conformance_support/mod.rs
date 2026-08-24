mod exercise;
mod plan;
mod preparation;
mod projection;

use peritus_conformance::{
    SandboxConformanceError, SandboxConformanceFixture, SandboxConformanceObservation,
    SandboxConformanceSubject, SandboxPreparationFixture, SandboxPreparationObservation,
};

pub struct WindowsConformanceSubject;

impl WindowsConformanceSubject {
    pub const fn new() -> Self {
        Self
    }
}

impl SandboxConformanceSubject for WindowsConformanceSubject {
    fn exercise(
        &mut self,
        fixture: &SandboxConformanceFixture,
    ) -> Result<SandboxConformanceObservation, SandboxConformanceError> {
        exercise::run(fixture).map_err(|()| SandboxConformanceError::Infrastructure)
    }

    fn prepare(
        &mut self,
        fixture: &SandboxPreparationFixture,
    ) -> Result<SandboxPreparationObservation, SandboxConformanceError> {
        preparation::run(fixture).map_err(|()| SandboxConformanceError::Infrastructure)
    }
}
