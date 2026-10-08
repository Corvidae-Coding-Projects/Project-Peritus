//! Deterministic crash-recovery decisions from durable observations.

use peritus_journal::OutboxDeliveryStatus;
use peritus_types::EvidenceId;

use crate::{
    EvaluationDirectiveDelivery, EvaluationPhase, EvaluationState, ExecutionDirectiveKind,
    PublicationDirectiveDelivery, PublicationRecord, ReportRecord, RetryIntent, RolloutId,
    RolloutStatus, ScheduleDirectiveDelivery, ScheduleDirectiveKind,
};

use super::PublicationOwnershipReceipt;

/// Exact external facts observed during recovery; no field grants mutation authority.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent artifact, evidence, publication, and conflict observations remain explicit"
)]
pub struct RecoveryObservation {
    /// Number of outstanding schedule outbox rows.
    pub schedule_directives: u32,
    /// Number of outstanding execution outbox rows.
    pub execution_directives: u32,
    /// Whether the exact publication directive remains outstanding.
    pub publication_directive: bool,
    /// Whether the report artifact is finalized and verified.
    pub report_artifact_verified: bool,
    /// Whether exact report evidence is already admitted.
    pub report_evidence_admitted: bool,
    /// Whether external owners report an irreconcilable identity conflict.
    pub identity_conflict: bool,
}

/// Identity-bound retained directive observations for exact recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent artifact, evidence, and conflict observations remain explicit"
)]
pub struct DeliveryRecoveryObservation<'a> {
    deliveries: &'a [EvaluationDirectiveDelivery],
    report_artifact_verified: bool,
    report_evidence_admitted: bool,
    identity_conflict: bool,
    publication_ownership: Option<PublicationOwnershipReceipt>,
}

impl<'a> DeliveryRecoveryObservation<'a> {
    /// Binds exact retained rows and publication-owner observations to one recovery decision.
    #[must_use]
    pub const fn new(
        deliveries: &'a [EvaluationDirectiveDelivery],
        report_artifact_verified: bool,
        report_evidence_admitted: bool,
        identity_conflict: bool,
    ) -> Self {
        Self {
            deliveries,
            report_artifact_verified,
            report_evidence_admitted,
            identity_conflict,
            publication_ownership: None,
        }
    }

    /// Adds the exact immutable report/directive/settlement ownership proof for a published state.
    #[must_use]
    pub const fn with_publication_ownership(
        mut self,
        ownership: PublicationOwnershipReceipt,
    ) -> Self {
        self.publication_ownership = Some(ownership);
        self
    }

    /// Exact retained rows observed through C0.
    #[must_use]
    pub const fn deliveries(self) -> &'a [EvaluationDirectiveDelivery] {
        self.deliveries
    }

    /// Whether the committed report artifact was loaded and verified.
    #[must_use]
    pub const fn report_artifact_verified(self) -> bool {
        self.report_artifact_verified
    }

    /// Whether exact report evidence already exists.
    #[must_use]
    pub const fn report_evidence_admitted(self) -> bool {
        self.report_evidence_admitted
    }

    /// Whether an owner reported irreconcilable identity drift.
    #[must_use]
    pub const fn identity_conflict(self) -> bool {
        self.identity_conflict
    }

    /// Exact terminal publication ownership proof, when independently recovered.
    #[must_use]
    pub const fn publication_ownership(self) -> Option<PublicationOwnershipReceipt> {
        self.publication_ownership
    }
}

/// Result of observing one exact publication dependency through its owning store.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationDependencyStatus {
    /// The exact dependency exists and passed owner validation.
    Verified,
    /// The owner authoritatively reported the exact identity absent.
    Missing,
    /// The owner could not complete the observation and the same read may be retried.
    Unavailable,
}

/// Read-only observation of the exact persistent publication directive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationDirectiveObservation {
    /// The exact retained row was loaded, decoded, and bound to the report operation.
    Observed(PublicationDirectiveDelivery),
    /// The outbox owner authoritatively reported the canonical identity absent.
    Missing,
    /// The outbox owner could not complete the read.
    Unavailable,
}

