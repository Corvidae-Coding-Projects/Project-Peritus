//! Exact independent oracle for accepted representation-boundary retirement outputs.

use crate::{
    ExpireLease, LeaseAggregate, LeaseCommandBindingKind, LeasePhase, LeaseScope, LeaseTransition,
    LeaseTransitionKind, ReconcileLease, RetirementReason,
};
use peritus_types::{CommandId, Generation, RevisionNumber};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RetirementBinding {
    Expire(ExpireLease),
    Reconcile(ReconcileLease),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RetirementTransitionReference {
    pub scope: LeaseScope,
    pub before_generation: Generation,
    pub before_version: RevisionNumber,
    pub before_phase: LeasePhase,
    pub command_id: CommandId,
    pub after_generation: Generation,
    pub after_version: RevisionNumber,
    pub authority_epoch: Generation,
    pub authority_tick: u64,
    pub reason: RetirementReason,
    pub binding: RetirementBinding,
}

impl RetirementTransitionReference {
    pub fn assert_matches(self, transition: &LeaseTransition, seed: u8, step: &str) {
        let record = transition.record();
        assert_eq!(record.command_id(), self.command_id, "seed {seed} {step}: command");
        assert_eq!(record.scope(), self.scope, "seed {seed} {step}: record scope");
        assert_eq!(
            record.before_generation(),
            Some(self.before_generation),
            "seed {seed} {step}: before generation"
        );
        assert_eq!(
            record.before_version(),
            Some(self.before_version),
            "seed {seed} {step}: before version"
        );
        assert_eq!(
            record.before_phase(),
            Some(self.before_phase),
            "seed {seed} {step}: before phase"
        );
        assert_eq!(record.after_generation(), self.after_generation, "seed {seed} {step}: after generation");
        assert_eq!(record.after_version(), self.after_version, "seed {seed} {step}: after version");
        assert_eq!(record.after_phase(), LeasePhase::Retired, "seed {seed} {step}: after phase");
        assert_eq!(record.kind(), LeaseTransitionKind::Retired(self.reason), "seed {seed} {step}: kind");
        self.binding.assert_matches(record, seed, step);
        self.assert_state(transition.next(), seed, step);
    }

    fn assert_state(self, aggregate: &LeaseAggregate, seed: u8, step: &str) {
        assert_eq!(aggregate.scope(), self.scope, "seed {seed} {step}: scope");
        assert_eq!(aggregate.generation(), self.after_generation, "seed {seed} {step}: generation");
        assert_eq!(aggregate.version(), self.after_version, "seed {seed} {step}: version");
        assert_eq!(aggregate.authority_time().epoch(), self.authority_epoch, "seed {seed} {step}: epoch");
        assert_eq!(aggregate.authority_time().greatest_tick_millis(), self.authority_tick, "seed {seed} {step}: tick");
        assert_eq!(aggregate.phase(), LeasePhase::Retired, "seed {seed} {step}: phase");
        assert!(aggregate.active().is_none(), "seed {seed} {step}: active");
        assert!(aggregate.reconciliation().is_none(), "seed {seed} {step}: reconciliation");
        assert!(aggregate.quarantine().is_none(), "seed {seed} {step}: quarantine");
        assert_eq!(aggregate.retirement_reason(), Some(self.reason), "seed {seed} {step}: reason");
    }
}

impl RetirementBinding {
    fn assert_matches(
        self,
        record: &crate::LeaseTransitionRecord,
        seed: u8,
        step: &str,
    ) {
        match self {
            Self::Expire(command) => {
                assert_eq!(record.binding().kind(), LeaseCommandBindingKind::Expire, "seed {seed} {step}: binding kind");
                assert_eq!(record.binding().as_expire(), Some(command), "seed {seed} {step}: binding");
            }
            Self::Reconcile(command) => {
                assert_eq!(record.binding().kind(), LeaseCommandBindingKind::Reconcile, "seed {seed} {step}: binding kind");
                assert_eq!(record.binding().as_reconcile(), Some(command), "seed {seed} {step}: binding");
            }
        }
    }
}
