//! Stable commit-before-effect directives and delivery state.

use peritus_collaboration::CollaborationTaskId;
use peritus_scheduler::WorkId;
use peritus_types::{EventId, RevisionTuple, Sha256Digest};

use crate::{
    Handoff, OrchestratorError, OrchestratorErrorKind, OrchestratorRecoveryAction,
    QualityCycleBinding, ResumeReconciliation,
};

/// Stable idempotency identity for one external directive.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DirectiveId([u8; 16]);

impl DirectiveId {
    /// Canonical byte length.
    pub const LENGTH: usize = 16;

    /// Creates an identity, rejecting the reserved all-zero value.
    ///
    /// # Errors
    /// Returns [`OrchestratorErrorKind::InvalidInput`] for zero bytes.
    pub fn new(bytes: [u8; 16]) -> Result<Self, OrchestratorError> {
        if bytes.iter().all(|byte| *byte == 0) {
            Err(invalid("all-zero directive identity is reserved"))
        } else {
            Ok(Self(bytes))
        }
    }

    /// Borrows the exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Returns the exact canonical bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// Closed external authority boundary addressed by a directive.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DirectiveDestination {
    /// D3 resource scheduler.
    Scheduler,
    /// D3 collaboration graph.
    Collaboration,
    /// D0 writer or fixer turn.
    Agent,
    /// D1 gate engine.
    Gates,
    /// D2 review engine.
    Review,
    /// Pure B2 acceptance evaluator port.
    QualityEvaluator,
    /// B0 lifecycle kernel.
    Kernel,
}

impl DirectiveDestination {
    /// Returns the stable C0 outbox destination.
    #[must_use]
    pub const fn outbox_destination(self) -> &'static str {
        match self {
            Self::Scheduler => "peritus.scheduler",
            Self::Collaboration => "peritus.collaboration",
            Self::Agent => "peritus.agent",
            Self::Gates => "peritus.gates",
            Self::Review => "peritus.review",
            Self::QualityEvaluator => "peritus.quality-evaluator",
            Self::Kernel => "peritus.kernel",
        }
    }
}

/// Closed semantic action requested through an external port.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DirectiveKind {
    /// Start or resume the writer handoff.
    StartWriter,
    /// Start the exact-current D1 gate run.
    StartGates,
    /// Start the exact-current independent D2 review.
    StartReview,
    /// Start a fixer handoff for canonical finding identities.
    StartFixer,
    /// Evaluate exact evidence under B2.
    EvaluateAcceptance,
    /// Ask D3 to close scheduler and collaboration work for a completed role handoff.
    FinalizeChildren,
    /// Ask B0 to begin acceptance for the run.
    BeginKernelAcceptance,
    /// Ask B0 to evaluate the exact acceptance inputs.
    EvaluateKernelAcceptance,
    /// Pause owned children without creating new work.
    PauseChildren,
    /// Resume one previously paused owned child without creating new work.
    ResumeChildren,
    /// Cancel owned children and reconcile their terminals.
    CancelChildren,
}

/// Durable delivery progress for one directive.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DirectiveDeliveryState {
    /// Committed and ready for first publication.
    Ready,
    /// At least one publication was attempted; a matching acknowledgement is pending.
    Published,
    /// The destination durably acknowledged the exact directive.
    Acknowledged,
}

