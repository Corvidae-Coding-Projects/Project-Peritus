use peritus_journal::{
    AggregateKind, CapabilityCommitRequest, CommittedCapabilityUse, CommittedLeaseTransition,
    HeadExpectation, LeaseCommitRequest, SqliteJournal,
};
use peritus_leases::{
    AcquireLease, LeaseAggregate, LeaseDuration, LeaseHolder, LeaseScope, LeaseTransition,
    LeaseTransitionOutcome, LeaseUseOutcome, MintLease, UseLease,
};
use peritus_policy::CapabilityUseTransition;

use super::{Ids, authority::instant, journal};

pub(super) fn commit(
    journal: &mut SqliteJournal,
    ids: &Ids,
    capability: CapabilityUseTransition,
) -> (CommittedCapabilityUse, CommittedLeaseTransition) {
    let scope = LeaseScope::new(ids.workspace, ids.resource, ids.environment);
    let key = journal::aggregate(AggregateKind::Lease, 40);
    let mint = LeaseAggregate::mint(MintLease::new(journal::command(40), scope, instant(10)))
        .expect("mint lease");
    let minted = commit_lease(journal, ids, key, mint, 1, 40, None);
    let active = accepted(minted.into_parts().1.acquire(AcquireLease::new(
        journal::command(41),
        LeaseHolder::new(ids.actor, ids.session),
        LeaseDuration::new(50).expect("lease duration"),
        instant(10),
    )));
    let acquired = commit_lease(journal, ids, key, active, 2, 41, Some(journal::event(40)));
    let active = acquired.into_parts().1;
    let claim = active.active().expect("active lease").claim();
    let logical = match active.authorize_use(UseLease::new(
        journal::command(42),
        claim,
        instant(20),
        capability,
    )) {
        LeaseUseOutcome::Accepted(value) => value,
        LeaseUseOutcome::Rejected(failure) => panic!("lease use: {:?}", failure.error()),
    };
    let (lease_transition, capability_transition) = logical.into_parts();
    let capability_key = journal::aggregate(AggregateKind::Approval, 70);
    let capability = journal
        .commit_capability_use(
            CapabilityCommitRequest::new(
                journal::append(
                    capability_key,
                    journal::command(43),
                    1,
                    journal::event(43),
                    None,
                    HeadExpectation::Absent(capability_key),
                    ids.revision,
                ),
                capability_transition,
                None,
            )
            .expect("bind capability use"),
        )
        .expect("commit capability use");
    let lease = commit_lease(journal, ids, key, lease_transition, 3, 42, Some(journal::event(41)));
    (capability, lease)
}

#[allow(clippy::too_many_arguments)]
fn commit_lease(
    journal: &mut SqliteJournal,
    ids: &Ids,
    key: peritus_journal::AggregateKey,
    transition: LeaseTransition,
    sequence: u64,
    seed: u8,
    previous: Option<peritus_types::EventId>,
) -> CommittedLeaseTransition {
    let head = journal
        .head(key)
        .expect("lease head")
        .map_or(HeadExpectation::Absent(key), HeadExpectation::Present);
    journal
        .commit_lease_transition(
            LeaseCommitRequest::new(
                journal::append(
                    key,
                    journal::command(seed),
                    sequence,
                    journal::event(seed),
                    previous,
                    head,
                    ids.revision,
                ),
                transition,
            )
            .expect("bind lease transition"),
        )
        .expect("commit lease transition")
}

fn accepted(outcome: LeaseTransitionOutcome) -> LeaseTransition {
    match outcome {
        LeaseTransitionOutcome::Accepted(value) => value,
        LeaseTransitionOutcome::Rejected(failure) => {
            panic!("lease transition: {:?}", failure.error())
        }
    }
}
