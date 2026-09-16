//! Queue bounds reject hostile inert input before trusting reservation correspondence.

use peritus_codec::{CodecErrorKind, CodecLimits};

use crate::{
    RecoveryPolicy, SchedulerBinding, SchedulerErrorKind, SchedulerState, WorkPhase, WorkRecord,
    WorkSpec, decode_scheduler_state, encode_scheduler_state,
};

fn legacy_state() -> SchedulerState {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fixtures/protocol/scheduler-v1-recovery/retry/checkpoint.bin");
    let bytes = std::fs::read(path).expect("read immutable legacy checkpoint fixture");
    decode_scheduler_state(&bytes, CodecLimits::PRODUCTION)
        .expect("reachable legacy two-waiter checkpoint under Q=1 and A=1")
}

fn record(state: &SchedulerState, phase: WorkPhase, recovery: RecoveryPolicy) -> WorkRecord {
    let original = state.work()[0].spec();
    let spec = WorkSpec::new(
        original.id(),
        original.owner(),
        original.revision(),
        original.class(),
        original.priority(),
        original.request().clone(),
        original.budget_reservation(),
        original.dependencies().to_vec(),
        original.parent(),
        original.maximum_attempts(),
        recovery,
        original.payload_digest(),
        state.binding().limits(),
    )
    .expect("same immutable work definition with an explicit recovery policy");
    WorkRecord::from_wire(
        spec,
        phase,
        state.work()[0].enqueue_ordinal(),
        0,
        1,
        if phase == WorkPhase::RetryPending {
            Some(peritus_types::Sha256Digest::new([70; 32]))
        } else {
            None
        },
        None,
    )
}

fn rejects_queue_bound(mut state: SchedulerState) {
    // Keep the digest consistent so a later hash check cannot mask an omitted queue check.
    state.state_digest = crate::canonical::state_digest(&state);
    assert!(!super::within_bounds(&state));
    let error = state.validate_inert().expect_err("inert queue bound must fail first");
    assert_eq!(error.kind(), SchedulerErrorKind::LimitExceeded);
    assert_eq!(error.detail(), "decoded scheduler state exceeds immutable bounds");
    let encoded = encode_scheduler_state(&state, CodecLimits::PRODUCTION)
        .expect("inert values can be serialized for hostile decoder input");
    assert_eq!(
        decode_scheduler_state(&encoded, CodecLimits::PRODUCTION)
            .expect_err("actual checkpoint decoder must enforce its semantic queue bound")
            .kind(),
        CodecErrorKind::InvalidDomainValue
    );
}

#[test]
fn strict_decoder_counts_each_waiting_and_recoverable_active_category_under_every_policy() {
    for phase in [
        WorkPhase::Queued,
        WorkPhase::WaitingDependencies,
        WorkPhase::RetryPending,
        WorkPhase::Reserved,
        WorkPhase::Running,
    ] {
        for policy in [RecoveryPolicy::RetrySafe, RecoveryPolicy::Ambiguous, RecoveryPolicy::Fail] {
            let mut state = legacy_state();
            let binding = state.binding();
            state.binding = SchedulerBinding::new(
                binding.run_id(),
                binding.scheduler_id(),
                binding.revision(),
                binding.limits(),
                binding.capacity().clone(),
            )
            .expect("strict binding with identical Q=1 and A=1");
            state.work[0] = record(&state, phase, policy);
            // No reservation is supplied for hostile active phases. The direct work scan
            // must reject excess pressure independently of later ownership validation.
            assert!(state.reservations().is_empty());
            rejects_queue_bound(state);
        }
    }
}

#[test]
fn legacy_decoder_counts_active_ownership_even_when_reservations_are_missing() {
    for phase in [WorkPhase::Reserved, WorkPhase::Running, WorkPhase::Cancelling] {
        let mut state = legacy_state();
        let active = record(&state, phase, RecoveryPolicy::RetrySafe);
        state.work.push(active);
        state.enqueue_ordinal += 1;
        assert!(state.reservations().is_empty());
        // W=2 still satisfies the old weaker W<=Q+A check. W+Awork=3 exceeds 2.
        // Duplicate identities are also hostile, but the queue guard must act before
        // relying on identity, reservation, or phase correspondence.
        rejects_queue_bound(state);
    }
}
