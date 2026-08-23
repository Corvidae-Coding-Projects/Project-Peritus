//! Full independent aggregate-state oracle.

use peritus_leases::{
    FenceCause, LeaseAggregate, LeaseHolder, LeasePhase, LeaseScope, ReconciliationCorrelation,
    ReconciliationDisposition, RetirementReason,
};
use peritus_policy::AuthorityInstant;
use peritus_types::{Generation, RevisionNumber};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReferenceDetail {
    Available,
    Active {
        holder: LeaseHolder,
        claim_version: RevisionNumber,
        issued_at: AuthorityInstant,
        expires_at: AuthorityInstant,
    },
    Reconciling {
        correlation: ReconciliationCorrelation,
        cause: FenceCause,
    },
    Quarantined {
        correlation: ReconciliationCorrelation,
        cause: FenceCause,
        disposition: ReconciliationDisposition,
    },
}

/// Every authoritative aggregate field, computed without production reducer/model helpers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReferenceState {
    scope: LeaseScope,
    generation: Generation,
    version: RevisionNumber,
    epoch: Generation,
    floor_tick: u64,
    detail: ReferenceDetail,
}

impl ReferenceState {
    pub const fn minted(scope: LeaseScope, observed_at: AuthorityInstant) -> Self {
        Self {
            scope,
            generation: Generation::first(),
            version: RevisionNumber::first(),
            epoch: observed_at.epoch(),
            floor_tick: observed_at.tick_millis(),
            detail: ReferenceDetail::Available,
        }
    }

    pub fn active(scope: LeaseScope, holder: LeaseHolder) -> Self {
        Self::minted(scope, instant(10)).after_acquire(holder, instant(10), 50)
    }

    pub fn after_acquire(
        self,
        holder: LeaseHolder,
        observed_at: AuthorityInstant,
        duration: u64,
    ) -> Self {
        Self {
            version: next_version(self.version),
            epoch: observed_at.epoch(),
            floor_tick: observed_at.tick_millis(),
            detail: ReferenceDetail::Active {
                holder,
                claim_version: RevisionNumber::first(),
                issued_at: observed_at,
                expires_at: AuthorityInstant::new(
                    observed_at.epoch(),
                    observed_at.tick_millis() + duration,
                ),
            },
            ..self
        }
    }

    pub fn after_renew(self, observed_at: AuthorityInstant, duration: u64) -> Self {
        let ReferenceDetail::Active { holder, claim_version, .. } = self.detail else {
            panic!("reference renewal requires active state")
        };
        Self {
            version: next_version(self.version),
            epoch: observed_at.epoch(),
            floor_tick: observed_at.tick_millis(),
            detail: ReferenceDetail::Active {
                holder,
                claim_version: next_version(claim_version),
                issued_at: observed_at,
                expires_at: AuthorityInstant::new(
                    observed_at.epoch(),
                    observed_at.tick_millis() + duration,
                ),
            },
            ..self
        }
    }

    pub fn after_use(self, observed_at: AuthorityInstant) -> Self {
        Self {
            version: next_version(self.version),
            epoch: observed_at.epoch(),
            floor_tick: observed_at.tick_millis(),
            ..self
        }
    }

    pub fn after_fence(
        self,
        direct_available: bool,
        observed_at: AuthorityInstant,
        cause: FenceCause,
    ) -> Self {
        let ReferenceDetail::Active { holder, .. } = self.detail else {
            panic!("reference fencing requires active state")
        };
        let generation = next_generation(self.generation);
        let detail = if direct_available {
            ReferenceDetail::Available
        } else {
            ReferenceDetail::Reconciling {
                correlation: ReconciliationCorrelation::new(self.scope, self.generation, holder),
                cause,
            }
        };
        let (epoch, floor_tick) =
            if cause == FenceCause::ClockDiscontinuity && observed_at.epoch() == self.epoch {
                (self.epoch, self.floor_tick)
            } else {
                (observed_at.epoch(), observed_at.tick_millis())
            };
        Self { generation, version: next_version(self.version), epoch, floor_tick, detail, ..self }
    }

    pub fn after_reconcile(
        self,
        observed_at: AuthorityInstant,
        disposition: ReconciliationDisposition,
    ) -> Self {
        let ReferenceDetail::Reconciling { correlation, cause } = self.detail else {
            panic!("reference reconciliation requires reconciling state")
        };
        let detail = match disposition {
            ReconciliationDisposition::SafeToAcquire { .. } => ReferenceDetail::Available,
            unsafe_disposition => {
                ReferenceDetail::Quarantined { correlation, cause, disposition: unsafe_disposition }
            }
        };
        let (epoch, floor_tick) = if cause == FenceCause::ClockDiscontinuity {
            (self.epoch, self.floor_tick)
        } else {
            (observed_at.epoch(), observed_at.tick_millis())
        };
        Self { version: next_version(self.version), epoch, floor_tick, detail, ..self }
    }