/// Exact state-bound material used to derive a non-B2/non-B0 directive payload.
#[derive(Clone, Copy, Debug)]
pub enum DirectivePayloadBinding<'a> {
    /// Exact D3 handoff for writer, review, or fixer activation.
    Handoff(&'a Handoff),
    /// Exact current quality cycle for gates or D3 finalization.
    QualityCycle(&'a QualityCycleBinding),
    /// Exact child-head checkpoint for one pause destination.
    Reconciliation(&'a ResumeReconciliation),
    /// Exact cancellation cause for one owned child destination.
    Cancellation(Sha256Digest),
}

/// Derives the canonical payload digest for a state-bound child directive.
///
/// B2 evaluation and B0 kernel directives use the richer helpers on
/// [`crate::AcceptanceCertificate`] instead.
///
/// # Errors
/// Rejects a payload shape that is not defined for the supplied directive kind.
pub fn directive_payload_digest(
    kind: DirectiveKind,
    destination: DirectiveDestination,
    binding: DirectivePayloadBinding<'_>,
) -> Result<Sha256Digest, OrchestratorError> {
    let valid = match kind {
        DirectiveKind::StartWriter | DirectiveKind::StartReview | DirectiveKind::StartFixer => {
            matches!(binding, DirectivePayloadBinding::Handoff(_))
        }
        DirectiveKind::StartGates | DirectiveKind::FinalizeChildren => {
            matches!(binding, DirectivePayloadBinding::QualityCycle(_))
        }
        DirectiveKind::PauseChildren | DirectiveKind::ResumeChildren => {
            matches!(binding, DirectivePayloadBinding::Reconciliation(_))
        }
        DirectiveKind::CancelChildren => {
            matches!(binding, DirectivePayloadBinding::Cancellation(_))
        }
        DirectiveKind::EvaluateAcceptance
        | DirectiveKind::BeginKernelAcceptance
        | DirectiveKind::EvaluateKernelAcceptance => false,
    };
    if !valid {
        return Err(invalid("directive kind and payload binding differ"));
    }
    if let DirectivePayloadBinding::Cancellation(cause) = binding
        && cause.as_bytes().iter().all(|byte| *byte == 0)
    {
        return Err(invalid("cancellation directive cause digest must be nonzero"));
    }
    Ok(crate::canonical::directive_payload_digest(kind, destination, binding))
}

/// One stable, bounded, commit-before-effect outbox directive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingDirective {
    id: DirectiveId,
    destination: DirectiveDestination,
    kind: DirectiveKind,
    payload_digest: Sha256Digest,
    maximum_deliveries: u16,
    deliveries: u16,
    delivery_state: DirectiveDeliveryState,
    source_event: EventId,
    task_id: Option<CollaborationTaskId>,
    work_id: Option<WorkId>,
    revision: RevisionTuple,
}

impl PendingDirective {
    /// Creates one committed directive before any external delivery is permitted.
    ///
    /// # Errors
    /// Rejects zero payload digests, zero delivery bounds, or partial D3 task/work bindings.
    #[allow(clippy::too_many_arguments, reason = "durable effect bindings remain explicit")]
    pub fn new(
        id: DirectiveId,
        destination: DirectiveDestination,
        kind: DirectiveKind,
        payload_digest: Sha256Digest,
        maximum_deliveries: u16,
        source_event: EventId,
        task_id: Option<CollaborationTaskId>,
        work_id: Option<WorkId>,
        revision: RevisionTuple,
    ) -> Result<Self, OrchestratorError> {
        let value = Self {
            id,
            destination,
            kind,
            payload_digest,
            maximum_deliveries,
            deliveries: 0,
            delivery_state: DirectiveDeliveryState::Ready,
            source_event,
            task_id,
            work_id,
            revision,
        };
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments, reason = "wire fields remain explicit and auditable")]
    pub(crate) fn from_wire(
        id: DirectiveId,
        destination: DirectiveDestination,
        kind: DirectiveKind,
        payload_digest: Sha256Digest,
        maximum_deliveries: u16,
        deliveries: u16,
        delivery_state: DirectiveDeliveryState,
        source_event: EventId,
        task_id: Option<CollaborationTaskId>,
        work_id: Option<WorkId>,
        revision: RevisionTuple,
    ) -> Result<Self, OrchestratorError> {
        let value = Self {
            id,
            destination,
            kind,
            payload_digest,
            maximum_deliveries,
            deliveries,
            delivery_state,
            source_event,
            task_id,
            work_id,
            revision,
        };
        value.validate()?;
        Ok(value)
    }

    /// Returns the stable directive identity.
    #[must_use]
    pub const fn id(&self) -> DirectiveId {
        self.id
    }

    /// Returns the addressed authority boundary.
    #[must_use]
    pub const fn destination(&self) -> DirectiveDestination {
        self.destination
    }

    /// Returns the closed semantic action.
    #[must_use]
    pub const fn kind(&self) -> DirectiveKind {
        self.kind
    }

    /// Returns the exact inert payload digest.
    #[must_use]
    pub const fn payload_digest(&self) -> Sha256Digest {
        self.payload_digest
    }

    /// Returns the maximum permitted publications.
    #[must_use]
    pub const fn maximum_deliveries(&self) -> u16 {
        self.maximum_deliveries
    }

    /// Returns publications already attempted.
    #[must_use]
    pub const fn deliveries(&self) -> u16 {
        self.deliveries
    }

    /// Returns durable delivery progress.
    #[must_use]
    pub const fn delivery_state(&self) -> DirectiveDeliveryState {
        self.delivery_state
    }

    /// Returns the committed event that created the directive.
    #[must_use]
    pub const fn source_event(&self) -> EventId {
        self.source_event
    }

    /// Returns the D3 causal task binding, when the directive owns work.
    #[must_use]
    pub const fn task_id(&self) -> Option<CollaborationTaskId> {
        self.task_id
    }

    /// Returns the D3 scheduler work binding, when the directive owns work.
    #[must_use]
    pub const fn work_id(&self) -> Option<WorkId> {
        self.work_id
    }

    /// Returns the exact candidate revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }

    /// Returns whether another publication is permitted.
    #[must_use]
    pub const fn can_publish(&self) -> bool {
        !matches!(self.delivery_state, DirectiveDeliveryState::Acknowledged)
            && self.deliveries < self.maximum_deliveries
    }

    /// Records one publication attempt before an external port is invoked.
    ///
    /// # Errors
    /// Rejects acknowledgement regression or delivery-limit exhaustion.
    pub(crate) fn mark_published(&mut self) -> Result<(), OrchestratorError> {
        if !self.can_publish() {
            return Err(OrchestratorError::new(
                OrchestratorErrorKind::LimitExceeded,
                OrchestratorRecoveryAction::NeedsHuman,
                "directive is acknowledged or has exhausted its delivery bound",
            ));
        }
        self.deliveries = self.deliveries.checked_add(1).ok_or_else(|| {
            OrchestratorError::new(
                OrchestratorErrorKind::LimitExceeded,
                OrchestratorRecoveryAction::NeedsHuman,
                "directive delivery counter overflowed",
            )
        })?;
        self.delivery_state = DirectiveDeliveryState::Published;
        Ok(())
    }

    /// Records a matching destination acknowledgement.
    ///
    /// # Errors
    /// Rejects acknowledgement before publication. Repeated acknowledgement is idempotent.
    pub(crate) const fn acknowledge(&mut self) -> Result<(), OrchestratorError> {
        match self.delivery_state {
            DirectiveDeliveryState::Ready => Err(OrchestratorError::new(
                OrchestratorErrorKind::InvalidTransition,
                OrchestratorRecoveryAction::CorrectInput,
                "directive cannot be acknowledged before publication",
            )),
            DirectiveDeliveryState::Published | DirectiveDeliveryState::Acknowledged => {
                self.delivery_state = DirectiveDeliveryState::Acknowledged;
                Ok(())
            }
        }
    }

    pub(crate) fn validate(&self) -> Result<(), OrchestratorError> {
        if self.payload_digest.as_bytes().iter().all(|byte| *byte == 0)
            || self.maximum_deliveries == 0
            || self.deliveries > self.maximum_deliveries
            || self.task_id.is_some() != self.work_id.is_some()
        {
            return Err(invalid("directive contains invalid digest, bounds, or D3 binding"));
        }
        match self.delivery_state {
            DirectiveDeliveryState::Ready if self.deliveries != 0 => {
                Err(invalid("ready directive already records a publication"))
            }
            DirectiveDeliveryState::Published | DirectiveDeliveryState::Acknowledged
                if self.deliveries == 0 =>
            {
                Err(invalid("delivered directive has no publication count"))
            }
            _ => Ok(()),
        }
    }
}

const fn invalid(detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::InvalidInput,
        OrchestratorRecoveryAction::CorrectInput,
        detail,
    )
}
