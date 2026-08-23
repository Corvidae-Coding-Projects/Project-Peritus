//! Exact accepted-output and command-binding oracle.

use super::ReferenceState;
use peritus_leases::{
    AcquireLease, ExpireLease, FenceClockDiscontinuity, FenceHolderLoss, LeaseCommandBinding,
    LeaseCommandBindingKind, LeaseTransition, LeaseTransitionKind, MintLease, ReconcileLease,
    ReleaseLease, RenewLease, RevokeLease,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpectedBinding {
    Mint(MintLease),
    Acquire(AcquireLease),
    Renew(RenewLease),
    Release(ReleaseLease),
    Expire(ExpireLease),
    HolderLoss(FenceHolderLoss),
    ClockDiscontinuity(FenceClockDiscontinuity),
    Revoke(RevokeLease),
    Reconcile(ReconcileLease),
}

pub struct ExpectedTransition {
    before: Option<ReferenceState>,
    after: ReferenceState,
    command_id: peritus_types::CommandId,
    kind: LeaseTransitionKind,
    binding: Box<ExpectedBinding>,
    step: &'static str,
}

impl ExpectedTransition {
    pub const fn new(
        before: Option<ReferenceState>,
        after: ReferenceState,
        command_id: peritus_types::CommandId,
        kind: LeaseTransitionKind,
        binding: Box<ExpectedBinding>,
        step: &'static str,
    ) -> Self {
        Self { before, after, command_id, kind, binding, step }
    }
}

pub fn assert_transition(transition: &LeaseTransition, expected: &ExpectedTransition, seed: u8) {
    assert_record_fields(
        transition,
        expected.before,
        expected.after,
        expected.command_id,
        expected.kind,
        seed,
        expected.step,
    );
    expected.binding.as_ref().assert_matches(transition.record().binding(), seed, expected.step);
    expected.after.assert_refines(transition.next(), seed, expected.step);
}

pub(super) fn assert_record_fields(
    transition: &LeaseTransition,
    before: Option<ReferenceState>,
    after: ReferenceState,
    command_id: peritus_types::CommandId,
    kind: LeaseTransitionKind,
    seed: u8,
    step: &str,
) {
    let record = transition.record();
    assert_eq!(record.command_id(), command_id, "seed {seed} {step}: command id");
    assert_eq!(record.scope(), after.scope(), "seed {seed} {step}: scope");
    assert_eq!(record.after_version(), after.version(), "seed {seed} {step}: after version");
    assert_eq!(
        record.after_generation(),
        after.generation(),
        "seed {seed} {step}: after generation"
    );
    assert_eq!(record.after_phase(), after.phase(), "seed {seed} {step}: after phase");
    assert_eq!(record.kind(), kind, "seed {seed} {step}: kind");
    match before {
        Some(prior) => assert_prior_fields(record, prior, seed, step),
        None => assert_absent_prior_fields(record, seed, step),
    }
}

fn assert_prior_fields(
    record: &peritus_leases::LeaseTransitionRecord,
    prior: ReferenceState,
    seed: u8,
    step: &str,
) {
    assert_eq!(
        record.before_version(),
        Some(prior.version()),
        "seed {seed} {step}: before version"
    );
    assert_eq!(
        record.before_generation(),
        Some(prior.generation()),
        "seed {seed} {step}: before generation"
    );
    assert_eq!(record.before_phase(), Some(prior.phase()), "seed {seed} {step}: before phase");
}

fn assert_absent_prior_fields(
    record: &peritus_leases::LeaseTransitionRecord,
    seed: u8,
    step: &str,
) {
    assert_eq!(record.before_version(), None, "seed {seed} {step}: absent version");
    assert_eq!(record.before_generation(), None, "seed {seed} {step}: absent generation");
    assert_eq!(record.before_phase(), None, "seed {seed} {step}: absent phase");
}

impl ExpectedBinding {
    fn assert_matches(&self, actual: &LeaseCommandBinding, seed: u8, step: &str) {
        let context = format!("seed {seed} {step}: binding");
        match self {
            Self::Mint(value) => {
                assert_eq!(actual.kind(), LeaseCommandBindingKind::Mint, "{context}: family");
                assert_eq!(actual.as_mint().as_ref(), Some(value), "{context}: command");
            }
            Self::Acquire(value) => {
                assert_eq!(actual.kind(), LeaseCommandBindingKind::Acquire, "{context}: family");
                assert_eq!(actual.as_acquire().as_ref(), Some(value), "{context}: command");
            }
            Self::Renew(value) => {
                assert_eq!(actual.kind(), LeaseCommandBindingKind::Renew, "{context}: family");
                assert_eq!(actual.as_renew().as_ref(), Some(value), "{context}: command");
            }
            Self::Release(value) => {
                assert_eq!(actual.kind(), LeaseCommandBindingKind::Release, "{context}: family");
                assert_eq!(actual.as_release().as_ref(), Some(value), "{context}: command");
            }
            Self::Expire(value) => {
                assert_eq!(actual.kind(), LeaseCommandBindingKind::Expire, "{context}: family");
                assert_eq!(actual.as_expire().as_ref(), Some(value), "{context}: command");
            }
            Self::HolderLoss(value) => {
                assert_eq!(actual.kind(), LeaseCommandBindingKind::HolderLoss, "{context}: family");
                assert_eq!(actual.as_holder_loss().as_ref(), Some(value), "{context}: command");
            }
            Self::ClockDiscontinuity(value) => {
                assert_eq!(
                    actual.kind(),
                    LeaseCommandBindingKind::ClockDiscontinuity,
                    "{context}: family"
                );
                assert_eq!(
                    actual.as_clock_discontinuity().as_ref(),
                    Some(value),
                    "{context}: command"
                );
            }
            Self::Revoke(value) => {
                assert_eq!(actual.kind(), LeaseCommandBindingKind::Revoke, "{context}: family");
                assert_eq!(actual.as_revoke().as_ref(), Some(value), "{context}: command");
            }
            Self::Reconcile(value) => {
                assert_eq!(actual.kind(), LeaseCommandBindingKind::Reconcile, "{context}: family");
                assert_eq!(actual.as_reconcile().as_ref(), Some(value), "{context}: command");
            }
        }
    }
}
