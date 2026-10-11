//! Stable typed failures for checked policy inputs and reducers.

#[doc = " Recovery guidance attached to every policy failure."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    #[doc = " The request cannot succeed without changing its intent or authoritative configuration."]
    Terminal,
    #[doc = " The caller must obtain a fresh authority observation before retrying."]
    Reobserve,
    #[doc = " The caller must evaluate policy again against current state."]
    Reauthorize,
    #[doc = " The caller supplied malformed or noncanonical input and may correct it."]
    CallerCorrectable,
}
#[doc = " Canonical collection whose checked constructor rejected input."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CanonicalCollection {
    #[doc = " Actor identifiers."]
    Actors,
    #[doc = " Security roles."]
    Roles,
    #[doc = " Environment identifiers."]
    Environments,
    #[doc = " Exact permission pairs."]
    Permissions,
    #[doc = " Operation descriptors."]
    Operations,
    #[doc = " Risk classes."]
    Risks,
    #[doc = " Independence requirements."]
    IndependenceRequirements,
    #[doc = " Restriction rules."]
    RestrictionRules,
    #[doc = " Authority-ceiling grants."]
    Grants,
    #[doc = " Policy restriction layers."]
    RestrictionLayers,
}
#[doc = " Exact scope dimension that did not match a capability or authority boundary."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ScopeDimension {
    #[doc = " Actor identity."]
    Actor,
    #[doc = " Security role."]
    Role,
    #[doc = " Environment identity."]
    Environment,
    #[doc = " Exact resource/capability permission pairs."]
    Permissions,
    #[doc = " Complete immutable revision tuple."]
    Revision,
}
#[doc = " Stable category for a checked policy failure."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PolicyErrorKind {
    #[doc = " A required canonical collection was empty."]
    EmptyCanonicalCollection,
    #[doc = " Values were not in canonical ascending order."]
    NonCanonicalOrder,
    #[doc = " A canonical collection contained an exact duplicate."]
    DuplicateCanonicalValue,
    #[doc = " A validity window crossed epochs or was empty."]
    InvalidValidityWindow,
    #[doc = " Authority time was compared across epochs."]
    ClockEpochMismatch,
    #[doc = " Authority time regressed within one epoch."]
    ClockRegression,
    #[doc = " Authority-time arithmetic overflowed."]
    TimeOverflow,
    #[doc = " A requested limited use count was zero."]
    ZeroUseLimit,
    #[doc = " A selector exceeded its containing boundary."]
    SelectorOutsideBoundary,
    #[doc = " Policy tiers were duplicated or out of order."]
    InvalidPolicyTier,
    #[doc = " A rule appeared in a collection for another kind."]
    InvalidRuleKind,
    #[doc = " A policy definition identity differed from its sole revision-tuple policy identity."]
    PolicyRevisionMismatch,
    #[doc = " An operation descriptor omitted its mandatory security risk classification."]
    InvalidOperationRisk,
    #[doc = " An amendment targeted the wrong base policy."]
    AmendmentBaseMismatch,
    #[doc = " An amendment reused its base policy identity."]
    AmendmentPolicyIdReuse,
    #[doc = " An amendment replacement had the wrong tier."]
    AmendmentTierMismatch,
    #[doc = " A capability use differed on one exact scope dimension."]
    CapabilityScopeMismatch,
    #[doc = " A limited capability had no remaining uses."]
    CapabilityExhausted,
    #[doc = " A capability was used before its validity interval."]
    CapabilityNotYetValid,
    #[doc = " A capability was used at or after expiry."]
    CapabilityExpired,
}
#[doc = " Failure returned by checked policy constructors and logical reducers."]
#[doc = ""]
#[doc = " Construction is closed so a category can only carry its corresponding collection or scope"]
#[doc = " detail. This avoids invalid public error states while keeping the value copyable and stable."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PolicyError {
    kind: PolicyErrorKind,
    collection: Option<CanonicalCollection>,
    dimension: Option<ScopeDimension>,
}
mod metadata;
impl PolicyError {
    const fn plain(kind: PolicyErrorKind) -> Self {
        Self { kind, collection: None, dimension: None }
    }
    #[doc = " Creates a canonical empty-collection failure with exact collection detail."]
    #[must_use]
    pub const fn empty_canonical_collection(collection: CanonicalCollection) -> Self {
        Self {
            kind: PolicyErrorKind::EmptyCanonicalCollection,
            collection: Some(collection),
            dimension: None,
        }
    }
    #[doc = " Creates a canonical-order failure with exact collection detail."]
    #[must_use]
    pub const fn non_canonical_order(collection: CanonicalCollection) -> Self {
        Self {
            kind: PolicyErrorKind::NonCanonicalOrder,
            collection: Some(collection),
            dimension: None,
        }
    }
    #[doc = " Creates a duplicate-value failure with exact collection detail."]
    #[must_use]
    pub const fn duplicate_canonical_value(collection: CanonicalCollection) -> Self {
        Self {
            kind: PolicyErrorKind::DuplicateCanonicalValue,
            collection: Some(collection),
            dimension: None,
        }
    }
    #[doc = " Creates a selector-containment failure with exact dimension detail."]
    #[must_use]
    pub const fn selector_outside_boundary(dimension: ScopeDimension) -> Self {
        Self {
            kind: PolicyErrorKind::SelectorOutsideBoundary,
            collection: None,
            dimension: Some(dimension),
        }
    }
    #[doc = " Creates a capability-scope failure with exact dimension detail."]
    #[must_use]
    pub const fn capability_scope_mismatch(dimension: ScopeDimension) -> Self {
        Self {
            kind: PolicyErrorKind::CapabilityScopeMismatch,
            collection: None,
            dimension: Some(dimension),
        }
    }
    #[doc = " Creates an invalid validity-window failure."]
    #[must_use]
    pub const fn invalid_validity_window() -> Self {
        Self::plain(PolicyErrorKind::InvalidValidityWindow)
    }
    #[doc = " Creates a cross-epoch authority-time failure."]
    #[must_use]
    pub const fn clock_epoch_mismatch() -> Self {
        Self::plain(PolicyErrorKind::ClockEpochMismatch)
    }
    #[doc = " Creates an authority-time regression failure."]
    #[must_use]
    pub const fn clock_regression() -> Self {
        Self::plain(PolicyErrorKind::ClockRegression)
    }
    #[doc = " Creates an authority-time overflow failure."]
    #[must_use]
    pub const fn time_overflow() -> Self {
        Self::plain(PolicyErrorKind::TimeOverflow)
    }
    #[doc = " Creates a zero-use-limit failure."]
    #[must_use]
    pub const fn zero_use_limit() -> Self {
        Self::plain(PolicyErrorKind::ZeroUseLimit)
    }
    #[doc = " Creates an invalid policy-tier failure."]
    #[must_use]
    pub const fn invalid_policy_tier() -> Self {
        Self::plain(PolicyErrorKind::InvalidPolicyTier)
    }
    #[doc = " Creates an invalid restriction-rule-kind failure."]
    #[must_use]
    pub const fn invalid_rule_kind() -> Self {
        Self::plain(PolicyErrorKind::InvalidRuleKind)
    }
    #[doc = " Creates a policy/revision identity mismatch failure."]
    #[must_use]
    pub const fn policy_revision_mismatch() -> Self {
        Self::plain(PolicyErrorKind::PolicyRevisionMismatch)
    }
    #[doc = " Creates an invalid operation/risk classification failure."]
    #[must_use]
    pub const fn invalid_operation_risk() -> Self {
        Self::plain(PolicyErrorKind::InvalidOperationRisk)
    }
    #[doc = " Creates an amendment base-policy mismatch."]
    #[must_use]
    pub const fn amendment_base_mismatch() -> Self {
        Self::plain(PolicyErrorKind::AmendmentBaseMismatch)
    }
    #[doc = " Creates an amendment policy-identity reuse failure."]
    #[must_use]
    pub const fn amendment_policy_id_reuse() -> Self {
        Self::plain(PolicyErrorKind::AmendmentPolicyIdReuse)
    }
    #[doc = " Creates an amendment tier mismatch."]
    #[must_use]
    pub const fn amendment_tier_mismatch() -> Self {
        Self::plain(PolicyErrorKind::AmendmentTierMismatch)
    }
    #[doc = " Creates an exhausted-capability failure."]
    #[must_use]
    pub const fn capability_exhausted() -> Self {
        Self::plain(PolicyErrorKind::CapabilityExhausted)
    }
    #[doc = " Creates a not-yet-valid capability failure."]
    #[must_use]
    pub const fn capability_not_yet_valid() -> Self {
        Self::plain(PolicyErrorKind::CapabilityNotYetValid)
    }
    #[doc = " Creates an expired-capability failure."]
    #[must_use]
    pub const fn capability_expired() -> Self {
        Self::plain(PolicyErrorKind::CapabilityExpired)
    }
    #[doc = " Creates a detail-free failure of the requested stable category."]
    #[doc = ""]
    #[doc = " Detail-bearing categories are rejected so callers cannot manufacture an incomplete error."]
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
    #[doc = " Returns the stable typed category."]
    #[must_use]
    pub const fn kind(&self) -> PolicyErrorKind {
        self.kind
    }
    #[doc = " Returns exact canonical-collection detail when the category requires it."]
    #[must_use]
    pub const fn collection(&self) -> Option<CanonicalCollection> {
        self.collection
    }
    #[doc = " Returns exact scope-dimension detail when the category requires it."]
    #[must_use]
    pub const fn dimension(&self) -> Option<ScopeDimension> {
        self.dimension
    }
}
