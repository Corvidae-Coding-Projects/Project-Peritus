//! A2 adapter that observes the production protocol and router.

mod dispatcher;
mod exercise;
mod fixture;
mod observation;

use peritus_conformance::{
    ToolConformanceError, ToolConformanceFixture, ToolConformanceObservation,
    ToolConformanceSubject,
};

pub struct ProductionToolSubject {
    next_seed: u8,
}

impl ProductionToolSubject {
    pub const fn new(seed: u8) -> Self {
        Self { next_seed: seed }
    }
}

impl ToolConformanceSubject for ProductionToolSubject {
    fn exercise(
        &mut self,
        request: &ToolConformanceFixture,
    ) -> Result<ToolConformanceObservation, ToolConformanceError> {
        let seed = self.next_seed;
        self.next_seed = self.next_seed.wrapping_add(23).max(1);
        exercise::run(request, seed)
    }
}
