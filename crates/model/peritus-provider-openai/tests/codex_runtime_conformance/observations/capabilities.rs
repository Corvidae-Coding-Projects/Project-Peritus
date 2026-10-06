//! A capability exercise retains completed probes when any later step fails.

use peritus_conformance::{
    ProviderCapability, ProviderCapabilityObservation, ProviderConformanceFixture,
    ProviderConformanceObservation, ProviderScenario,
};
use peritus_model_protocol::{Capability, RequestedCapabilities, negotiate};

use super::super::diagnostics::ProbeError;
use super::super::support::{Probe, profile};

const SCOPES: [&str; 3] = ["capability.first", "capability.second", "capability.third"];

pub(in super::super) fn observe(
    fixture: &ProviderConformanceFixture,
) -> Result<ProviderConformanceObservation, ProbeError> {
    observe_with(fixture, Probe::run)
}

pub(in super::super) fn observe_with(
    fixture: &ProviderConformanceFixture,
    mut run: impl FnMut(&ProviderConformanceFixture) -> Result<Probe, ProbeError>,
) -> Result<ProviderConformanceObservation, ProbeError> {
    let mut probes = Vec::new();
    for scope in SCOPES {
        let probe = run(fixture)
            .map_err(|failure| failure.with_scope(scope).with_evidence(evidence(&probes)))?;
        probes.push((scope, probe));
    }
    let result = (|| {
        if probes.iter().any(|(_, probe)| {
            !probe.completed()
                || probe.auth_requests() != 1
                || probe.turn_requests() != 1
                || !probe.directory_removed
        }) {
            return Err(ProbeError::stage("verify.capability-effects"));
        }
        let profile = profile(ProviderScenario::CapabilityHonesty, 0xD5)
            .map_err(|_| ProbeError::stage("capability.profile"))?;
        let unsupported =
            RequestedCapabilities::new(&[Capability::AudioInput], &[], profile.limits())
                .map_err(|_| ProbeError::stage("capability.request"))?;
        if negotiate(&profile, unsupported).is_ok() {
            return Err(ProbeError::stage("verify.unsupported-capability"));
        }
        let advertised = vec![
            ProviderCapability::ToolCalls,
            ProviderCapability::ParallelToolCalls,
            ProviderCapability::UsageDetail,
        ];
        Ok(ProviderConformanceObservation::Capabilities(ProviderCapabilityObservation::new(
            advertised.clone(),
            advertised,
            vec![ProviderCapability::AudioInput],
            vec![ProviderCapability::ToolCalls, ProviderCapability::ParallelToolCalls],
            3,
        )))
    })();
    result.map_err(|failure| failure.with_evidence(evidence(&probes)))
}

fn evidence(probes: &[(&'static str, Probe)]) -> Vec<peritus_conformance::Observation> {
    probes.iter().flat_map(|(scope, probe)| probe.evidence(scope)).collect()
}