/// Identity-bound owner observations for one report publication restart.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicationRecoveryObservation {
    campaign_id: crate::EvaluationCampaignId,
    report: ReportRecord,
    publication: PublicationRecord,
    evidence_id: EvidenceId,
    artifact: PublicationDependencyStatus,
    evidence: PublicationDependencyStatus,
    directive: PublicationDirectiveObservation,
    ownership: Option<PublicationOwnershipReceipt>,
}

impl PublicationRecoveryObservation {
    #[allow(clippy::too_many_arguments, reason = "independent owner facts remain explicit")]
    pub(crate) const fn new(
        campaign_id: crate::EvaluationCampaignId,
        report: ReportRecord,
        publication: PublicationRecord,
        evidence_id: EvidenceId,
        artifact: PublicationDependencyStatus,
        evidence: PublicationDependencyStatus,
        directive: PublicationDirectiveObservation,
        ownership: Option<PublicationOwnershipReceipt>,
    ) -> Self {
        Self {
            campaign_id,
            report,
            publication,
            evidence_id,
            artifact,
            evidence,
            directive,
            ownership,
        }
    }

    /// Owning evaluation campaign.
    #[must_use]
    pub const fn campaign_id(self) -> crate::EvaluationCampaignId {
        self.campaign_id
    }
    /// Exact committed report.
    #[must_use]
    pub const fn report(self) -> ReportRecord {
        self.report
    }
    /// Expected or accepted evidence-backed publication.
    #[must_use]
    pub const fn publication(self) -> PublicationRecord {
        self.publication
    }
    /// Content-derived exact evidence identity.
    #[must_use]
    pub const fn evidence_id(self) -> EvidenceId {
        self.evidence_id
    }
    /// Artifact-owner observation.
    #[must_use]
    pub const fn artifact(self) -> PublicationDependencyStatus {
        self.artifact
    }
    /// Evidence-owner observation.
    #[must_use]
    pub const fn evidence(self) -> PublicationDependencyStatus {
        self.evidence
    }
    /// Exact directive-owner observation.
    #[must_use]
    pub const fn directive(self) -> PublicationDirectiveObservation {
        self.directive
    }
    /// Exact terminal operation ownership, present only after verified settlement recovery.
    #[must_use]
    pub const fn ownership(self) -> Option<PublicationOwnershipReceipt> {
        self.ownership
    }
}

/// Exact semantic effect named by one retained E3 outbox row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvaluationDeliveryTarget {
    /// D3 scheduling or cancellation for one rollout.
    Schedule {
        /// Logical rollout identity.
        rollout_id: RolloutId,
    },
    /// Candidate/evaluator execution or cancellation for one attempt.
    Execution {
        /// Logical rollout identity.
        rollout_id: RolloutId,
        /// Exact one-based logical attempt.
        attempt: u16,
        /// Retained retry provenance, absent for initial or accepted legacy execution.
        retry: Option<RetryIntent>,
    },
    /// Publication of the campaign's exact committed report.
    Publication,
}

/// Closed recovery action selected without guessing an external outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvaluationRecoveryDecision {
    /// No effect is pending; continue normal planning/scheduling.
    Continue,
    /// Redeliver existing schedule/cancellation directives.
    RedeliverScheduling,
    /// Redeliver existing execution/cancellation directives.
    RedeliverExecution,
    /// Redeliver the exact retained retry directive already committed with its predecessor ack.
    RedeliverRetainedRetry {
        /// Logical rollout that owns the retained continuation.
        rollout_id: RolloutId,
        /// Complete artifact, continuation, policy, delay, and next-attempt identity.
        retry: RetryIntent,
    },
    /// Act on one exact persistent row without replacing its semantic effect identity.
    DeliverDirective {
        /// Schedule, execution attempt, retained retry, or publication named by the row.
        target: EvaluationDeliveryTarget,
        /// Pending rows may be claimed, live claims must wait, and expired claims may be reclaimed.
        status: OutboxDeliveryStatus,
    },
    /// Every logical rollout is terminal; deterministic analysis may begin.
    BeginAnalysis,
    /// Continue the exact durable analysis frontier under its retained owner.
    ResumeAnalysis,
    /// Report bytes must be finalized or reconciled.
    ReconcileReportArtifact,
    /// Restore the canonical publication directive under the original report operation.
    ReconcilePublicationIntent,
    /// Publication directive should be retried exactly.
    RetryPublication,
    /// Evidence exists after a crash; retry the exact atomic settlement.
    ReconcileEvidenceSettlement,
    /// Cancellation routing/settlement must continue.
    ContinueCancellation,
    /// Durable suspension remains authoritative until an explicit matching resume.
    RemainSuspended,
    /// A publication owner could not complete an exact observation; retry that read later.
    AwaitPublicationObservation,
    /// Campaign is already terminal and consistent.
    Complete,
    /// Conflicting external identities require quarantine.
    Quarantine,
}

