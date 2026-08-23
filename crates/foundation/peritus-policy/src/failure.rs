//! Stable typed failures for checked policy inputs and reducers.

use vstd::prelude::*;

verus! {

/// Recovery guidance attached to every policy failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    /// The request cannot succeed without changing its intent or authoritative configuration.
    Terminal,
    /// The caller must obtain a fresh authority observation before retrying.
    Reobserve,
    /// The caller must evaluate policy again against current state.
    Reauthorize,
    /// The caller supplied malformed or noncanonical input and may correct it.
    CallerCorrectable,
}

/// Canonical collection whose checked constructor rejected input.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CanonicalCollection {
    /// Actor identifiers.
    Actors,
    /// Security roles.
    Roles,
    /// Environment identifiers.
    Environments,
    /// Exact permission pairs.
    Permissions,
    /// Operation descriptors.
    Operations,
    /// Risk classes.
    Risks,
    /// Independence requirements.
    IndependenceRequirements,
    /// Restriction rules.
    RestrictionRules,
    /// Authority-ceiling grants.
    Grants,
    /// Policy restriction layers.
    RestrictionLayers,
}

/// Exact scope dimension that did not match a capability or authority boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ScopeDimension {
    /// Actor identity.
    Actor,
    /// Security role.
    Role,
    /// Environment identity.
    Environment,
    /// Exact resource/capability permission pairs.
    Permissions,
    /// Complete immutable revision tuple.
    Revision,
}

/// Stable category for a checked policy failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PolicyErrorKind {
    /// A required canonical collection was empty.
    EmptyCanonicalCollection,
    /// Values were not in canonical ascending order.
    NonCanonicalOrder,
    /// A canonical collection contained an exact duplicate.
    DuplicateCanonicalValue,
    /// A validity window crossed epochs or was empty.
    InvalidValidityWindow,
    /// Authority time was compared across epochs.
    ClockEpochMismatch,
    /// Authority time regressed within one epoch.
    ClockRegression,
    /// Authority-time arithmetic overflowed.
    TimeOverflow,
    /// A requested limited use count was zero.
    ZeroUseLimit,
    /// A selector exceeded its containing boundary.
    SelectorOutsideBoundary,
    /// Policy tiers were duplicated or out of order.
    InvalidPolicyTier,
    /// A rule appeared in a collection for another kind.
    InvalidRuleKind,
    /// A policy definition identity differed from its sole revision-tuple policy identity.
    PolicyRevisionMismatch,
    /// An operation descriptor omitted its mandatory security risk classification.
    InvalidOperationRisk,
    /// An amendment targeted the wrong base policy.
    AmendmentBaseMismatch,
    /// An amendment reused its base policy identity.
    AmendmentPolicyIdReuse,
    /// An amendment replacement had the wrong tier.
    AmendmentTierMismatch,
    /// A capability use differed on one exact scope dimension.
    CapabilityScopeMismatch,
    /// A limited capability had no remaining uses.
    CapabilityExhausted,
    /// A capability was used before its validity interval.
    CapabilityNotYetValid,
    /// A capability was used at or after expiry.
    CapabilityExpired,
}

/// Failure returned by checked policy constructors and logical reducers.
///
/// Construction is closed so a category can only carry its corresponding collection or scope
/// detail. This avoids invalid public error states while keeping the value copyable and stable.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PolicyError {
    kind: PolicyErrorKind,
    collection: Option<CanonicalCollection>,
    dimension: Option<ScopeDimension>,
}

mod metadata;

impl PolicyError {
    /// Returns the exact stable error category used by specifications.
    pub closed spec fn spec_kind(&self) -> PolicyErrorKind { self.kind }

    /// Returns the exact scope-dimension detail used by specifications.
    pub closed spec fn spec_dimension(&self) -> Option<ScopeDimension> { self.dimension }

    /// Returns the exact canonical-collection detail used by specifications.
    pub closed spec fn spec_collection(&self) -> Option<CanonicalCollection> { self.collection }

    const fn plain(kind: PolicyErrorKind) -> (error: Self)
        ensures
            error.spec_kind() == kind,
            error.spec_dimension().is_none(),
            error.spec_collection().is_none(),
    {
        Self { kind, collection: None, dimension: None }
    }

