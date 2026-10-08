//! Writable workspace lease consumption for one C2 product command.

use peritus_journal::{
    AggregateKey, AggregateKind, CapabilityCommitRequest, CommittedCapabilityUse,
    CommittedLeaseTransition, HeadExpectation, LeaseCommitRequest, SqliteJournal,
};
use peritus_leases::{
    AcquireLease, LeaseAggregate, LeaseDuration, LeaseHolder, LeaseScope, LeaseTransition,
    LeaseTransitionOutcome, LeaseUseOutcome, MintLease, UseLease,
};
use peritus_policy::CapabilityUseTransition;

use super::{authority::instant, identity::CommandIds, journal};

const LEASE_ACQUIRE_TICK_MILLIS: u64 = 10;
const LEASE_USE_TICK_MILLIS: u64 = 20;
const AUTHORITY_ONLY_LEASE_DURATION_MILLIS: u64 =
    LEASE_USE_TICK_MILLIS - LEASE_ACQUIRE_TICK_MILLIS + 1;

pub(super) fn commit_with_wall_policy(
    store: &mut SqliteJournal,
    store_label: &str,
    ids: &CommandIds,
    capability: CapabilityUseTransition,
    wall_millis: Option<u64>,
) -> Result<(CommittedCapabilityUse, CommittedLeaseTransition), String> {
    commit_with_profile(
        store,
        store_label,
        ids,
        capability,
        LeaseProfile::Command { wall_millis },
    )
}

/// Commits a lease used only by the immediate pre-effect authority transaction.
///
/// The claim is acquired at logical tick 10, used at tick 20, and expires at the exclusive tick
/// 21 boundary. C1 consumes the resulting durable receipt; this lease does not bound later target
/// work or recovery.
pub(super) fn commit_authority_only(
    store: &mut SqliteJournal,
    store_label: &str,
    ids: &CommandIds,
    capability: CapabilityUseTransition,
) -> Result<(CommittedCapabilityUse, CommittedLeaseTransition), String> {
    commit_with_profile(store, store_label, ids, capability, LeaseProfile::AuthorityOnly)
}

enum LeaseProfile {
    Command { wall_millis: Option<u64> },
    AuthorityOnly,
}

fn commit_with_profile(
    store: &mut SqliteJournal,
    store_label: &str,
    ids: &CommandIds,
    capability: CapabilityUseTransition,
    profile: LeaseProfile,
) -> Result<(CommittedCapabilityUse, CommittedLeaseTransition), String> {
    let scope = LeaseScope::new(ids.workspace, ids.resource, ids.environment);
    let key = AggregateKey::new(AggregateKind::Lease, ids.aggregate("lease")?);
    let mint = LeaseAggregate::mint(MintLease::new(
        ids.command("lease-mint-command")?,
        scope,
        instant(LEASE_ACQUIRE_TICK_MILLIS),
    ))
    .map_err(|error| format!("mint command workspace lease: {error:?}"))?;
    let minted = commit_lease(store, store_label, ids, key, mint, 1, "lease-mint", None)?;
    let duration_millis = match profile {
        LeaseProfile::Command { wall_millis: Some(wall_millis) } => {
            wall_millis.saturating_add(10_000)
        }
        LeaseProfile::Command { wall_millis: None } => AUTHORITY_ONLY_LEASE_DURATION_MILLIS,
        LeaseProfile::AuthorityOnly => AUTHORITY_ONLY_LEASE_DURATION_MILLIS,
    };
    let active = accepted(
        minted.into_parts().1.acquire(AcquireLease::new(
            ids.command("lease-acquire-command")?,
            LeaseHolder::new(ids.actor, ids.session),
            LeaseDuration::new(duration_millis)
                .map_err(|error| format!("construct command lease duration: {error:?}"))?,
            instant(LEASE_ACQUIRE_TICK_MILLIS),
        )),
    )?;
    let acquired = commit_lease(
        store,
        store_label,
        ids,
        key,
        active,
        2,
        "lease-acquire",
        Some(ids.event("lease-mint-event")?),
    )?;
    let active = acquired.into_parts().1;
    let claim = active
        .active()
        .ok_or_else(|| "command workspace lease did not become active".to_owned())?
        .claim();
    let logical = match active.authorize_use(UseLease::new(
        ids.command("lease-use-command")?,
        claim,
        instant(LEASE_USE_TICK_MILLIS),
        capability,
    )) {
        LeaseUseOutcome::Accepted(value) => value,
        LeaseUseOutcome::Rejected(failure) => {
            return Err(format!("authorize command workspace lease use: {:?}", failure.error()));
        }
    };
    let (lease_transition, capability_transition) = logical.into_parts();
    let capability_key = AggregateKey::new(AggregateKind::Approval, ids.aggregate("capability")?);
    let capability = store
        .commit_capability_use(
            CapabilityCommitRequest::new(
                journal::append(
                    ids,
                    store_label,
                    capability_key,
                    "capability-use-command",
                    1,
                    "capability-use-event",
                    None,
                    HeadExpectation::Absent(capability_key),
                )?,
                capability_transition,
                None,
            )
            .map_err(|error| format!("bind command capability use: {error}"))?,
        )
        .map_err(|error| format!("commit command capability use: {error}"))?;
    let lease = commit_lease(
        store,
        store_label,
        ids,
        key,
        lease_transition,
        3,
        "lease-use",
        Some(ids.event("lease-acquire-event")?),
    )?;
    Ok((capability, lease))
}

#[allow(clippy::too_many_arguments)]
fn commit_lease(
    store: &mut SqliteJournal,
    store_label: &str,
    ids: &CommandIds,
    key: AggregateKey,
    transition: LeaseTransition,
    sequence: u64,
    label: &str,
    previous: Option<peritus_types::EventId>,
) -> Result<CommittedLeaseTransition, String> {
    let head = store
        .head(key)
        .map_err(|error| format!("load command lease head: {error}"))?
        .map_or(HeadExpectation::Absent(key), HeadExpectation::Present);
    store
        .commit_lease_transition(
            LeaseCommitRequest::new(
                journal::append(
                    ids,
                    store_label,
                    key,
                    &format!("{label}-command"),
                    sequence,
                    &format!("{label}-event"),
                    previous,
                    head,
                )?,
                transition,
            )
            .map_err(|error| format!("bind command lease transition: {error}"))?,
        )
        .map_err(|error| format!("commit command lease transition: {error}"))
}

fn accepted(outcome: LeaseTransitionOutcome) -> Result<LeaseTransition, String> {
    match outcome {
        LeaseTransitionOutcome::Accepted(value) => Ok(value),
        LeaseTransitionOutcome::Rejected(failure) => {
            Err(format!("advance command workspace lease: {:?}", failure.error()))
        }
    }
}