/// Selects one exact recovery action from state and observed external ownership.
#[must_use]
pub fn decide_recovery(
    state: &EvaluationState,
    observed: RecoveryObservation,
) -> EvaluationRecoveryDecision {
    if observed.identity_conflict {
        return EvaluationRecoveryDecision::Quarantine;
    }
    match state.phase() {
        EvaluationPhase::Published => return EvaluationRecoveryDecision::Quarantine,
        EvaluationPhase::Failed | EvaluationPhase::Cancelled => {
            return EvaluationRecoveryDecision::Complete;
        }
        EvaluationPhase::Cancelling => {
            return EvaluationRecoveryDecision::ContinueCancellation;
        }
        EvaluationPhase::Suspended => return EvaluationRecoveryDecision::RemainSuspended,
        EvaluationPhase::ReportReady => {
            if state.report().is_none() {
                return EvaluationRecoveryDecision::Quarantine;
            }
            if !observed.report_artifact_verified {
                return EvaluationRecoveryDecision::ReconcileReportArtifact;
            }
            if observed.publication_directive {
                return if observed.report_evidence_admitted {
                    EvaluationRecoveryDecision::ReconcileEvidenceSettlement
                } else {
                    EvaluationRecoveryDecision::RetryPublication
                };
            }
            return EvaluationRecoveryDecision::ReconcilePublicationIntent;
        }
        EvaluationPhase::Analyzing => {
            return if observed.schedule_directives == 0
                && observed.execution_directives == 0
                && !observed.publication_directive
            {
                if state.analysis_digest().is_some() {
                    EvaluationRecoveryDecision::ReconcileReportArtifact
                } else {
                    EvaluationRecoveryDecision::ResumeAnalysis
                }
            } else {
                EvaluationRecoveryDecision::Quarantine
            };
        }
        EvaluationPhase::Created
        | EvaluationPhase::Planned
        | EvaluationPhase::Scheduling
        | EvaluationPhase::Running => {}
    }
    let mut retained_retries = state.rollouts().filter_map(|(rollout_id, progress)| {
        match progress.status() {
            RolloutStatus::RetryPending { retry } | RolloutStatus::RetryRunning { retry } => {
                Some((rollout_id, retry))
            }
            _ => None,
        }
    });
    if let Some((rollout_id, retry)) = retained_retries.next() {
        let Some(required_directives) = u32::try_from(retained_retries.count())
            .ok()
            .and_then(|additional| additional.checked_add(1))
        else {
            return EvaluationRecoveryDecision::Quarantine;
        };
        if observed.execution_directives < required_directives {
            return EvaluationRecoveryDecision::Quarantine;
        }
        return EvaluationRecoveryDecision::RedeliverRetainedRetry { rollout_id, retry };
    }
    if observed.schedule_directives > 0 {
        return EvaluationRecoveryDecision::RedeliverScheduling;
    }
    if observed.execution_directives > 0
        || state
            .rollouts()
            .any(|(_, value)| matches!(value.status(), RolloutStatus::Running { .. }))
    {
        return EvaluationRecoveryDecision::RedeliverExecution;
    }
    if state.counts().complete()
        && matches!(
            state.phase(),
            EvaluationPhase::Planned | EvaluationPhase::Scheduling | EvaluationPhase::Running
        )
    {
        return EvaluationRecoveryDecision::BeginAnalysis;
    }
    EvaluationRecoveryDecision::Continue
}

