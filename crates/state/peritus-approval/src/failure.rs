//! Stable fail-closed approval failures and recovery guidance.

// Verus lowers documented payload variants to synthetic methods without carrying documentation.
// This module's public items are fully documented; this scopes the workaround to those artifacts.
#![allow(missing_docs)]

use vstd::prelude::*;

verus! {

/// Public approval lifecycle phase.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApprovalPhase {
    /// No decision has resolved the request.
    Pending,
    /// One approve-once decision is available for exact consumption.
    ApprovedOnce,
    /// One exact amendment has been authorized.
    AmendmentAuthorized,
    /// The approve-once decision was consumed.
    Consumed,
    /// The approved amendment was consumed by a logical amendment transition.
    Amended,
    /// The request was denied.
    Denied,
    /// The request expired before resolution or consumption.
    Expired,
    /// The request was explicitly cancelled.
    Cancelled,
}

/// Recovery guidance attached to an approval failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    /// The request cannot progress without new intent.
    Terminal,
    /// Current logical state must be observed again.
    Reobserve,
    /// Policy and human authority must be acquired again.
    Reauthorize,
    /// The caller may correct malformed or mismatched input.
    CallerCorrectable,
}

/// Canonical bounded collection rejected by a constructor.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CanonicalCollection {
    /// Producing-attempt participant actors.
    ProducingParticipants,
    /// Candidate-review participant actors.
    ReviewParticipants,
    /// Credential approval-role labels.
    CredentialApprovalRoles,
    /// Credential-registry entries.
    CredentialRegistry,
    /// Approval request permissions.
    Permissions,
    /// Structured request risk classes.
    Risks,
    /// Approval independence requirements.
    IndependenceRequirements,
}

/// Credential or registry dimension that failed exact validation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CredentialDimension {
    /// Public-key-derived identifier.
    KeyId,
    /// Credential registry snapshot revision.
    RegistryRevision,
    /// Credential revocation/reissue generation.
    Generation,
    /// Authenticated responder actor.
    Actor,
    /// Human credential principal role.
    PrincipalRole,
    /// Explicit signed approval role.
    ApprovalRole,
    /// Credential environment scope.
    Environment,
    /// Credential workspace scope.
    Workspace,
    /// Maximum credential authority tier.
    AuthorityTier,
    /// Credential validity window.
    Validity,
    /// Credential enabled/disabled status.
    Status,
}

/// Request, decision, or use scope dimension that failed exact matching.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ScopeDimension {
    /// Approval request identity.
    Request,
    /// Request digest.
    RequestDigest,
    /// Exact action identity.
    Action,
    /// Exact action digest.
    ActionDigest,
    /// Exact revision tuple.
    Revision,
    /// Sole policy identity.
    Policy,
    /// Decision command identity.
    Command,
    /// Decision choice or amendment identity.
    Choice,
}

/// Checked-constructor, authentication, reducer, and rendering failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApprovalError {
    /// A collection was empty although its contract requires a value.
    EmptyCollection(CanonicalCollection),
    /// A collection exceeded its exact public bound.
    CollectionTooLarge(CanonicalCollection),
    /// A collection contained a duplicate value.
    DuplicateCanonicalValue(CanonicalCollection),
    /// A collection was not supplied in strict canonical order.
    NonCanonicalOrder(CanonicalCollection),
    /// The exact canonical digest preimage exceeded its frozen bound.
    PreimageTooLarge,
    /// A public key or signature had the wrong byte length.
    InvalidCryptoLength,
    /// A public key or signature encoding was malformed or noncanonical.
    InvalidCryptoEncoding,
    /// Strict Ed25519 verification rejected the decision.
    SignatureInvalid,
    /// No credential with the signed key ID exists in the supplied snapshot.
    CredentialMissing,
    /// One credential or registry dimension mismatched.
    CredentialMismatch(CredentialDimension),
    /// One signed/request/state binding mismatched.
    BindingMismatch(ScopeDimension),
    /// The request digest does not match its exact semantic fields.
    RequestDigestMismatch,
    /// The decision digest does not match its exact signed fields.
    DecisionDigestMismatch,
    /// The authority-clock epoch changed.
    ClockEpochMismatch,
    /// The authority observation regressed.
    ClockRegression,
    /// A validity interval has not started.
    NotYetValid,
    /// A request, credential, challenge, or decision expired.
    Expired,
    /// The responder violated an exact independence requirement.
    IndependenceViolation,
    /// A command was attempted in the wrong lifecycle phase.
    IllegalPhase {
        /// Required phase.
        expected: ApprovalPhase,
        /// Actual phase.
        actual: ApprovalPhase,
    },
    /// A terminal request received a semantically conflicting response.
    AlreadyResolved,
    /// An approve-once grant was already consumed.
    AlreadyConsumed,
    /// Rendering could not preserve its safe bounded representation.
    UnsafeRenderingInput,
    /// Internal checked state did not satisfy its representation invariant.
    CorruptState,
}

