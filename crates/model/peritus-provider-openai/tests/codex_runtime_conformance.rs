//! A2 qualification of the production `Codex` executable boundary.
#![cfg(feature = "test-runtime-fake")]

mod codex_runtime_conformance {
    mod diagnostics;
    mod hardening;
    mod observations;
    mod redaction;
    mod runner;
    mod support;

    use peritus_conformance::{
        ProviderConformanceError, ProviderConformanceFixture, ProviderConformanceObservation,
        ProviderConformanceSubject, ProviderExerciseResult,
    };

    struct Subject;

    impl ProviderConformanceSubject for Subject {
        fn exercise(
            &mut self,
            fixture: &ProviderConformanceFixture,
        ) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
            observations::exercise(fixture).map_err(|_| ProviderConformanceError::Infrastructure)
        }

        fn exercise_with_evidence(
            &mut self,
            fixture: &ProviderConformanceFixture,
        ) -> ProviderExerciseResult {
            match observations::exercise(fixture) {
                Ok(observed) => ProviderExerciseResult::new(Ok(observed), Vec::new()),
                Err(error) => ProviderExerciseResult::new(
                    Err(ProviderConformanceError::Infrastructure),
                    error.into_observations(),
                ),
            }
        }
    }
}
