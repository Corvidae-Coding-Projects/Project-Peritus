use super::*;
use peritus_leases::{LeaseHolder, LeaseScope};
use peritus_types::{ActorId, EnvironmentId, Generation, ResourceId, SessionId, WorkspaceId};

fn correlation(seed: u8) -> ReconciliationCorrelation {
    ReconciliationCorrelation::new(
        LeaseScope::new(
            WorkspaceId::new([seed; 16]).unwrap(),
            ResourceId::new([seed.wrapping_add(1); 16]).unwrap(),
            EnvironmentId::new([seed.wrapping_add(2); 16]).unwrap(),
        ),
        Generation::first(),
        LeaseHolder::new(
            ActorId::new([seed.wrapping_add(3); 16]).unwrap(),
            SessionId::new([seed.wrapping_add(4); 16]).unwrap(),
        ),
    )
}

fn with_holder(correlation: ReconciliationCorrelation, seed: u8) -> ReconciliationCorrelation {
    ReconciliationCorrelation::new(
        correlation.scope(),
        correlation.fenced_generation(),
        LeaseHolder::new(
            ActorId::new([seed; 16]).unwrap(),
            SessionId::new([seed.wrapping_add(1); 16]).unwrap(),
        ),
    )
}

#[test]
fn post_fence_classification_never_guesses_safety() {
    let expected = correlation(1);
    let detail = Sha256Digest::new([9; 32]);
    assert_eq!(
        classify(ReconciliationInput::new(expected, expected, true, true, true, detail,))
            .disposition(),
        RestartDisposition::Clean,
    );
    assert_eq!(
        classify(ReconciliationInput::new(expected, expected, true, false, true, detail,))
            .disposition(),
        RestartDisposition::Dirty,
    );
    assert_eq!(
        classify(ReconciliationInput::new(expected, correlation(2), true, true, true, detail,))
            .disposition(),
        RestartDisposition::Fenced,
    );
    let wrong_holder = with_holder(expected, 31);
    let wrong_holder_observation =
        classify(ReconciliationInput::new(expected, wrong_holder, true, true, true, detail));
    assert_eq!(wrong_holder_observation.disposition(), RestartDisposition::Fenced);
    assert_eq!(wrong_holder_observation.evidence().correlation(), wrong_holder);
    assert_eq!(
        classify(ReconciliationInput::new(expected, expected, false, true, true, detail,))
            .disposition(),
        RestartDisposition::Indeterminate,
    );
}
