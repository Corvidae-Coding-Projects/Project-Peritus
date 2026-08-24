//! Named Linux portions of the C3 formal obligations and their executable projection.

use vstd::prelude::*;

use crate::verified::{ActivationFacts, NativeBindingFacts, TeardownFacts};

verus! {

/// `OBL-0130-LINUX`: admitted native preparation covers and binds every exact input identity.
pub proof fn admitted_linux_backend_covers_and_binds(facts: NativeBindingFacts)
    requires crate::verified::native_binding_complete_spec(facts),
    ensures
        facts.features_covered,
        facts.plan_exact,
        facts.descriptor_exact,
        facts.probe_exact,
        facts.preparation_exact,
{
}

/// `OBL-0133-LINUX`: complete teardown leaves no backend, proxy, or secret resource.
pub proof fn complete_linux_teardown_releases_every_resource(facts: TeardownFacts)
    requires crate::verified::teardown_complete_spec(facts),
    ensures
        facts.backend_resources_empty,
        facts.proxy_resources_empty,
        facts.secret_resources_empty,
{
}

/// `OBL-0134-LINUX`: unsupported or mismatched preparation has no activation effect.
pub proof fn unsupported_or_mismatched_linux_preparation_has_no_effect(facts: ActivationFacts)
    requires
        !facts.supported || !facts.binding_exact,
        crate::verified::unsupported_or_mismatched_no_effect_spec(facts),
    ensures
        !facts.process_activated,
        !facts.network_activated,
        !facts.secrets_activated,
{
}

} // verus!

use peritus_types::Sha256Digest;

/// Bounded facts for the OBL-0130/0133/0134 executable refinement checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "formal refinement inputs preserve independent activation and cleanup facts"
)]
pub struct RefinementFacts {
    /// Required feature bits.
    pub required_features: u64,
    /// Probed supported feature bits.
    pub supported_features: u64,
    /// Checked plan identity.
    pub plan_digest: Sha256Digest,
    /// Manifest plan identity.
    pub manifest_plan_digest: Sha256Digest,
    /// Admitted descriptor identity.
    pub descriptor_digest: Sha256Digest,
    /// Manifest descriptor identity.
    pub manifest_descriptor_digest: Sha256Digest,
    /// Live probe identity.
    pub probe_digest: Sha256Digest,
    /// Manifest probe identity.
    pub manifest_probe_digest: Sha256Digest,
    /// Authorized preparation identity.
    pub preparation_digest: Sha256Digest,
    /// Manifest preparation identity.
    pub manifest_preparation_digest: Sha256Digest,
    /// Whether a process activation effect occurred.
    pub process_activated: bool,
    /// Whether a network activation effect occurred.
    pub network_activated: bool,
    /// Whether a secret activation effect occurred.
    pub secrets_activated: bool,
    /// Whether exact complete cleanup was observed.
    pub cleanup_complete: bool,
    /// Number of remaining backend-owned native resources.
    pub owned_backend_resources: usize,
    /// Number of remaining managed-proxy resources.
    pub owned_proxy_resources: usize,
    /// Number of remaining secret-delivery resources.
    pub owned_secret_resources: usize,
}

impl RefinementFacts {
    fn binding_facts(self) -> NativeBindingFacts {
        NativeBindingFacts {
            features_covered: crate::verified::support_covers(
                self.required_features,
                self.supported_features,
            ),
            plan_exact: self.plan_digest == self.manifest_plan_digest,
            descriptor_exact: self.descriptor_digest == self.manifest_descriptor_digest,
            probe_exact: self.probe_digest == self.manifest_probe_digest,
            preparation_exact: self.preparation_digest == self.manifest_preparation_digest,
        }
    }

    /// OBL-0130: admitted support and every session binding are exact.
    #[must_use]
    pub fn admission_is_exact(self) -> bool {
        crate::verified::native_binding_complete(self.binding_facts())
    }

    /// OBL-0133: complete teardown leaves no backend, proxy, or secret resource.
    #[must_use]
    pub const fn complete_teardown_is_empty(self) -> bool {
        !self.cleanup_complete
            || crate::verified::teardown_complete(TeardownFacts {
                backend_resources_empty: self.owned_backend_resources == 0,
                proxy_resources_empty: self.owned_proxy_resources == 0,
                secret_resources_empty: self.owned_secret_resources == 0,
            })
    }

    /// OBL-0134: unsupported or mismatched preparation has no activation effect.
    #[must_use]
    pub fn mismatch_has_no_activation(self) -> bool {
        crate::verified::unsupported_or_mismatched_has_no_effect(ActivationFacts {
            supported: crate::verified::support_covers(
                self.required_features,
                self.supported_features,
            ),
            binding_exact: crate::verified::native_binding_complete(self.binding_facts()),
            process_activated: self.process_activated,
            network_activated: self.network_activated,
            secrets_activated: self.secrets_activated,
        })
    }
}