    /// Creates a canonical empty-collection failure with exact collection detail.
    #[must_use]
    pub const fn empty_canonical_collection(collection: CanonicalCollection) -> (error: Self)
        ensures
            error.spec_kind() == PolicyErrorKind::EmptyCanonicalCollection,
            error.spec_collection() == Some(collection),
            error.spec_dimension().is_none(),
    {
        Self { kind: PolicyErrorKind::EmptyCanonicalCollection, collection: Some(collection), dimension: None }
    }

    /// Creates a canonical-order failure with exact collection detail.
    #[must_use]
    pub const fn non_canonical_order(collection: CanonicalCollection) -> (error: Self)
        ensures
            error.spec_kind() == PolicyErrorKind::NonCanonicalOrder,
            error.spec_collection() == Some(collection),
            error.spec_dimension().is_none(),
    {
        Self { kind: PolicyErrorKind::NonCanonicalOrder, collection: Some(collection), dimension: None }
    }

    /// Creates a duplicate-value failure with exact collection detail.
    #[must_use]
    pub const fn duplicate_canonical_value(collection: CanonicalCollection) -> (error: Self)
        ensures
            error.spec_kind() == PolicyErrorKind::DuplicateCanonicalValue,
            error.spec_collection() == Some(collection),
            error.spec_dimension().is_none(),
    {
        Self { kind: PolicyErrorKind::DuplicateCanonicalValue, collection: Some(collection), dimension: None }
    }

    /// Creates a selector-containment failure with exact dimension detail.
    #[must_use]
    pub const fn selector_outside_boundary(dimension: ScopeDimension) -> (error: Self)
        ensures
            error.spec_kind() == PolicyErrorKind::SelectorOutsideBoundary,
            error.spec_collection().is_none(),
            error.spec_dimension() == Some(dimension),
    {
        Self { kind: PolicyErrorKind::SelectorOutsideBoundary, collection: None, dimension: Some(dimension) }
    }

    /// Creates a capability-scope failure with exact dimension detail.
    #[must_use]
    pub const fn capability_scope_mismatch(dimension: ScopeDimension) -> (error: Self)
        ensures
            error.spec_kind() == PolicyErrorKind::CapabilityScopeMismatch,
            error.spec_dimension() == Some(dimension),
            error.spec_collection().is_none(),
    {
        Self { kind: PolicyErrorKind::CapabilityScopeMismatch, collection: None, dimension: Some(dimension) }
    }

    /// Creates an invalid validity-window failure.
    #[must_use]
    pub const fn invalid_validity_window() -> (error: Self)
        ensures error.spec_kind() == PolicyErrorKind::InvalidValidityWindow,
    { Self::plain(PolicyErrorKind::InvalidValidityWindow) }

    /// Creates a cross-epoch authority-time failure.
    #[must_use]
    pub const fn clock_epoch_mismatch() -> (error: Self)
        ensures
            error.spec_kind() == PolicyErrorKind::ClockEpochMismatch,
            error.spec_dimension().is_none(),
            error.spec_collection().is_none(),
    { Self::plain(PolicyErrorKind::ClockEpochMismatch) }

    /// Creates an authority-time regression failure.
    #[must_use]
    pub const fn clock_regression() -> (error: Self)
        ensures
            error.spec_kind() == PolicyErrorKind::ClockRegression,
            error.spec_dimension().is_none(),
            error.spec_collection().is_none(),
    { Self::plain(PolicyErrorKind::ClockRegression) }

    /// Creates an authority-time overflow failure.
    #[must_use]
    pub const fn time_overflow() -> (error: Self)
        ensures
            error.spec_kind() == PolicyErrorKind::TimeOverflow,
            error.spec_dimension().is_none(),
    { Self::plain(PolicyErrorKind::TimeOverflow) }

    /// Creates a zero-use-limit failure.
    #[must_use]
    pub const fn zero_use_limit() -> Self { Self::plain(PolicyErrorKind::ZeroUseLimit) }

    /// Creates an invalid policy-tier failure.
    #[must_use]
    pub const fn invalid_policy_tier() -> (error: Self)
        ensures
            error.spec_kind() == PolicyErrorKind::InvalidPolicyTier,
            error.spec_collection().is_none(),
            error.spec_dimension().is_none(),
    { Self::plain(PolicyErrorKind::InvalidPolicyTier) }

    /// Creates an invalid restriction-rule-kind failure.
    #[must_use]
    pub const fn invalid_rule_kind() -> (error: Self)
        ensures
            error.spec_kind() == PolicyErrorKind::InvalidRuleKind,
            error.spec_collection().is_none(),
            error.spec_dimension().is_none(),
    { Self::plain(PolicyErrorKind::InvalidRuleKind) }