impl ApprovalError {
    /// Returns the stable subsystem diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::EmptyCollection(_) => "PERITUS-APPROVAL-INPUT-001",
            Self::CollectionTooLarge(_) => "PERITUS-APPROVAL-INPUT-002",
            Self::DuplicateCanonicalValue(_) => "PERITUS-APPROVAL-INPUT-003",
            Self::NonCanonicalOrder(_) => "PERITUS-APPROVAL-INPUT-004",
            Self::PreimageTooLarge => "PERITUS-APPROVAL-DIGEST-001",
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

    /// Returns the recovery classification.
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

/// Rejected move-only state transition preserving the unchanged aggregate and observation.
#[derive(Debug, Eq, PartialEq)]
pub struct ApprovalTransitionFailure {
    pub(crate) error: ApprovalError,
    pub(crate) aggregate: crate::ApprovalAggregate,
    pub(crate) observation: Option<crate::AuthenticatedApprovalObservation>,
}

impl ApprovalTransitionFailure {
    /// Returns exact aggregate preservation for formal reducer contracts.
    pub closed spec fn spec_preserves_aggregate(
        &self,
        previous: &crate::ApprovalAggregate,
    ) -> bool {
        self.aggregate == *previous
    }

    /// Returns the exact rejected authentication observation.
    pub closed spec fn spec_observation(
        &self,
    ) -> Option<crate::AuthenticatedApprovalObservation> {
        self.observation
    }

    pub(crate) const fn new(
        error: ApprovalError,
        aggregate: crate::ApprovalAggregate,
        observation: Option<crate::AuthenticatedApprovalObservation>,
    ) -> (failure: Self)
        ensures
            failure.error == error,
            failure.aggregate == aggregate,
            failure.observation == observation,
            failure.spec_preserves_aggregate(&aggregate),
            failure.spec_observation() == observation,
    {
        Self { error, aggregate, observation }
    }

    pub(crate) proof fn prove_preserves(&self, previous: &crate::ApprovalAggregate)
        requires self.spec_preserves_aggregate(previous),
        ensures self.aggregate == *previous,
    {
    }

    /// Borrows the exact error.
    #[must_use]
    pub const fn error(&self) -> &ApprovalError { &self.error }

    /// Borrows the unchanged aggregate.
    #[must_use]
    pub const fn aggregate(&self) -> &crate::ApprovalAggregate { &self.aggregate }

    /// Consumes the failure into its exact parts.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (ApprovalError, crate::ApprovalAggregate, Option<crate::AuthenticatedApprovalObservation>) {
        (self.error, self.aggregate, self.observation)
    }
}

/// Rejected approve-once use preserving the unchanged aggregate.
#[derive(Debug, Eq, PartialEq)]
pub struct ApprovalUseFailure {
    pub(crate) error: ApprovalError,
    pub(crate) aggregate: crate::ApprovalAggregate,
}

impl ApprovalUseFailure {
    /// Returns exact aggregate preservation for formal use-reducer contracts.
    pub closed spec fn spec_preserves_aggregate(
        &self,
        previous: &crate::ApprovalAggregate,
    ) -> bool {
        self.aggregate == *previous
    }

    pub(crate) const fn new(
        error: ApprovalError,
        aggregate: crate::ApprovalAggregate,
    ) -> (failure: Self)
        ensures
            failure.error == error,
            failure.aggregate == aggregate,
            failure.spec_preserves_aggregate(&aggregate),
    {
        Self { error, aggregate }
    }

    pub(crate) proof fn prove_preserves(&self, previous: &crate::ApprovalAggregate)
        requires self.spec_preserves_aggregate(previous),
        ensures self.aggregate == *previous,
    {
    }

    /// Borrows the exact error.
    #[must_use]
    pub const fn error(&self) -> &ApprovalError { &self.error }

    /// Borrows the exact unchanged aggregate.
    #[must_use]
    pub const fn aggregate(&self) -> &crate::ApprovalAggregate { &self.aggregate }

    /// Consumes the failure into its reason and unchanged aggregate.
    #[must_use]
    pub fn into_parts(self) -> (ApprovalError, crate::ApprovalAggregate) {
        (self.error, self.aggregate)
    }
}

} // verus!
