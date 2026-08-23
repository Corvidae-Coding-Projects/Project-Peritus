use crate::support::{
    FixtureIds, capability_commit_claim_fixtures, capability_for_commit_claim, command, digest,
};

#[test]
fn forged_malformed_and_stale_commit_claim_fixtures_are_rejected_without_consumption() {
    let ids = FixtureIds::new();
    for fixture in capability_commit_claim_fixtures(&ids) {
        let capability = capability_for_commit_claim(&ids);
        let failure = capability
            .try_use(fixture.request, digest(90))
            .expect_err("invalid commit claim must fail");
        assert_eq!(failure.error().kind(), fixture.expected_error, "{:?}", fixture.kind);
        assert_eq!(failure.error().dimension(), fixture.expected_dimension, "{:?}", fixture.kind);
        assert_eq!(failure.capability().remaining_uses().remaining(), Some(2));
        assert_eq!(failure.capability().issuance_command_id(), command(1));
        assert_eq!(failure.capability().issuance_digest(), digest(2));
        assert_eq!(failure.capability().time_state().greatest_tick_millis(), 10);
    }
}
