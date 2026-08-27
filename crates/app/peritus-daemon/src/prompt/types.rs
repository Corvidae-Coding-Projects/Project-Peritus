//! Public prompt ownership, admission, and accepted-response values.

use peritus_app_protocol::{PromptAnswer, PromptCancellation, PromptCorrelation};
use peritus_approval::{ApprovalRequest, AuthenticatedApprovalObservation, SignedApprovalDecision};
use peritus_types::{ActorId, Generation, RevisionTuple, SessionId};

use super::{PromptBrokerError, PromptBrokerErrorKind};

const MAX_OUTSTANDING_PROMPTS: usize = 4_096;

/// Startup ceiling for prompt entries retained until authority settlement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PromptBrokerLimits {
    maximum_outstanding: usize,
}

impl PromptBrokerLimits {
    /// Conservative single-user production default.
    pub const PRODUCTION: Self = Self { maximum_outstanding: 256 };

    /// Creates a positive prompt ceiling within the compiled production maximum.
    ///
    /// # Errors
    ///
    /// Rejects zero or a value above the compiled bound.
    pub const fn new(maximum_outstanding: usize) -> Result<Self, PromptBrokerError> {
        if maximum_outstanding == 0 || maximum_outstanding > MAX_OUTSTANDING_PROMPTS {
            Err(PromptBrokerError::new(
                PromptBrokerErrorKind::InvalidLimit,
                "prompt registry limit must be positive and within its compiled maximum",
            ))
        } else {
            Ok(Self { maximum_outstanding })
        }
    }

    /// Returns the maximum awaiting and retained-terminal entries.
    #[must_use]
    pub const fn maximum_outstanding(self) -> usize {
        self.maximum_outstanding
    }
}

impl Default for PromptBrokerLimits {
    fn default() -> Self {
        Self::PRODUCTION
    }
}

/// Independently authoritative live facts supplied for one response attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PromptAdmission {
    actor_id: ActorId,
    session_id: SessionId,
    live_revision: RevisionTuple,
    cancellation_generation: Generation,
}

impl PromptAdmission {
    /// Binds an authenticated peer to the current domain freshness observations.
    #[must_use]
    pub const fn new(
        actor_id: ActorId,
        session_id: SessionId,
        live_revision: RevisionTuple,
        cancellation_generation: Generation,
    ) -> Self {
        Self { actor_id, session_id, live_revision, cancellation_generation }
    }

    /// Returns the authenticated actor rather than a client-selected identity.
    #[must_use]
    pub const fn actor_id(self) -> ActorId {
        self.actor_id
    }

    /// Returns the durable authenticated application session.
    #[must_use]
    pub const fn session_id(self) -> SessionId {
        self.session_id
    }

    /// Returns the current authoritative revision.
    #[must_use]
    pub const fn live_revision(self) -> RevisionTuple {
        self.live_revision
    }

    /// Returns the current authoritative cancellation generation.
    #[must_use]
    pub const fn cancellation_generation(self) -> Generation {
        self.cancellation_generation
    }
}

/// One signed approval admitted by A3 freshness and strict B1 authentication.
///
/// This is not a committed approval or effect permit. AuthorityOwner must resolve the move-only
/// observation against the same current registry and commit the resulting B1 transition.
#[derive(Debug, Eq, PartialEq)]
pub struct AuthenticatedApprovalResponse {
    answer: PromptAnswer,
    request: ApprovalRequest,
    signed: SignedApprovalDecision,
    observation: AuthenticatedApprovalObservation,
}

impl AuthenticatedApprovalResponse {
    pub(super) const fn new(
        answer: PromptAnswer,
        request: ApprovalRequest,
        signed: SignedApprovalDecision,
        observation: AuthenticatedApprovalObservation,
    ) -> Self {
        Self { answer, request, signed, observation }
    }

    /// Borrows the exact accepted A3 answer.
    #[must_use]
    pub const fn answer(&self) -> &PromptAnswer {
        &self.answer
    }

