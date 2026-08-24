#![cfg(feature = "test-runtime-fake")]
//! A2 qualification of the production `Codex` executable boundary.

mod codex_runtime_conformance {
    mod observations;
    mod redaction;
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
