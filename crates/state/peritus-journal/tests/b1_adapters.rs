//! Durable integration coverage for capability, budget, and lease transitions.

mod support;

use peritus_budget::{BudgetCommand, BudgetOperation, ReservationReference};
use peritus_journal::{
    BudgetCommitRequest, CapabilityCommitRequest, HeadExpectation, JournalErrorKind,
    LeaseCommitRequest,
};
use peritus_leases::{
    AcquireLease, LeaseAggregate, LeaseDuration, LeaseHolder, LeasePhase, LeaseScope, MintLease,
};
use peritus_types::ActorId;
use tempfile::TempDir;

use support::b1::{accepted_lease, capability_use, held_budget, instant, lease_key};
use support::{DomainIds, aggregate, append_request, command, digest, event, frame, open};

#[test]
fn capability_use_is_exposed_only_with_its_exact_durable_successor() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let mut ids = DomainIds::new(*b"capabil1");
    let transition = capability_use(&mut ids);
    let issuance_command = transition.successor().issuance_command_id();
    let aggregate = aggregate(peritus_journal::AggregateKind::Approval, 20);
    let append = append_request(
        command(20),
        digest(20),
        HeadExpectation::Absent(aggregate),
        1,
        event(20),
        None,
        frame(20),
        digest(120),
    );
    let committed = journal
        .commit_capability_use(
            CapabilityCommitRequest::new(append, transition, None).expect("capability commit"),
        )
        .expect("durable capability use");

    assert_eq!(committed.state_revision(), 1);
    assert_eq!(committed.transition().successor().remaining_uses().remaining(), Some(2));
    let state = journal
        .state_record(101, issuance_command.as_bytes())
        .expect("capability state")
        .expect("capability state present");
    assert_eq!(state.revision(), 1);
    assert_eq!(state.digest(), committed.state_digest());
    assert_eq!(state.producing_position(), committed.batch().last_position());
}

#[test]
fn held_budget_cancellation_requires_and_consumes_a_current_durable_non_activation_observation() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let mut ids = DomainIds::new(*b"budget02");
    let fixture = held_budget(&mut ids);
    let reservation_id = fixture.reservation_id;
    let aggregate = aggregate(peritus_journal::AggregateKind::Budget, 30);
    let begin = journal
        .commit_budget_transition(
            BudgetCommitRequest::new(
                append_request(
                    command(30),
                    digest(30),
                    HeadExpectation::Absent(aggregate),
                    1,
                    event(30),
                    None,
                    frame(30),
                    digest(130),
                ),
                fixture.begin,
                None,
                None,
            )
            .expect("bind budget begin"),
        )
        .expect("commit budget begin");
    let (_, ledger) = begin.into_parts();
    let observation =
        journal.observe_budget_non_activation(reservation_id).expect("durable held observation");
    assert_eq!(observation.reservation_id(), reservation_id);
    assert_eq!(observation.producing_position(), 1);

    let reference =
        ReservationReference::new(reservation_id, fixture.action_id, digest(30), digest(31));
    let head = journal.head(aggregate).expect("budget head").expect("present");
    let missing_observation = BudgetCommitRequest::new(
        budget_cancel_append(head),
        ledger.transition(BudgetCommand::CancelHeld(reference)).expect("logical cancellation"),
        Some(1),
        None,
    );
    assert_eq!(
        missing_observation.err().expect("missing observation rejected").kind(),
        JournalErrorKind::InvalidInput
    );

    let cancellation =
        ledger.transition(BudgetCommand::CancelHeld(reference)).expect("logical cancellation");
    let committed = journal
        .commit_budget_transition(
            BudgetCommitRequest::new(
                budget_cancel_append(head),
                cancellation,
                Some(1),
                Some(&observation),
            )
            .expect("bind durable held cancellation"),
        )
        .expect("commit held cancellation");
    assert_eq!(committed.state_revision(), 2);
    assert_eq!(committed.transition().receipt().operation(), BudgetOperation::CancelHeld);
    assert_eq!(committed.transition().receipt().released(), fixture.reserve);
    let no_longer_held = journal.observe_budget_non_activation(reservation_id);
    let Err(error) = no_longer_held else {
        panic!("cancellation remained observable as held");
    };
    assert_eq!(error.kind(), JournalErrorKind::NotFound);
    assert!(
        journal
            .state_record_revision(102, reservation_id.as_bytes(), 1)
            .expect("begin history")
            .is_some()
    );
}

#[test]
fn lease_mint_and_acquisition_follow_the_verified_version_cas() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let mut ids = DomainIds::new(*b"leases02");
    let actor = ids.next(ActorId::new);
    let session = ids.next(peritus_types::SessionId::new);
    let scope = LeaseScope::new(ids.workspace, ids.resource, ids.environment);
    let holder = LeaseHolder::new(actor, session);
    let aggregate = aggregate(peritus_journal::AggregateKind::Lease, 40);

    let mint = LeaseAggregate::mint(MintLease::new(command(40), scope, instant(10)))
        .expect("logical lease mint");
    let minted = journal
        .commit_lease_transition(
            LeaseCommitRequest::new(
                append_request(
                    command(40),
                    digest(40),
                    HeadExpectation::Absent(aggregate),
                    1,
                    event(40),
                    None,
                    frame(40),
                    digest(140),
                ),
                mint,
            )
            .expect("bind lease mint"),
        )
        .expect("commit lease mint");
    assert_eq!(minted.state_revision(), 1);
    let (_, available) = minted.into_parts();
    let acquired = accepted_lease(available.acquire(AcquireLease::new(
        command(41),
        holder,
        LeaseDuration::new(50).expect("lease duration"),
        instant(10),
    )));
    let head = journal.head(aggregate).expect("lease head").expect("present");
    let committed = journal
        .commit_lease_transition(
            LeaseCommitRequest::new(
                append_request(
                    command(41),
                    digest(41),
                    HeadExpectation::Present(head),
                    2,
                    event(41),
                    Some(event(40)),
                    frame(41),
                    digest(141),
                ),
                acquired,
            )
            .expect("bind lease acquisition"),
        )
        .expect("commit lease acquisition");
    assert_eq!(committed.state_revision(), 2);
    assert_eq!(committed.transition().next().phase(), LeasePhase::Active);
    assert_eq!(committed.transition().next().active().expect("active").claim().holder(), holder);
    assert_eq!(
        journal
            .state_record_revision(103, &lease_key(scope), 1)
            .expect("mint history")
            .expect("mint retained")
            .revision(),
        1
    );
}

fn budget_cancel_append(head: peritus_journal::AggregateHead) -> peritus_journal::AppendRequest {
    append_request(
        command(31),
        digest(31),
        HeadExpectation::Present(head),
        2,
        event(31),
        Some(event(30)),
        frame(31),
        digest(131),
    )
}
