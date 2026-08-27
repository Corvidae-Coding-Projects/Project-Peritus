//! Instance, startup, outbox, shutdown, restart, and resource-bound observations.

use super::DaemonReadiness;

/// Direct exclusive-instance ownership facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonInstanceObservation {
    already_running_reported: bool,
    active_identity_matches: bool,
    endpoint_replaced: bool,
    migrations_started: u64,
}

impl DaemonInstanceObservation {
    /// Creates one complete instance observation.
    #[must_use]
    pub const fn new(
        already_running_reported: bool,
        active_identity_matches: bool,
        endpoint_replaced: bool,
        migrations_started: u64,
    ) -> Self {
        Self {
            already_running_reported,
            active_identity_matches,
            endpoint_replaced,
            migrations_started,
        }
    }

    /// Returns whether startup reported the existing live owner.
    #[must_use]
    pub const fn already_running_reported(self) -> bool {
        self.already_running_reported
    }

    /// Returns whether the report retained the exact active instance identity.
    #[must_use]
    pub const fn active_identity_matches(self) -> bool {
        self.active_identity_matches
    }

    /// Returns whether the second instance replaced the active endpoint.
    #[must_use]
    pub const fn endpoint_replaced(self) -> bool {
        self.endpoint_replaced
    }

    /// Returns migrations started by the rejected second instance.
    #[must_use]
    pub const fn migrations_started(self) -> u64 {
        self.migrations_started
    }
}

/// Direct diagnostic-safe startup failure facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonStartupObservation {
    readiness: DaemonReadiness,
    typed_failure_reported: bool,
    effect_workers_started: u64,
    mutations_admitted: u64,
    recovery_rerun_idempotent: bool,
}

impl DaemonStartupObservation {
    /// Creates one complete startup observation.
    #[must_use]
    pub const fn new(
        readiness: DaemonReadiness,
        typed_failure_reported: bool,
        effect_workers_started: u64,
        mutations_admitted: u64,
        recovery_rerun_idempotent: bool,
    ) -> Self {
        Self {
            readiness,
            typed_failure_reported,
            effect_workers_started,
            mutations_admitted,
            recovery_rerun_idempotent,
        }
    }

    /// Returns readiness published after the diagnostic-safe failure.
    #[must_use]
    pub const fn readiness(self) -> DaemonReadiness {
        self.readiness
    }

    /// Returns whether the failure retained a bounded typed category.
    #[must_use]
    pub const fn typed_failure_reported(self) -> bool {
        self.typed_failure_reported
    }

    /// Returns forbidden effect workers started after failure.
    #[must_use]
    pub const fn effect_workers_started(self) -> u64 {
        self.effect_workers_started
    }

    /// Returns mutation requests admitted after failure.
    #[must_use]
    pub const fn mutations_admitted(self) -> u64 {
        self.mutations_admitted
    }

    /// Returns whether rerunning recovery retained the same classification and facts.
    #[must_use]
    pub const fn recovery_rerun_idempotent(self) -> bool {
        self.recovery_rerun_idempotent
    }
}

/// Direct effect-before-ack outbox recovery facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonOutboxObservation {
    destination_reconciled: bool,
    external_effects: u64,
    duplicate_effects: u64,
    exact_fence_acknowledged: bool,
    pending_claims: u64,
}

impl DaemonOutboxObservation {
    /// Creates one complete outbox observation.
    #[must_use]
    pub const fn new(
        destination_reconciled: bool,
        external_effects: u64,
        duplicate_effects: u64,
        exact_fence_acknowledged: bool,
        pending_claims: u64,
    ) -> Self {
        Self {
            destination_reconciled,
            external_effects,
            duplicate_effects,
            exact_fence_acknowledged,
            pending_claims,
        }
    }

    /// Returns whether recovery reconciled the destination identity before retry.
    #[must_use]
    pub const fn destination_reconciled(self) -> bool {
        self.destination_reconciled
    }

    /// Returns total target effects observed across the crash and recovery.
    #[must_use]
    pub const fn external_effects(self) -> u64 {
        self.external_effects
    }

    /// Returns target effects duplicated by recovery.
    #[must_use]
    pub const fn duplicate_effects(self) -> u64 {
        self.duplicate_effects
    }

    /// Returns whether settlement acknowledged the exact live claim fence.
    #[must_use]
    pub const fn exact_fence_acknowledged(self) -> bool {
        self.exact_fence_acknowledged
    }

    /// Returns unsettled claims remaining after recovery.
    #[must_use]
    pub const fn pending_claims(self) -> u64 {
        self.pending_claims
    }
}

