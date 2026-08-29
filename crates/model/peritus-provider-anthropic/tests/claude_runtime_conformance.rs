//! A2 qualification of the production Claude executable boundary.
#![cfg(feature = "test-runtime-fake")]

mod claude_runtime_conformance {
    mod observations;
    mod runner;
    mod support;

    use peritus_conformance::{
        ProviderConformanceError, ProviderConformanceFixture, ProviderConformanceObservation,
        ProviderConformanceSubject,
    };

    struct Subject;

    impl ProviderConformanceSubject for Subject {
        fn exercise(
            &mut self,
            fixture: &ProviderConformanceFixture,
        ) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
            observations::exercise(fixture)
        }
    }
}
