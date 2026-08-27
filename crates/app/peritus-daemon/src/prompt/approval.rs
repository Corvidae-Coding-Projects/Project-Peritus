//! Exact B1 challenge binding and signed-decision authentication.

use peritus_app_protocol::{ApprovalAnswer, PromptAnswerPayload, PromptBinding, PromptKind};
use peritus_approval::{
    ApprovalRequest, AuthenticatedApprovalObservation, CredentialRegistrySnapshot,
    SignedApprovalDecision, decode_approval_request, decode_signed_decision,
    verify_signed_decision,
};
use peritus_codec::{CodecLimits, decode_frame};
use peritus_journal::{CurrentAuthorityEpoch, CurrentCredentialRegistry};
use peritus_policy::AuthorityInstant;

use super::{PromptAdmission, PromptBrokerError, PromptBrokerErrorKind};

/// Current durable authority observations injected by `AuthorityOwner` for signed approval input.
#[derive(Clone, Copy)]
pub struct CurrentApprovalAuthority<'a> {
    epoch: &'a CurrentAuthorityEpoch,
    current_registry: &'a CurrentCredentialRegistry,
    registry: &'a CredentialRegistrySnapshot,
    observed_at: AuthorityInstant,
}

impl<'a> CurrentApprovalAuthority<'a> {
    /// Groups one same-turn current epoch, registry row, decoded snapshot, and monotonic time.
    #[must_use]
    pub const fn new(
        epoch: &'a CurrentAuthorityEpoch,
        current_registry: &'a CurrentCredentialRegistry,
        registry: &'a CredentialRegistrySnapshot,
        observed_at: AuthorityInstant,
    ) -> Self {
        Self { epoch, current_registry, registry, observed_at }
    }
}

pub(super) enum PreparedAnswer {
    UserInput,
    Cancelled,
    Approval {
        request: ApprovalRequest,
        signed: SignedApprovalDecision,
        observation: AuthenticatedApprovalObservation,
    },
}

pub(super) fn validate_binding(binding: &PromptBinding) -> Result<(), PromptBrokerError> {
    if binding.kind() != PromptKind::Approval {
        return Ok(());
    }
    let challenge = binding.approval_challenge().ok_or_else(challenge_missing)?;
    let request = decode_approval_request(challenge.request_frame()).map_err(challenge_error)?;
    if request.scope().revision() != binding.correlation().revision()
        || request.digest().sha256() != binding.correlation().freshness_digest()
    {
        return Err(PromptBrokerError::new(
            PromptBrokerErrorKind::ApprovalChallenge,
            "approval request is not bound to the prompt revision and freshness digest",
        ));
    }
    Ok(())
}

pub(super) fn prepare(
    binding: &PromptBinding,
    payload: &PromptAnswerPayload,
    admission: PromptAdmission,
    authority: Option<CurrentApprovalAuthority<'_>>,
) -> Result<PreparedAnswer, PromptBrokerError> {
    match payload {
        PromptAnswerPayload::UserInput(_) => Ok(PreparedAnswer::UserInput),
        PromptAnswerPayload::Approval { answer: ApprovalAnswer::Cancel, .. } => {
            Ok(PreparedAnswer::Cancelled)
        }
        PromptAnswerPayload::Approval { answer: ApprovalAnswer::SignedDecision(frame), .. } => {
            authenticate(binding, frame.bytes(), admission, authority)
        }
    }
}

fn authenticate(
    binding: &PromptBinding,
    decision_frame: &[u8],
    admission: PromptAdmission,
    authority: Option<CurrentApprovalAuthority<'_>>,
) -> Result<PreparedAnswer, PromptBrokerError> {
    let authority = authority.ok_or_else(|| {
        PromptBrokerError::new(
            PromptBrokerErrorKind::ApprovalAuthorityMissing,
            "signed approval requires current durable authority observations",
        )
    })?;
    let challenge = binding.approval_challenge().ok_or_else(challenge_missing)?;
    let request = decode_approval_request(challenge.request_frame()).map_err(challenge_error)?;
    let canonical_registry = authority.registry.canonical_bytes().map_err(authentication_error)?;
    let registry_digest = authority.registry.digest().map_err(authentication_error)?;
    let current_frame =
        decode_frame(authority.current_registry.snapshot_bytes(), CodecLimits::PRODUCTION)
            .map_err(|_| stale_registry())?;
    if challenge.registry_revision().get() != authority.current_registry.revision()
        || authority.registry.revision().get() != authority.current_registry.revision()
        || registry_digest != authority.current_registry.digest()
        || current_frame.payload() != canonical_registry.as_slice()
    {
        return Err(stale_registry());
    }
    let epoch = authority.epoch.get();
    if authority.observed_at.epoch().get() != epoch
        || request.authority_time().epoch().get() != epoch
    {
        return Err(PromptBrokerError::new(
            PromptBrokerErrorKind::StaleAuthorityEpoch,
            "approval prompt does not match the current durable authority epoch",
        ));
    }
    let signed = decode_signed_decision(decision_frame).map_err(authentication_error)?;
    if signed.decision().command_id() != challenge.decision_command_id()
        || signed.decision().responder() != admission.actor_id()
    {
        return Err(PromptBrokerError::new(
            PromptBrokerErrorKind::BindingMismatch,
            "signed decision command or responder does not match the prompt owner",
        ));
    }
    let observation =
        verify_signed_decision(&request, &signed, authority.registry, authority.observed_at)
            .map_err(authentication_error)?;
    Ok(PreparedAnswer::Approval { request, signed, observation })
}

const fn challenge_missing() -> PromptBrokerError {
    PromptBrokerError::new(
        PromptBrokerErrorKind::ApprovalChallenge,
        "approval prompt has no exact B1 challenge",
    )
}

const fn challenge_error(error: peritus_approval::ApprovalError) -> PromptBrokerError {
    PromptBrokerError::approval(
        PromptBrokerErrorKind::ApprovalChallenge,
        "approval challenge is not a canonical B1 request",
        error,
    )
}

const fn authentication_error(error: peritus_approval::ApprovalError) -> PromptBrokerError {
    PromptBrokerError::approval(
        PromptBrokerErrorKind::ApprovalAuthentication,
        "B1 rejected signed approval authentication",
        error,
    )
}

const fn stale_registry() -> PromptBrokerError {
    PromptBrokerError::new(
        PromptBrokerErrorKind::StaleCredentialRegistry,
        "approval prompt does not match the current durable credential registry",
    )
}
