//! Stable fail-closed approval failures and recovery guidance.

// Verus lowers documented payload variants to synthetic methods without carrying documentation.
// This module's public items are fully documented; this scopes the workaround to those artifacts.
#![allow(missing_docs)]

#[doc = " Public approval lifecycle phase."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApprovalPhase {
    #[doc = " No decision has resolved the request."]
    Pending,
    #[doc = " One approve-once decision is available for exact consumption."]
    ApprovedOnce,
    #[doc = " One exact amendment has been authorized."]
    AmendmentAuthorized,
    #[doc = " The approve-once decision was consumed."]
    Consumed,
    #[doc = " The approved amendment was consumed by a logical amendment transition."]
    Amended,
    #[doc = " The request was denied."]
    Denied,
    #[doc = " The request expired before resolution or consumption."]
    Expired,
    #[doc = " The request was explicitly cancelled."]
    Cancelled,
}
#[doc = " Recovery guidance attached to an approval failure."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    #[doc = " The request cannot progress without new intent."]
    Terminal,
    #[doc = " Current logical state must be observed again."]
    Reobserve,
    #[doc = " Policy and human authority must be acquired again."]
    Reauthorize,
    #[doc = " The caller may correct malformed or mismatched input."]
    CallerCorrectable,
}
#[doc = " Canonical bounded collection rejected by a constructor."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CanonicalCollection {
    #[doc = " Producing-attempt participant actors."]
    ProducingParticipants,
    #[doc = " Candidate-review participant actors."]
    ReviewParticipants,
    #[doc = " Credential approval-role labels."]
    CredentialApprovalRoles,
    #[doc = " Credential-registry entries."]
    CredentialRegistry,
    #[doc = " Approval request permissions."]
    Permissions,
    #[doc = " Structured request risk classes."]
    Risks,
    #[doc = " Approval independence requirements."]
    IndependenceRequirements,
}
#[doc = " Credential or registry dimension that failed exact validation."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CredentialDimension {
    #[doc = " Public-key-derived identifier."]
    KeyId,
    #[doc = " Credential registry snapshot revision."]
    RegistryRevision,
    #[doc = " Credential revocation/reissue generation."]
    Generation,
    #[doc = " Authenticated responder actor."]
    Actor,
    #[doc = " Human credential principal role."]
    PrincipalRole,
    #[doc = " Explicit signed approval role."]
    ApprovalRole,
    #[doc = " Credential environment scope."]
    Environment,
    #[doc = " Credential workspace scope."]
    Workspace,
    #[doc = " Maximum credential authority tier."]
    AuthorityTier,
    #[doc = " Credential validity window."]
    Validity,
    #[doc = " Credential enabled/disabled status."]
    Status,
}
#[doc = " Request, decision, or use scope dimension that failed exact matching."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ScopeDimension {
    #[doc = " Approval request identity."]
    Request,
    #[doc = " Request digest."]
    RequestDigest,
    #[doc = " Exact action identity."]
    Action,
    #[doc = " Exact action digest."]
    ActionDigest,
    #[doc = " Exact revision tuple."]
    Revision,
    #[doc = " Sole policy identity."]
    Policy,
    #[doc = " Decision command identity."]
    Command,
    #[doc = " Decision choice or amendment identity."]
    Choice,
}
#[doc = " Checked-constructor, authentication, reducer, and rendering failure."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApprovalError {
    #[doc = " A collection was empty although its contract requires a value."]
    EmptyCollection(CanonicalCollection),
    #[doc = " A collection exceeded its exact public bound."]
    CollectionTooLarge(CanonicalCollection),
    #[doc = " A collection contained a duplicate value."]
    DuplicateCanonicalValue(CanonicalCollection),
    #[doc = " A collection was not supplied in strict canonical order."]
    NonCanonicalOrder(CanonicalCollection),
    #[doc = " The exact canonical digest preimage exceeded its frozen bound."]
    PreimageTooLarge,
    #[doc = " A bounded canonical authority frame was malformed, noncanonical, or had trailing bytes."]
    InvalidCanonicalEncoding,
    #[doc = " A public key or signature had the wrong byte length."]
    InvalidCryptoLength,
    #[doc = " A public key or signature encoding was malformed or noncanonical."]
    InvalidCryptoEncoding,
    #[doc = " Strict Ed25519 verification rejected the decision."]
    SignatureInvalid,
    #[doc = " No credential with the signed key ID exists in the supplied snapshot."]
    CredentialMissing,
    #[doc = " One credential or registry dimension mismatched."]
    CredentialMismatch(CredentialDimension),
    #[doc = " One signed/request/state binding mismatched."]
    BindingMismatch(ScopeDimension),
    #[doc = " The request digest does not match its exact semantic fields."]
    RequestDigestMismatch,
    #[doc = " The decision digest does not match its exact signed fields."]
    DecisionDigestMismatch,
    #[doc = " The authority-clock epoch changed."]
    ClockEpochMismatch,
    #[doc = " The authority observation regressed."]
    ClockRegression,
    #[doc = " A validity interval has not started."]
    NotYetValid,
    #[doc = " A request, credential, challenge, or decision expired."]
    Expired,
    #[doc = " The responder violated an exact independence requirement."]
    IndependenceViolation,
    #[doc = " A command was attempted in the wrong lifecycle phase."]
    IllegalPhase {
        #[doc = " Required phase."]
        expected: ApprovalPhase,
        #[doc = " Actual phase."]
        actual: ApprovalPhase,
    },
    #[doc = " A terminal request received a semantically conflicting response."]
    AlreadyResolved,
    #[doc = " An approve-once grant was already consumed."]
    AlreadyConsumed,
    #[doc = " Rendering could not preserve its safe bounded representation."]
    UnsafeRenderingInput,
    #[doc = " Internal checked state did not satisfy its representation invariant."]
    CorruptState,
}
impl ApprovalError {
    #[doc = " Returns the stable subsystem diagnostic code."]
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::EmptyCollection(_) => "PERITUS-APPROVAL-INPUT-001",
            Self::CollectionTooLarge(_) => "PERITUS-APPROVAL-INPUT-002",
            Self::DuplicateCanonicalValue(_) => "PERITUS-APPROVAL-INPUT-003",
            Self::NonCanonicalOrder(_) => "PERITUS-APPROVAL-INPUT-004",
            Self::PreimageTooLarge => "PERITUS-APPROVAL-DIGEST-001",
            Self::InvalidCanonicalEncoding => "PERITUS-APPROVAL-CODEC-001",
            Self::InvalidCryptoLength => "PERITUS-APPROVAL-AUTH-001",
            Self::InvalidCryptoEncoding => "PERITUS-APPROVAL-AUTH-002",
            Self::SignatureInvalid => "PERITUS-APPROVAL-AUTH-003",
            Self::CredentialMissing => "PERITUS-APPROVAL-CREDENTIAL-001",
            Self::CredentialMismatch(_) => "PERITUS-APPROVAL-CREDENTIAL-002",
            Self::BindingMismatch(_) => "PERITUS-APPROVAL-BINDING-001",
            Self::RequestDigestMismatch => "PERITUS-APPROVAL-DIGEST-002",
            Self::DecisionDigestMismatch => "PERITUS-APPROVAL-DIGEST-003",
            Self::ClockEpochMismatch => "PERITUS-APPROVAL-TIME-001",
            Self::ClockRegression => "PERITUS-APPROVAL-TIME-002",
            Self::NotYetValid => "PERITUS-APPROVAL-TIME-003",
            Self::Expired => "PERITUS-APPROVAL-TIME-004",
            Self::IndependenceViolation => "PERITUS-APPROVAL-INDEPENDENCE-001",
            Self::IllegalPhase { .. } => "PERITUS-APPROVAL-STATE-001",
            Self::AlreadyResolved => "PERITUS-APPROVAL-STATE-002",
            Self::AlreadyConsumed => "PERITUS-APPROVAL-STATE-003",
            Self::UnsafeRenderingInput => "PERITUS-APPROVAL-RENDER-001",
            Self::CorruptState => "PERITUS-APPROVAL-STATE-004",
        }
    }
    #[doc = " Returns the recovery classification."]
    #[must_use]
    pub const fn recovery(&self) -> RecoveryClass {
        match self {
            Self::IllegalPhase { .. } | Self::ClockRegression => RecoveryClass::Reobserve,
            Self::CredentialMissing
            | Self::CredentialMismatch(_)
            | Self::SignatureInvalid
            | Self::IndependenceViolation => RecoveryClass::Reauthorize,
            Self::EmptyCollection(_)
            | Self::CollectionTooLarge(_)
            | Self::DuplicateCanonicalValue(_)
            | Self::NonCanonicalOrder(_)
            | Self::PreimageTooLarge
            | Self::InvalidCanonicalEncoding
            | Self::InvalidCryptoLength
            | Self::InvalidCryptoEncoding
            | Self::BindingMismatch(_)
            | Self::RequestDigestMismatch
            | Self::DecisionDigestMismatch
            | Self::NotYetValid
            | Self::UnsafeRenderingInput => RecoveryClass::CallerCorrectable,
            Self::AlreadyResolved
            | Self::AlreadyConsumed
            | Self::Expired
            | Self::ClockEpochMismatch
            | Self::CorruptState => RecoveryClass::Terminal,
        }
    }
}
#[doc = " Rejected move-only state transition preserving the unchanged aggregate and observation."]
#[derive(Debug, Eq, PartialEq)]
pub struct ApprovalTransitionFailure {
    pub(crate) error: ApprovalError,
    pub(crate) aggregate: crate::ApprovalAggregate,
    pub(crate) observation: Option<crate::AuthenticatedApprovalObservation>,
}
impl ApprovalTransitionFailure {
    pub(crate) const fn new(
        error: ApprovalError,
        aggregate: crate::ApprovalAggregate,
        observation: Option<crate::AuthenticatedApprovalObservation>,
    ) -> Self {
        Self { error, aggregate, observation }
    }
    #[doc = " Borrows the exact error."]
    #[must_use]
    pub const fn error(&self) -> &ApprovalError {
        &self.error
    }
    #[doc = " Borrows the unchanged aggregate."]
    #[must_use]
    pub const fn aggregate(&self) -> &crate::ApprovalAggregate {
        &self.aggregate
    }
    #[doc = " Consumes the failure into its exact parts."]
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (ApprovalError, crate::ApprovalAggregate, Option<crate::AuthenticatedApprovalObservation>)
    {
        (self.error, self.aggregate, self.observation)
    }
}
#[doc = " Rejected approve-once use preserving the unchanged aggregate."]
#[derive(Debug, Eq, PartialEq)]
pub struct ApprovalUseFailure {
    pub(crate) error: ApprovalError,
    pub(crate) aggregate: crate::ApprovalAggregate,
}
impl ApprovalUseFailure {
    pub(crate) const fn new(error: ApprovalError, aggregate: crate::ApprovalAggregate) -> Self {
        Self { error, aggregate }
    }
    #[doc = " Borrows the exact error."]
    #[must_use]
    pub const fn error(&self) -> &ApprovalError {
        &self.error
    }
    #[doc = " Borrows the exact unchanged aggregate."]
    #[must_use]
    pub const fn aggregate(&self) -> &crate::ApprovalAggregate {
        &self.aggregate
    }
    #[doc = " Consumes the failure into its reason and unchanged aggregate."]
    #[must_use]
    pub fn into_parts(self) -> (ApprovalError, crate::ApprovalAggregate) {
        (self.error, self.aggregate)
    }
}