    /// Borrows the canonical decoded B1 request.
    #[must_use]
    pub const fn request(&self) -> &ApprovalRequest {
        &self.request
    }

    /// Borrows the canonical decoded signed decision.
    #[must_use]
    pub const fn signed_decision(&self) -> &SignedApprovalDecision {
        &self.signed
    }

    /// Borrows the move-only B1 authentication observation.
    #[must_use]
    pub const fn observation(&self) -> &AuthenticatedApprovalObservation {
        &self.observation
    }

    /// Consumes the response for B1 resolution and durable settlement.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (PromptAnswer, ApprovalRequest, SignedApprovalDecision, AuthenticatedApprovalObservation)
    {
        (self.answer, self.request, self.signed, self.observation)
    }
}

/// Accepted cancellation source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PromptCancellationAcceptance {
    /// An approval-answer payload carried the unprivileged `Cancel` intent.
    ApprovalAnswer(PromptAnswer),
    /// The dedicated A3 cancellation request was accepted.
    Control(PromptCancellation),
}

/// One fresh accepted prompt response, still awaiting authoritative settlement.
#[derive(Debug, Eq, PartialEq)]
pub enum PromptAcceptance {
    /// Bounded user input accepted without granting authority.
    UserInput(PromptAnswer),
    /// Signed approval input strictly authenticated by B1.
    Approval(AuthenticatedApprovalResponse),
    /// Unprivileged cancellation accepted for the exact prompt.
    Cancelled(PromptCancellationAcceptance),
}

/// Move-only prepared response plus the inert token needed after durable settlement.
///
/// Preparing a response performs every freshness, protocol, and signature check but does not
/// change broker state. The authority owner consumes the acceptance while committing its target,
/// then supplies the separate token to `PromptBroker::commit_settlement`.
#[derive(Debug, Eq, PartialEq)]
pub struct PreparedPromptResponse {
    acceptance: PromptAcceptance,
    settlement: PromptSettlementToken,
}

impl PreparedPromptResponse {
    pub(super) const fn new(
        acceptance: PromptAcceptance,
        settlement: PromptSettlementToken,
    ) -> Self {
        Self { acceptance, settlement }
    }

    /// Consumes the prepared response into its target payload and post-commit broker token.
    #[must_use]
    pub fn into_parts(self) -> (PromptAcceptance, PromptSettlementToken) {
        (self.acceptance, self.settlement)
    }
}

/// Inert exact response retained while the authority owner performs durable settlement.
///
/// This token grants no approval or target authority. It can only terminalize the matching broker
/// entry after its caller has established the corresponding durable result.
#[derive(Debug, Eq, PartialEq)]
pub struct PromptSettlementToken {
    response: PromptSettlementResponse,
}

impl PromptSettlementToken {
    pub(super) const fn answer(answer: PromptAnswer) -> Self {
        Self { response: PromptSettlementResponse::Answer(answer) }
    }

    pub(super) const fn cancellation(cancellation: PromptCancellation) -> Self {
        Self { response: PromptSettlementResponse::Cancellation(cancellation) }
    }

    pub(super) const fn correlation(&self) -> PromptCorrelation {
        match &self.response {
            PromptSettlementResponse::Answer(answer) => answer.correlation(),
            PromptSettlementResponse::Cancellation(cancellation) => cancellation.correlation(),
        }
    }

    pub(super) const fn response(&self) -> &PromptSettlementResponse {
        &self.response
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum PromptSettlementResponse {
    Answer(PromptAnswer),
    Cancellation(PromptCancellation),
}

/// Lifecycle retained by the broker until AuthorityOwner retires the entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromptTerminalStatus {
    /// No terminal response has been accepted.
    AwaitingAnswer,
    /// An answer, including an approval-answer cancellation, was accepted.
    Answered,
    /// The dedicated cancellation request was accepted.
    Cancelled,
}