/// Selects recovery from exact persistent rows and their time-aware C0 delivery states.
///
/// Every nonterminal effect-bearing aggregate state must have one matching retained directive.
/// Bounded or exhausted rows are rejected by the directive observers before this function runs;
/// acknowledged rows without their atomic aggregate settlement are quarantined here.
#[must_use]
pub fn decide_recovery_with_delivery(
    state: &EvaluationState,
    observed: DeliveryRecoveryObservation<'_>,
) -> EvaluationRecoveryDecision {
    if observed.identity_conflict() {
        return EvaluationRecoveryDecision::Quarantine;
    }
    let Some(targets) = expected_targets(state) else {
        return EvaluationRecoveryDecision::Quarantine;
    };
    if state.phase() == EvaluationPhase::ReportReady
        && targets.as_slice() == [EvaluationDeliveryTarget::Publication]
        && observed.deliveries().is_empty()
    {
        return if observed.report_artifact_verified() {
            EvaluationRecoveryDecision::ReconcilePublicationIntent
        } else {
            EvaluationRecoveryDecision::ReconcileReportArtifact
        };
    }
    if targets.len() != observed.deliveries().len() {
        return EvaluationRecoveryDecision::Quarantine;
    }
    let mut actions = Vec::with_capacity(targets.len());
    for target in targets {
        let mut matching = observed
            .deliveries()
            .iter()
            .filter(|delivery| delivery_matches_target(state, target, delivery));
        let Some(delivery) = matching.next() else {
            return EvaluationRecoveryDecision::Quarantine;
        };
        if matching.next().is_some() {
            return EvaluationRecoveryDecision::Quarantine;
        }
        let status = delivery.status();
        if matches!(status, OutboxDeliveryStatus::Acknowledged | OutboxDeliveryStatus::Exhausted) {
            return EvaluationRecoveryDecision::Quarantine;
        }
        actions.push((target, status));
    }
    if state.phase() == EvaluationPhase::Published {
        let complete = state.report().is_some()
            && state.publication().is_some_and(|publication| {
                state.report().map(ReportRecord::id) == Some(publication.report_id())
            })
            && observed.report_artifact_verified()
            && observed.report_evidence_admitted()
            && observed
                .publication_ownership()
                .is_some_and(|ownership| ownership.matches_state(state));
        return if complete {
            EvaluationRecoveryDecision::Complete
        } else {
            EvaluationRecoveryDecision::Quarantine
        };
    }
    if matches!(state.phase(), EvaluationPhase::Failed | EvaluationPhase::Cancelled) {
        return EvaluationRecoveryDecision::Complete;
    }
    if state.phase() == EvaluationPhase::Suspended {
        return EvaluationRecoveryDecision::RemainSuspended;
    }
    if state.phase() == EvaluationPhase::Cancelling {
        return actions.first().map_or(
            EvaluationRecoveryDecision::ContinueCancellation,
            |(target, status)| EvaluationRecoveryDecision::DeliverDirective {
                target: *target,
                status: *status,
            },
        );
    }
    if state.phase() == EvaluationPhase::ReportReady {
        if !observed.report_artifact_verified() {
            return EvaluationRecoveryDecision::ReconcileReportArtifact;
        }
        let Some((target, status)) = actions.first().copied() else {
            return EvaluationRecoveryDecision::Quarantine;
        };
        if target != EvaluationDeliveryTarget::Publication {
            return EvaluationRecoveryDecision::Quarantine;
        }
        return match status {
            OutboxDeliveryStatus::Pending if observed.report_evidence_admitted() => {
                EvaluationRecoveryDecision::ReconcileEvidenceSettlement
            }
            OutboxDeliveryStatus::Pending
            | OutboxDeliveryStatus::Waiting { .. }
            | OutboxDeliveryStatus::Reclaimable { .. } => {
                EvaluationRecoveryDecision::DeliverDirective { target, status }
            }
            OutboxDeliveryStatus::Acknowledged | OutboxDeliveryStatus::Exhausted => {
                EvaluationRecoveryDecision::Quarantine
            }
        };
    }
    if state.phase() == EvaluationPhase::Analyzing {
        return if state.analysis_digest().is_some() {
            EvaluationRecoveryDecision::ReconcileReportArtifact
        } else {
            EvaluationRecoveryDecision::ResumeAnalysis
        };
    }
    if let Some((target, status)) = actions.first().copied() {
        return EvaluationRecoveryDecision::DeliverDirective { target, status };
    }
    if state.counts().complete()
        && matches!(
            state.phase(),
            EvaluationPhase::Planned | EvaluationPhase::Scheduling | EvaluationPhase::Running
        )
    {
        return EvaluationRecoveryDecision::BeginAnalysis;
    }
    EvaluationRecoveryDecision::Continue
}