    pub const fn scope(self) -> LeaseScope {
        self.scope
    }
    pub const fn generation(self) -> Generation {
        self.generation
    }
    pub const fn version(self) -> RevisionNumber {
        self.version
    }

    pub const fn phase(self) -> LeasePhase {
        match self.detail {
            ReferenceDetail::Available => LeasePhase::Available,
            ReferenceDetail::Active { .. } => LeasePhase::Active,
            ReferenceDetail::Reconciling { .. } => LeasePhase::Reconciling,
            ReferenceDetail::Quarantined { .. } => LeasePhase::Quarantined,
        }
    }

    pub fn assert_refines(self, aggregate: &LeaseAggregate, seed: u8, step: &str) {
        let context = || format!("seed {seed} step {step}");
        assert_eq!(aggregate.scope(), self.scope, "{}: scope", context());
        assert_eq!(aggregate.generation(), self.generation, "{}: generation", context());
        assert_eq!(aggregate.version(), self.version, "{}: version", context());
        assert_eq!(aggregate.authority_time().epoch(), self.epoch, "{}: epoch", context());
        assert_eq!(
            aggregate.authority_time().greatest_tick_millis(),
            self.floor_tick,
            "{}: time floor",
            context()
        );
        assert_eq!(aggregate.phase(), self.phase(), "{}: phase", context());
        self.assert_detail(aggregate, &context());
    }

    fn assert_detail(self, aggregate: &LeaseAggregate, context: &str) {
        match self.detail {
            ReferenceDetail::Available => assert_empty_detail(aggregate, context, None),
            ReferenceDetail::Active { holder, claim_version, issued_at, expires_at } => {
                let claim = aggregate.active().expect("expected active state").claim();
                assert_eq!(claim.scope(), self.scope, "{context}: active scope");
                assert_eq!(claim.holder(), holder, "{context}: holder");
                assert_eq!(claim.generation(), self.generation, "{context}: claim generation");
                assert_eq!(claim.claim_version(), claim_version, "{context}: claim version");
                assert_eq!(claim.issued_at(), issued_at, "{context}: issued at");
                assert_eq!(claim.expires_at(), expires_at, "{context}: expires at");
                assert!(aggregate.reconciliation().is_none(), "{context}: reconciliation");
                assert!(aggregate.quarantine().is_none(), "{context}: quarantine");
                assert!(aggregate.retirement_reason().is_none(), "{context}: retirement");
            }
            ReferenceDetail::Reconciling { correlation, cause } => {
                let actual = aggregate.reconciliation().expect("expected reconciliation");
                assert_eq!(actual.correlation(), correlation, "{context}: correlation");
                assert_eq!(actual.cause(), cause, "{context}: cause");
                assert_empty_non_reconciliation(aggregate, context);
            }
            ReferenceDetail::Quarantined { correlation, cause, disposition } => {
                let actual = aggregate.quarantine().expect("expected quarantine");
                assert_eq!(actual.correlation(), correlation, "{context}: correlation");
                assert_eq!(actual.cause(), cause, "{context}: cause");
                assert_eq!(actual.disposition(), disposition, "{context}: disposition");
                assert!(aggregate.active().is_none(), "{context}: active");
                assert!(aggregate.reconciliation().is_none(), "{context}: reconciliation");
                assert!(aggregate.retirement_reason().is_none(), "{context}: retirement");
            }
        }
    }
}

fn assert_empty_detail(
    aggregate: &LeaseAggregate,
    context: &str,
    retirement: Option<RetirementReason>,
) {
    assert!(aggregate.active().is_none(), "{context}: active");
    assert!(aggregate.reconciliation().is_none(), "{context}: reconciliation");
    assert!(aggregate.quarantine().is_none(), "{context}: quarantine");
    assert_eq!(aggregate.retirement_reason(), retirement, "{context}: retirement");
}

fn assert_empty_non_reconciliation(aggregate: &LeaseAggregate, context: &str) {
    assert!(aggregate.active().is_none(), "{context}: active");
    assert!(aggregate.quarantine().is_none(), "{context}: quarantine");
    assert!(aggregate.retirement_reason().is_none(), "{context}: retirement");
}

fn next_generation(value: Generation) -> Generation {
    Generation::new(value.get() + 1).expect("generated reference generation")
}

fn next_version(value: RevisionNumber) -> RevisionNumber {
    RevisionNumber::new(value.get() + 1).expect("generated reference version")
}

const fn instant(tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::first(), tick)
}