    /// Creates a policy/revision identity mismatch failure.
    #[must_use]
    pub const fn policy_revision_mismatch() -> (error: Self)
        ensures
            error.spec_kind() == PolicyErrorKind::PolicyRevisionMismatch,
            error.spec_collection().is_none(),
            error.spec_dimension().is_none(),
    {
        Self::plain(PolicyErrorKind::PolicyRevisionMismatch)
    }

    /// Creates an invalid operation/risk classification failure.
    #[must_use]
    pub const fn invalid_operation_risk() -> (error: Self)
        ensures
            error.spec_kind() == PolicyErrorKind::InvalidOperationRisk,
            error.spec_dimension().is_none(),
            error.spec_collection().is_none(),
    {
        Self::plain(PolicyErrorKind::InvalidOperationRisk)
    }

    /// Creates an amendment base-policy mismatch.
    #[must_use]
    pub const fn amendment_base_mismatch() -> (error: Self)
        ensures
            error.spec_kind() == PolicyErrorKind::AmendmentBaseMismatch,
            error.spec_dimension().is_none(),
            error.spec_collection().is_none(),
    { Self::plain(PolicyErrorKind::AmendmentBaseMismatch) }

    /// Creates an amendment policy-identity reuse failure.
    #[must_use]
    pub const fn amendment_policy_id_reuse() -> (error: Self)
        ensures
            error.spec_kind() == PolicyErrorKind::AmendmentPolicyIdReuse,
            error.spec_dimension().is_none(),
            error.spec_collection().is_none(),
    { Self::plain(PolicyErrorKind::AmendmentPolicyIdReuse) }

    /// Creates an amendment tier mismatch.
    #[must_use]
    pub const fn amendment_tier_mismatch() -> (error: Self)
        ensures
            error.spec_kind() == PolicyErrorKind::AmendmentTierMismatch,
            error.spec_dimension().is_none(),
            error.spec_collection().is_none(),
    { Self::plain(PolicyErrorKind::AmendmentTierMismatch) }

    /// Creates an exhausted-capability failure.
    #[must_use]
    pub const fn capability_exhausted() -> (error: Self)
        ensures
            error.spec_kind() == PolicyErrorKind::CapabilityExhausted,
            error.spec_dimension().is_none(),
            error.spec_collection().is_none(),
    { Self::plain(PolicyErrorKind::CapabilityExhausted) }

    /// Creates a not-yet-valid capability failure.
    #[must_use]
    pub const fn capability_not_yet_valid() -> (error: Self)
        ensures
            error.spec_kind() == PolicyErrorKind::CapabilityNotYetValid,
            error.spec_dimension().is_none(),
            error.spec_collection().is_none(),
    { Self::plain(PolicyErrorKind::CapabilityNotYetValid) }

    /// Creates an expired-capability failure.
    #[must_use]
    pub const fn capability_expired() -> (error: Self)
        ensures
            error.spec_kind() == PolicyErrorKind::CapabilityExpired,
            error.spec_dimension().is_none(),
            error.spec_collection().is_none(),
    { Self::plain(PolicyErrorKind::CapabilityExpired) }

    /// Creates a detail-free failure of the requested stable category.
    ///
    /// Detail-bearing categories are rejected so callers cannot manufacture an incomplete error.
    #[must_use]
    pub const fn from_kind(kind: PolicyErrorKind) -> Option<Self> {
        match kind {
            PolicyErrorKind::EmptyCanonicalCollection
            | PolicyErrorKind::NonCanonicalOrder
            | PolicyErrorKind::DuplicateCanonicalValue
            | PolicyErrorKind::SelectorOutsideBoundary
            | PolicyErrorKind::CapabilityScopeMismatch => None,
            _ => Some(Self::plain(kind)),
        }
    }

    /// Returns the stable typed category.
    #[must_use]
    pub const fn kind(&self) -> (kind: PolicyErrorKind)
        ensures kind == self.spec_kind(),
    { self.kind }

    /// Returns exact canonical-collection detail when the category requires it.
    #[must_use]
    pub const fn collection(&self) -> Option<CanonicalCollection> { self.collection }

    /// Returns exact scope-dimension detail when the category requires it.
    #[must_use]
    pub const fn dimension(&self) -> (dimension: Option<ScopeDimension>)
        ensures dimension == self.spec_dimension(),
    { self.dimension }

}

} // verus!