/// Chooses publication restart work from exact artifact, evidence, directive, and operation owners.
#[must_use]
pub fn decide_publication_recovery(
    state: &EvaluationState,
    observed: PublicationRecoveryObservation,
) -> EvaluationRecoveryDecision {
    let Some(report) = state.report() else {
        return EvaluationRecoveryDecision::Quarantine;
    };
    if observed.campaign_id() != state.campaign_id()
        || observed.report() != report
        || observed.evidence_id() != observed.publication().evidence_id()
        || observed.publication().report_id() != report.id()
    {
        return EvaluationRecoveryDecision::Quarantine;
    }
    match state.phase() {
        EvaluationPhase::ReportReady => {
            if observed.ownership().is_some() {
                return EvaluationRecoveryDecision::Quarantine;
            }
            if observed.artifact() == PublicationDependencyStatus::Unavailable
                || observed.evidence() == PublicationDependencyStatus::Unavailable
                || observed.directive() == PublicationDirectiveObservation::Unavailable
            {
                return EvaluationRecoveryDecision::AwaitPublicationObservation;
            }
            if observed.artifact() != PublicationDependencyStatus::Verified {
                return EvaluationRecoveryDecision::ReconcileReportArtifact;
            }
            match observed.directive() {
                PublicationDirectiveObservation::Missing => {
                    EvaluationRecoveryDecision::ReconcilePublicationIntent
                }
                PublicationDirectiveObservation::Observed(delivery) => {
                    let target = EvaluationDeliveryTarget::Publication;
                    if !publication_delivery_matches(state, delivery) {
                        return EvaluationRecoveryDecision::Quarantine;
                    }
                    match delivery.status() {
                        OutboxDeliveryStatus::Pending
                            if observed.evidence() == PublicationDependencyStatus::Verified =>
                        {
                            EvaluationRecoveryDecision::ReconcileEvidenceSettlement
                        }
                        OutboxDeliveryStatus::Pending
                            if observed.evidence() == PublicationDependencyStatus::Missing =>
                        {
                            EvaluationRecoveryDecision::DeliverDirective {
                                target,
                                status: OutboxDeliveryStatus::Pending,
                            }
                        }
                        status @ (OutboxDeliveryStatus::Waiting { .. }
                        | OutboxDeliveryStatus::Reclaimable { .. }) => {
                            EvaluationRecoveryDecision::DeliverDirective { target, status }
                        }
                        OutboxDeliveryStatus::Acknowledged
                        | OutboxDeliveryStatus::Exhausted
                        | OutboxDeliveryStatus::Pending => EvaluationRecoveryDecision::Quarantine,
                    }
                }
                PublicationDirectiveObservation::Unavailable => {
                    EvaluationRecoveryDecision::AwaitPublicationObservation
                }
            }
        }
        EvaluationPhase::Published => {
            if state.publication() != Some(observed.publication()) {
                return EvaluationRecoveryDecision::Quarantine;
            }
            if observed.artifact() == PublicationDependencyStatus::Unavailable
                || observed.evidence() == PublicationDependencyStatus::Unavailable
            {
                return EvaluationRecoveryDecision::AwaitPublicationObservation;
            }
            if observed.artifact() == PublicationDependencyStatus::Verified
                && observed.evidence() == PublicationDependencyStatus::Verified
                && observed
                    .ownership()
                    .is_some_and(|ownership| ownership.matches_state(state))
            {
                EvaluationRecoveryDecision::Complete
            } else {
                EvaluationRecoveryDecision::Quarantine
            }
        }
        EvaluationPhase::Created
        | EvaluationPhase::Planned
        | EvaluationPhase::Scheduling
        | EvaluationPhase::Running
        | EvaluationPhase::Cancelling
        | EvaluationPhase::Analyzing
        | EvaluationPhase::Failed
        | EvaluationPhase::Cancelled
        | EvaluationPhase::Suspended => EvaluationRecoveryDecision::Quarantine,
    }
}

