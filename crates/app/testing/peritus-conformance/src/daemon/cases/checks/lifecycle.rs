//! Instance, startup, outbox, shutdown, restart, and resource-bound checks.

use super::super::super::{
    DaemonConformanceFixture, DaemonConformanceObservation, DaemonReadiness, DaemonScenario,
    DaemonShutdownOutcome,
};

pub(super) fn matches(
    fixture: &DaemonConformanceFixture,
    observed: &DaemonConformanceObservation,
) -> bool {
    use DaemonConformanceObservation as O;
    use DaemonScenario as D;
    match (fixture.scenario(), observed) {
        (D::SecondInstance, O::Instance(value)) => {
            value.already_running_reported()
                && value.active_identity_matches()
                && !value.endpoint_replaced()
                && value.migrations_started() == 0
        }
        (D::StartupFailure, O::Startup(value)) => {
            value.readiness() == DaemonReadiness::ReadyReadOnly
                && value.typed_failure_reported()
                && value.effect_workers_started() == 0
                && value.mutations_admitted() == 0
                && value.recovery_rerun_idempotent()
        }
        (D::OutboxCrash, O::Outbox(value)) => {
            value.destination_reconciled()
                && value.external_effects() == 1
                && value.duplicate_effects() == 0
                && value.exact_fence_acknowledged()
                && value.pending_claims() == 0
        }
        (D::GracefulShutdown, O::Shutdown(value)) => {
            value.outcome() == DaemonShutdownOutcome::Clean
                && value.mutation_intake_closed()
                && value.owned_tasks_joined()
                && value.durable_state_recoverable()
                && value.remaining_work() == 0
        }
        (D::ForcedRestart, O::Recovery(value)) => {
            value.replay_equivalent()
                && value.durable_work_reconciled()
                && value.duplicate_effects() == 0
                && value.orphaned_authority_work() == 0
        }
        (D::Bounds, O::Bounds(value)) => {
            value.oversized_rejected()
                && value.bytes_allocated_before_rejection() == 0
                && value.peak_in_flight() <= fixture.maximum_in_flight()
                && value.retained_items() <= fixture.maximum_in_flight()
        }
        _ => false,
    }
}
