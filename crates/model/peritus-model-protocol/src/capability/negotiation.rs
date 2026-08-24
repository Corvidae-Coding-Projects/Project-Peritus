//! Pure checked capability and limit negotiation.

use peritus_types::ProviderProfileId;

use super::{Capability, ModelLimits, ProviderProfile};
use crate::{ProtocolError, ProtocolErrorKind};

/// Required/optional feature request plus caller ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestedCapabilities {
    required: u64,
    optional: u64,
    limits: ModelLimits,
}

impl RequestedCapabilities {
    /// Creates a feature request.
    ///
    /// # Errors
    ///
    /// Rejects a feature listed as both required and optional.
    pub fn new(
        required: &[Capability],
        optional: &[Capability],
        limits: ModelLimits,
    ) -> Result<Self, ProtocolError> {
        let required = mask(required);
        let optional = mask(optional);
        if required & optional != 0 {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidRequest,
                "requested_capabilities",
                "a capability cannot be both required and optional",
            ));
        }
        Ok(Self { required, optional, limits })
    }
}

/// Exact result of profile/request intersection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NegotiatedCapabilities {
    profile_id: ProviderProfileId,
    profile_revision: u64,
    selected: u64,
    limits: ModelLimits,
}

impl NegotiatedCapabilities {
    /// Returns the exact profile identity used for negotiation.
    #[must_use]
    pub const fn profile_id(self) -> ProviderProfileId {
        self.profile_id
    }

    /// Returns the exact profile revision used for negotiation.
    #[must_use]
    pub const fn profile_revision(self) -> u64 {
        self.profile_revision
    }

    /// Returns whether a feature was selected.
    #[must_use]
    pub const fn includes(self, capability: Capability) -> bool {
        self.selected & capability.bit() != 0
    }

    /// Returns narrowed limits.
    #[must_use]
    pub const fn limits(self) -> ModelLimits {
        self.limits
    }

    pub(crate) const fn selected_mask(self) -> u64 {
        self.selected
    }

    pub(crate) fn from_canonical(
        profile: &ProviderProfile,
        selected: u64,
        limits: ModelLimits,
    ) -> Result<Self, ProtocolError> {
        if selected & !Capability::known_mask() != 0
            || !crate::verified::capability_mask_legal(
                selected,
                profile.capabilities().supported_mask(),
            )
            || !limits.is_within(profile.limits())
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidRequest,
                "negotiated_capabilities",
                "canonical capabilities or limits contradict the supplied profile",
            ));
        }
        Ok(Self {
            profile_id: profile.profile_id(),
            profile_revision: profile.revision(),
            selected,
            limits,
        })
    }
}

/// Computes the pure fail-closed profile/request intersection.
///
/// # Errors
///
/// Rejects the first required feature not proven supported.
pub fn negotiate(
    profile: &ProviderProfile,
    requested: RequestedCapabilities,
) -> Result<NegotiatedCapabilities, ProtocolError> {
    let supported = profile.capabilities().supported_mask();
    if !crate::verified::capability_mask_legal(requested.required, supported) {
        for capability in Capability::ALL {
            if requested.required & capability.bit() == 0
                || profile.capabilities().supports(capability)
            {
                continue;
            }
            return Err(ProtocolError::at(
                ProtocolErrorKind::UnsupportedCapability,
                capability.name(),
                "required provider capability is not proven supported",
            ));
        }
        return Err(ProtocolError::at(
            ProtocolErrorKind::UnsupportedCapability,
            "requested_capabilities",
            "required capability mask is not a subset of supported capabilities",
        ));
    }
    let wanted = requested.required | requested.optional;
    Ok(NegotiatedCapabilities {
        profile_id: profile.profile_id(),
        profile_revision: profile.revision(),
        selected: wanted & supported,
        limits: profile.limits().intersect(requested.limits),
    })
}

fn mask(capabilities: &[Capability]) -> u64 {
    capabilities.iter().fold(0, |value, capability| value | capability.bit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CancellationKind, CapabilityMatrix, CapabilityProvenance, ModelName,
        OutputLimitEnforcement, ProviderName, ResumeKind, StateMode, WireDialect,
    };

    #[test]
    fn unknown_does_not_satisfy_required_capability() {
        let matrix = CapabilityMatrix::new(&[Capability::Streaming], &[Capability::ToolCalls])
            .expect("valid matrix");
        let limits = ModelLimits::new(1_000, 100, 4, 2, 1_024).expect("valid limits");
        let profile = ProviderProfile::new(
            ProviderProfileId::new([7; 16]).expect("profile ID"),
            1,
            ProviderName::new("test".to_owned()).expect("provider"),
            ModelName::new("model".to_owned()).expect("model"),
            WireDialect::CompatibleResponses,
            matrix,
            CapabilityProvenance::Probed,
            limits,
            OutputLimitEnforcement::ProviderEnforced,
            StateMode::StatelessReplay,
            ResumeKind::Unsupported,
            CancellationKind::BestEffortLocalAbort,
        )
        .expect("valid profile");
        let requested = RequestedCapabilities::new(&[Capability::ToolCalls], &[], limits)
            .expect("valid request");
        assert_eq!(
            negotiate(&profile, requested).expect_err("unknown must fail").kind(),
            ProtocolErrorKind::UnsupportedCapability
        );
    }

    #[test]
    fn optional_features_and_limits_are_intersected() {
        let matrix = CapabilityMatrix::new(&[Capability::Streaming, Capability::ToolCalls], &[])
            .expect("valid matrix");
        let profile_limits = ModelLimits::new(1_000, 100, 4, 2, 1_024).expect("profile limits");
        let profile = ProviderProfile::new(
            ProviderProfileId::new([8; 16]).expect("profile ID"),
            2,
            ProviderName::new("test".to_owned()).expect("provider"),
            ModelName::new("model".to_owned()).expect("model"),
            WireDialect::CompatibleChatCompletions,
            matrix,
            CapabilityProvenance::Profiled,
            profile_limits,
            OutputLimitEnforcement::ProviderEnforced,
            StateMode::StatelessReplay,
            ResumeKind::Unsupported,
            CancellationKind::BestEffortLocalAbort,
        )
        .expect("valid profile");
        let requested_limits = ModelLimits::new(500, 200, 2, 1, 2_048).expect("request limits");
        let requested = RequestedCapabilities::new(
            &[Capability::Streaming],
            &[Capability::ToolCalls, Capability::AudioInput],
            requested_limits,
        )
        .expect("valid request");
        let negotiated = negotiate(&profile, requested).expect("compatible");
        assert!(negotiated.includes(Capability::Streaming));
        assert!(negotiated.includes(Capability::ToolCalls));
        assert!(!negotiated.includes(Capability::AudioInput));
        assert_eq!(negotiated.limits(), ModelLimits::new(500, 100, 2, 1, 1_024).unwrap());
    }
}