fn expected_targets(state: &EvaluationState) -> Option<Vec<EvaluationDeliveryTarget>> {
    let mut targets = Vec::new();
    for (rollout_id, progress) in state.rollouts() {
        match progress.status() {
            RolloutStatus::Planned
            | RolloutStatus::Settled(_)
            | RolloutStatus::Cancelled { .. } => {}
            RolloutStatus::Scheduling => {
                targets.push(EvaluationDeliveryTarget::Schedule { rollout_id });
            }
            RolloutStatus::Scheduled { .. } => {
                targets.push(EvaluationDeliveryTarget::Execution {
                    rollout_id,
                    attempt: progress.attempts_retained().checked_add(1)?,
                    retry: None,
                });
            }
            RolloutStatus::Running { attempt } => {
                targets.push(EvaluationDeliveryTarget::Execution {
                    rollout_id,
                    attempt,
                    retry: None,
                });
            }
            RolloutStatus::RetryPending { retry } | RolloutStatus::RetryRunning { retry } => {
                targets.push(EvaluationDeliveryTarget::Execution {
                    rollout_id,
                    attempt: retry.next_attempt(),
                    retry: Some(retry),
                });
            }
        }
    }
    if state.phase() == EvaluationPhase::ReportReady
        || state.phase() == EvaluationPhase::Cancelling
            && state.cancellation_origin() == Some(EvaluationPhase::ReportReady)
            && state.publication_cancellation().is_none()
    {
        targets.push(EvaluationDeliveryTarget::Publication);
    }
    Some(targets)
}

fn delivery_matches_target(
    state: &EvaluationState,
    target: EvaluationDeliveryTarget,
    delivery: &EvaluationDirectiveDelivery,
) -> bool {
    match (target, delivery) {
        (
            EvaluationDeliveryTarget::Schedule { rollout_id },
            EvaluationDirectiveDelivery::Schedule(delivery),
        ) => schedule_delivery_matches(state, rollout_id, delivery),
        (
            EvaluationDeliveryTarget::Execution { rollout_id, attempt, retry },
            EvaluationDirectiveDelivery::Execution(delivery),
        ) => execution_delivery_matches(state, rollout_id, attempt, retry, *delivery),
        (
            EvaluationDeliveryTarget::Publication,
            EvaluationDirectiveDelivery::Publication(delivery),
        ) => publication_delivery_matches(state, *delivery),
        _ => false,
    }
}

fn schedule_delivery_matches(
    state: &EvaluationState,
    rollout_id: RolloutId,
    delivery: &ScheduleDirectiveDelivery,
) -> bool {
    let Some(progress) = state.rollout(rollout_id) else {
        return false;
    };
    let directive = delivery.directive();
    if directive.campaign_id() != state.campaign_id() || directive.rollout_id() != rollout_id {
        return false;
    }
    match directive.kind() {
        ScheduleDirectiveKind::Submit(work) => {
            work.id() == progress.binding().work_id()
                && work.payload_digest() == progress.binding().request_digest()
                && work.revision() == *state.revision()
                && work.class() == peritus_scheduler::ExecutionClass::Coordination
        }
        ScheduleDirectiveKind::Cancel(work_id) => {
            state.phase() == EvaluationPhase::Cancelling
                && *work_id == progress.binding().work_id()
        }
    }
}

fn execution_delivery_matches(
    state: &EvaluationState,
    rollout_id: RolloutId,
    attempt: u16,
    retry: Option<RetryIntent>,
    delivery: crate::ExecutionDirectiveDelivery,
) -> bool {
    let Some(progress) = state.rollout(rollout_id) else {
        return false;
    };
    let directive = delivery.directive();
    if directive.campaign_id() != state.campaign_id() || directive.rollout_id() != rollout_id {
        return false;
    }
    match directive.kind() {
        ExecutionDirectiveKind::Execute { request_digest } => {
            retry.is_none() && request_digest == progress.binding().request_digest()
        }
        ExecutionDirectiveKind::ExecuteAttempt {
            request_digest,
            attempt: delivered_attempt,
            retry: delivered_retry,
        } => {
            request_digest == progress.binding().request_digest()
                && delivered_attempt == attempt
                && delivered_retry == retry
        }
        ExecutionDirectiveKind::Cancel => state.phase() == EvaluationPhase::Cancelling,
    }
}

fn publication_delivery_matches(
    state: &EvaluationState,
    delivery: PublicationDirectiveDelivery,
) -> bool {
    let Some(report) = state.report() else {
        return false;
    };
    let directive = delivery.directive();
    directive.campaign_id() == state.campaign_id() && directive.report() == report
}
