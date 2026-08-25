//! Scheduler binding digest regression coverage.

mod support;

use peritus_scheduler::{SchedulerBinding, SchedulerId};

use support::{Fixture, bytes};

#[test]
fn canonical_binding_digest_is_stable_and_commits_every_binding_dimension() {
    let fixture = Fixture::new();
    let same = fixture.binding.clone();
    let changed_identity = SchedulerBinding::new(
        fixture.binding.run_id(),
        SchedulerId::new(bytes(91)).expect("fixture scheduler identity is nonzero"),
        fixture.binding.revision(),
        fixture.binding.limits(),
        fixture.binding.capacity().clone(),
    )
    .expect("fixture binding remains valid");

    assert_eq!(fixture.binding.digest(), same.digest());
    assert_ne!(fixture.binding.digest(), changed_identity.digest());
}
