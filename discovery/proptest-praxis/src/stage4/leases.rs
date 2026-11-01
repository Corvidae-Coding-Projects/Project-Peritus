//! Versioned lease histories against an independent table; durable commits belong to C0.

use peritus_leases::{
    AcquireLease, ExpireLease, FenceClockDiscontinuity, FenceHolderLoss, HolderLossEvidence,
    HolderQuiescenceEvidence, LeaseAggregate, LeaseClaim, LeaseDuration, LeaseHolder, LeasePhase,
    LeaseScope, LeaseTransitionOutcome, MintLease, ReconcileLease, ReconciliationCorrelation,
    ReconciliationDisposition, ReconciliationObservation, ReleaseLease, RenewLease, RevokeLease,
};
use peritus_policy::AuthorityInstant;
use peritus_types::{
    ActorId, CommandId, EnvironmentId, EvidenceId, Generation, ResourceId, SessionId, WorkspaceId,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Acquire,
    Renew,
    Release,
    Expire,
    HolderLost,
    Revoke,
    ClockFence,
    Reconcile,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Time {
    Floor,
    Later,
    Earlier,
    Expiry,
    AfterExpiry,
    NewEpoch,
    Overflow,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Operation {
    pub kind: Kind,
    pub time: Time,
    pub duration: u8,
    pub claim: u8,
    pub holder: u8,
    pub evidence: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub operations: Vec<Operation>,
}

pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let operation = (
        prop_oneof![
            Just(Kind::Acquire),
            Just(Kind::Renew),
            Just(Kind::Release),
            Just(Kind::Expire),
            Just(Kind::HolderLost),
            Just(Kind::Revoke),
            Just(Kind::ClockFence),
            Just(Kind::Reconcile)
        ],
        prop_oneof![
            Just(Time::Floor),
            Just(Time::Later),
            Just(Time::Earlier),
            Just(Time::Expiry),
            Just(Time::AfterExpiry),
            Just(Time::NewEpoch),
            Just(Time::Overflow)
        ],
        1_u8..=32,
        any::<u8>(),
        0_u8..=2,
        0_u8..=3,
    )
        .prop_map(|(kind, time, duration, claim, holder, evidence)| Operation {
            kind,
            time,
            duration,
            claim,
            holder,
            evidence,
        });
    let general = proptest::collection::vec(operation.clone(), 0..=max_steps.min(128))
        .prop_map(|operations| Case { operations })
        .boxed();
    if max_steps < 6 {
        return general;
    }
    let warm =
        proptest::collection::vec(operation, 0..=max_steps.min(128) - 6).prop_map(|suffix| {
            let mut operations = vec![
                op(Kind::Renew, Time::Later, 0),
                op(Kind::Release, Time::Floor, 1),
                op(Kind::Acquire, Time::Floor, 0),
                op(Kind::Expire, Time::Expiry, 0),
                op(Kind::Reconcile, Time::Floor, 0),
                op(Kind::Acquire, Time::Later, 0),
            ];
            operations.extend(suffix);
            Case { operations }
        });
    prop_oneof![2=>general,1=>warm].boxed()
}
fn op(kind: Kind, time: Time, evidence: u8) -> Operation {
    Operation { kind, time, evidence, duration: 20, claim: 255, holder: 1 }
}
pub fn calibration() -> Case {
    Case { operations: vec![op(Kind::Release, Time::Floor, 1)] }
}
pub fn examples() -> Vec<Case> {
    vec![
        calibration(),
        Case {
            operations: vec![
                op(Kind::Renew, Time::Later, 0),
                Operation { claim: 0, ..op(Kind::Renew, Time::Later, 0) },
                op(Kind::Expire, Time::Expiry, 0),
                op(Kind::Reconcile, Time::Floor, 3),
                op(Kind::Reconcile, Time::Floor, 0),
                op(Kind::Acquire, Time::Later, 0),
            ],
        },
        Case {
            operations: vec![
                op(Kind::ClockFence, Time::Earlier, 0),
                op(Kind::Reconcile, Time::Floor, 0),
                op(Kind::Acquire, Time::NewEpoch, 0),
                op(Kind::Acquire, Time::Later, 0),
            ],
        },
        Case {
            operations: vec![
                op(Kind::ClockFence, Time::NewEpoch, 0),
                op(Kind::Reconcile, Time::Floor, 1),
                op(Kind::Acquire, Time::Later, 0),
            ],
        },
        Case {
            operations: vec![
                op(Kind::HolderLost, Time::Later, 0),
                op(Kind::Reconcile, Time::Floor, 2),
            ],
        },
        Case {
            operations: vec![
                op(Kind::Release, Time::Overflow, 1),
                op(Kind::Acquire, Time::Floor, 0),
            ],
        },
    ]
}
fn bytes(seed: u64) -> [u8; 16] {
    let mut id = [91; 16];
    id[..8].copy_from_slice(&seed.to_be_bytes());
    id
}
fn instant(epoch: u64, tick: u64) -> Result<AuthorityInstant, String> {
    Ok(AuthorityInstant::new(Generation::new(epoch).map_err(err)?, tick))
}
fn err(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}
fn holder(value: u8) -> Result<LeaseHolder, String> {
    Ok(LeaseHolder::new(
        ActorId::new(bytes(u64::from(value) + 10)).map_err(err)?,
        SessionId::new(bytes(u64::from(value) + 20)).map_err(err)?,
    ))
}
fn scope() -> Result<LeaseScope, String> {
    Ok(LeaseScope::new(
        WorkspaceId::new(bytes(1)).map_err(err)?,
        ResourceId::new(bytes(2)).map_err(err)?,
        EnvironmentId::new(bytes(3)).map_err(err)?,
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Claim {
    holder: u8,
    generation: u64,
    version: u64,
    epoch: u64,
    issued: u64,
    expires: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Available,
    Active(Claim),
    Reconciling { holder: u8, generation: u64, clock: bool },
    Quarantined,
}
#[derive(Clone, Debug)]
struct Model {
    phase: Phase,
    generation: u64,
    version: u64,
    epoch: u64,
    floor: u64,
}
impl Model {
    fn observation(&self, time: Time) -> (u64, u64) {
        let expiry = match self.phase {
            Phase::Active(claim) => claim.expires,
            _ => self.floor,
        };
        match time {
            Time::Floor => (self.epoch, self.floor),
            Time::Later => (self.epoch, self.floor.saturating_add(1)),
            Time::Earlier => (self.epoch, self.floor.saturating_sub(1)),
            Time::Expiry => (self.epoch, expiry),
            Time::AfterExpiry => (self.epoch, expiry.saturating_add(1)),
            Time::NewEpoch => (self.epoch + 1, 1),
            Time::Overflow => (self.epoch, u64::MAX),
        }
    }
    fn apply(
        &mut self,
        op: &Operation,
        claim: Claim,
        quiescence: Option<Claim>,
        epoch: u64,
        tick: u64,
    ) -> bool {
        let ordinary = epoch == self.epoch && tick >= self.floor;
        let active = if let Phase::Active(current) = self.phase { Some(current) } else { None };
        let exact = active == Some(claim);
        let mut fence = None;
        match op.kind {
            Kind::Acquire => {
                if self.phase != Phase::Available || !ordinary {
                    return false;
                }
                let Some(expires) = tick.checked_add(u64::from(op.duration)) else {
                    return false;
                };
                self.phase = Phase::Active(Claim {
                    holder: op.holder,
                    generation: self.generation,
                    version: 1,
                    epoch,
                    issued: tick,
                    expires,
                });
            }
            Kind::Renew => {
                if !exact || !ordinary || tick >= claim.expires {
                    return false;
                }
                let Some(expires) = tick.checked_add(u64::from(op.duration)) else {
                    return false;
                };
                if expires <= claim.expires {
                    return false;
                }
                self.phase = Phase::Active(Claim {
                    issued: tick,
                    expires,
                    version: claim.version + 1,
                    ..claim
                });
            }
            Kind::Release => {
                if !exact || !ordinary || quiescence.is_some_and(|value| value != claim) {
                    return false;
                }
                fence = Some((claim, quiescence.is_none(), false));
            }
            Kind::HolderLost | Kind::Revoke => {
                if !exact || !ordinary {
                    return false;
                }
                fence = Some((claim, true, false));
            }
            Kind::Expire => {
                let Some(active) = active else {
                    return false;
                };
                if !ordinary || tick < active.expires {
                    return false;
                }
                fence = Some((active, true, false));
            }
            Kind::ClockFence => {
                let Some(active) = active else {
                    return false;
                };
                if ordinary {
                    return false;
                }
                fence = Some((active, true, true));
            }
            Kind::Reconcile => {
                let Phase::Reconciling { clock, .. } = self.phase else {
                    return false;
                };
                if op.evidence == 3 || (!ordinary && !(clock && epoch != self.epoch)) {
                    return false;
                }
                self.phase = if op.evidence == 0 { Phase::Available } else { Phase::Quarantined };
            }
        }
        if let Some((prior, reconciling, clock)) = fence {
            self.generation += 1;
            self.phase = if reconciling {
                Phase::Reconciling { holder: prior.holder, generation: prior.generation, clock }
            } else {
                Phase::Available
            };
            if clock && epoch == self.epoch {
                self.version += 1;
                return true;
            }
        }
        self.version += 1;
        self.epoch = epoch;
        self.floor = tick;
        true
    }
}

fn compare(actual: &LeaseAggregate, model: &Model) -> Result<(), String> {
    let phase = match model.phase {
        Phase::Available => LeasePhase::Available,
        Phase::Active(_) => LeasePhase::Active,
        Phase::Reconciling { .. } => LeasePhase::Reconciling,
        Phase::Quarantined => LeasePhase::Quarantined,
    };
    if actual.scope() != scope()?
        || actual.phase() != phase
        || actual.generation().get() != model.generation
        || actual.version().get() != model.version
        || actual.authority_time().epoch().get() != model.epoch
        || actual.authority_time().greatest_tick_millis() != model.floor
    {
        return Err(format!("lease table mismatch: expected={model:?},actual={actual:?}"));
    }
    if let Phase::Active(wanted) = model.phase {
        let claim = actual.active().ok_or("active claim missing")?.claim();
        if claim.holder() != holder(wanted.holder)?
            || claim.generation().get() != wanted.generation
            || claim.claim_version().get() != wanted.version
            || claim.issued_at() != instant(wanted.epoch, wanted.issued)?
            || claim.expires_at() != instant(wanted.epoch, wanted.expires)?
        {
            return Err("exact active lease claim drift".into());
        }
    }
    if let Phase::Reconciling { holder: prior, generation, .. } = model.phase {
        let correlation =
            actual.reconciliation().ok_or("reconciliation correlation missing")?.correlation();
        if correlation.scope() != scope()?
            || correlation.prior_holder() != holder(prior)?
            || correlation.fenced_generation().get() != generation
        {
            return Err("fenced generation correlation drift".into());
        }
    }
    Ok(())
}

pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    if case.operations.len() > 128 || case.operations.iter().any(|op| op.duration == 0) {
        return Err("invalid lease fixture capacity".into());
    }
    let mint = LeaseAggregate::mint(MintLease::new(
        CommandId::new(bytes(100)).map_err(err)?,
        scope()?,
        instant(1, 10)?,
    ))
    .map_err(err)?;
    let mut actual = mint.into_next();
    let mut model =
        Model { phase: Phase::Available, generation: 1, version: 1, epoch: 1, floor: 10 };
    compare(&actual, &model)?;
    let initial = Operation {
        kind: Kind::Acquire,
        time: Time::Floor,
        duration: 20,
        claim: 0,
        holder: 0,
        evidence: 0,
    };
    let mut claims = Vec::<LeaseClaim>::new();
    let mut model_claims = Vec::<Claim>::new();
    let mut accepted = 0;
    let mut rejected = 0;
    let mut fences = 0;
    let mut stale = 0;
    let mut recoveries = 0;
    for (index, op) in std::iter::once(&initial).chain(case.operations.iter()).enumerate() {
        let (epoch, tick) = model.observation(op.time);
        let now = instant(epoch, tick)?;
        let selected =
            if claims.is_empty() { None } else { Some(usize::from(op.claim) % claims.len()) };
        let claim = selected.map(|index| claims[index]);
        let mc = selected.map(|index| model_claims[index]).unwrap_or(Claim {
            holder: 0,
            generation: 0,
            version: 0,
            epoch: 1,
            issued: 0,
            expires: 0,
        });
        let quiescence = if op.evidence == 1 {
            Some(mc)
        } else if op.evidence >= 2 {
            model_claims.first().copied()
        } else {
            None
        };
        let qclaim = if op.evidence == 1 {
            claim
        } else if op.evidence >= 2 {
            claims.first().copied()
        } else {
            None
        };
        let before = format!("{actual:?}");
        let mut next = model.clone();
        let expected = next.apply(op, mc, quiescence, epoch, tick);
        let command =
            CommandId::new(bytes(u64::try_from(index).map_err(err)? + 101)).map_err(err)?;
        let evidence = EvidenceId::new(bytes(40)).map_err(err)?;
        let outcome = match op.kind {
            Kind::Acquire => actual.acquire(AcquireLease::new(
                command,
                holder(op.holder)?,
                LeaseDuration::new(u64::from(op.duration)).map_err(err)?,
                now,
            )),
            Kind::Renew => actual.renew(RenewLease::new(
                command,
                claim.ok_or("missing generated claim")?,
                LeaseDuration::new(u64::from(op.duration)).map_err(err)?,
                now,
            )),
            Kind::Release => actual.release(ReleaseLease::new(
                command,
                claim.ok_or("missing generated claim")?,
                now,
                qclaim.map(|claim| HolderQuiescenceEvidence::new(claim, evidence)),
            )),
            Kind::Expire => actual.expire(ExpireLease::new(command, now)),
            Kind::HolderLost => actual.fence_holder_loss(FenceHolderLoss::new(
                command,
                now,
                HolderLossEvidence::new(claim.ok_or("missing generated claim")?, evidence),
            )),
            Kind::Revoke => actual.revoke(RevokeLease::new(
                command,
                claim.ok_or("missing generated claim")?,
                now,
                evidence,
            )),
            Kind::ClockFence => {
                actual.fence_clock_discontinuity(FenceClockDiscontinuity::new(command, now))
            }
            Kind::Reconcile => {
                let (prior, generation) = match model.phase {
                    Phase::Reconciling { holder, generation, .. } => (holder, generation),
                    _ => (0, 1),
                };
                let correlation = ReconciliationCorrelation::new(
                    scope()?,
                    Generation::new(generation).map_err(err)?,
                    holder(if op.evidence == 3 { prior.wrapping_add(1) } else { prior })?,
                );
                let disposition = match op.evidence {
                    0 | 3 => ReconciliationDisposition::SafeToAcquire {
                        holder_quiescence: evidence,
                        resource_safety: evidence,
                    },
                    1 => ReconciliationDisposition::Dirty { evidence_id: evidence },
                    _ => ReconciliationDisposition::Indeterminate { evidence_id: evidence },
                };
                actual.reconcile(ReconcileLease::new(
                    command,
                    now,
                    ReconciliationObservation::new(correlation, disposition),
                ))
            }
        };
        actual = match outcome {
            LeaseTransitionOutcome::Accepted(value) => {
                if !expected {
                    return Err(format!(
                        "lease accepted forbidden operation {index}: {op:?}; model={model:?}"
                    ));
                }
                if value.record().before_version().map(|v| v.get()) != Some(model.version)
                    || value.record().after_version().get() != next.version
                    || value.record().after_generation().get() != next.generation
                {
                    return Err("lease transition CAS record drift".into());
                }
                accepted += 1;
                fences += u64::from(next.generation != model.generation);
                recoveries += u64::from(matches!(op.kind, Kind::Reconcile));
                model = next;
                value.into_next()
            }
            LeaseTransitionOutcome::Rejected(failure) => {
                if expected {
                    return Err(format!(
                        "lease rejected permitted operation {index}: {op:?}; {:?}",
                        failure.error()
                    ));
                }
                rejected += 1;
                stale += u64::from(
                    matches!(
                        op.kind,
                        Kind::Renew | Kind::Release | Kind::HolderLost | Kind::Revoke
                    ) && matches!(model.phase,Phase::Active(current)if current!=mc),
                );
                let preserved = failure.into_aggregate();
                if format!("{preserved:?}") != before {
                    return Err("rejected lease operation changed linear aggregate".into());
                }
                preserved
            }
        };
        compare(&actual, &model)?;
        if matches!(op.kind, Kind::Acquire | Kind::Renew) && expected {
            claims.push(actual.active().ok_or("accepted active claim missing")?.claim());
            if let Phase::Active(claim) = model.phase {
                model_claims.push(claim);
            }
        }
    }
    Ok(crate::Metrics::from([
        ("accepted".into(), accepted),
        ("rejected".into(), rejected),
        ("fences".into(), fences),
        ("stale_claim_rejections".into(), stale),
        ("reconciliations".into(), recoveries),
        ("operations".into(), case.operations.len() as u64 + 1),
    ]))
}