/// Stable shutdown completion class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonShutdownOutcome {
    /// Every externally active work item reached an observed terminal result.
    Clean,
    /// Shutdown ended with an exact nonempty remaining-work report.
    Unclean,
}

/// Direct graceful-shutdown facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonShutdownObservation {
    outcome: DaemonShutdownOutcome,
    mutation_intake_closed: bool,
    owned_tasks_joined: bool,
    durable_state_recoverable: bool,
    remaining_work: u64,
}

impl DaemonShutdownObservation {
    /// Creates one complete shutdown observation.
    #[must_use]
    pub const fn new(
        outcome: DaemonShutdownOutcome,
        mutation_intake_closed: bool,
        owned_tasks_joined: bool,
        durable_state_recoverable: bool,
        remaining_work: u64,
    ) -> Self {
        Self {
            outcome,
            mutation_intake_closed,
            owned_tasks_joined,
            durable_state_recoverable,
            remaining_work,
        }
    }

    /// Returns the stable shutdown completion class.
    #[must_use]
    pub const fn outcome(self) -> DaemonShutdownOutcome {
        self.outcome
    }

    /// Returns whether mutation intake closed before draining.
    #[must_use]
    pub const fn mutation_intake_closed(self) -> bool {
        self.mutation_intake_closed
    }

    /// Returns whether every owned task was joined to an observed terminal result.
    #[must_use]
    pub const fn owned_tasks_joined(self) -> bool {
        self.owned_tasks_joined
    }

    /// Returns whether unsettled durable work remains restart-recoverable.
    #[must_use]
    pub const fn durable_state_recoverable(self) -> bool {
        self.durable_state_recoverable
    }

    /// Returns exact active work remaining at shutdown completion.
    #[must_use]
    pub const fn remaining_work(self) -> u64 {
        self.remaining_work
    }
}

/// Direct forced-restart reconciliation facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonRecoveryObservation {
    replay_equivalent: bool,
    durable_work_reconciled: bool,
    duplicate_effects: u64,
    orphaned_authority_work: u64,
    indeterminate_work_reported: bool,
}

impl DaemonRecoveryObservation {
    /// Creates one complete restart observation.
    #[must_use]
    pub const fn new(
        replay_equivalent: bool,
        durable_work_reconciled: bool,
        duplicate_effects: u64,
        orphaned_authority_work: u64,
        indeterminate_work_reported: bool,
    ) -> Self {
        Self {
            replay_equivalent,
            durable_work_reconciled,
            duplicate_effects,
            orphaned_authority_work,
            indeterminate_work_reported,
        }
    }

    /// Returns whether genesis replay reproduced the recovered live state.
    #[must_use]
    pub const fn replay_equivalent(self) -> bool {
        self.replay_equivalent
    }

    /// Returns whether every durable work record received an explicit recovery classification.
    #[must_use]
    pub const fn durable_work_reconciled(self) -> bool {
        self.durable_work_reconciled
    }

    /// Returns effects duplicated during restart.
    #[must_use]
    pub const fn duplicate_effects(self) -> u64 {
        self.duplicate_effects
    }

    /// Returns durable authority work no longer owned or classified after restart.
    #[must_use]
    pub const fn orphaned_authority_work(self) -> u64 {
        self.orphaned_authority_work
    }

    /// Returns whether any genuinely indeterminate work remained explicit.
    #[must_use]
    pub const fn indeterminate_work_reported(self) -> bool {
        self.indeterminate_work_reported
    }
}

/// Direct resource-bound enforcement facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonBoundsObservation {
    oversized_rejected: bool,
    bytes_allocated_before_rejection: u64,
    peak_in_flight: u64,
    retained_items: u64,
}

impl DaemonBoundsObservation {
    /// Creates one complete bound observation.
    #[must_use]
    pub const fn new(
        oversized_rejected: bool,
        bytes_allocated_before_rejection: u64,
        peak_in_flight: u64,
        retained_items: u64,
    ) -> Self {
        Self {
            oversized_rejected,
            bytes_allocated_before_rejection,
            peak_in_flight,
            retained_items,
        }
    }

    /// Returns whether the over-limit request was rejected.
    #[must_use]
    pub const fn oversized_rejected(self) -> bool {
        self.oversized_rejected
    }

    /// Returns payload bytes allocated before over-limit rejection.
    #[must_use]
    pub const fn bytes_allocated_before_rejection(self) -> u64 {
        self.bytes_allocated_before_rejection
    }

    /// Returns peak simultaneous in-flight work.
    #[must_use]
    pub const fn peak_in_flight(self) -> u64 {
        self.peak_in_flight
    }

    /// Returns retained work items after the exercise.
    #[must_use]
    pub const fn retained_items(self) -> u64 {
        self.retained_items
    }
}
