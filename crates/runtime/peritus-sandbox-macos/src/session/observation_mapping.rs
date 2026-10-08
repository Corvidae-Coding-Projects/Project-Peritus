//! Protected-handle validation and native control observations.

use peritus_sandbox::{CapabilityDomain, ObservationTail};

use crate::{
    EnforcementLevel, HelperManifest, MacosObservation, ObservationEvent, ObservationStatus,
};

pub(super) fn push_native_mapping(
    observations: &mut ObservationTail<MacosObservation>,
    next_sequence: &mut u64,
    manifest: &HelperManifest,
    event: ObservationEvent,
    domain: CapabilityDomain,
    resource: Option<peritus_sandbox::SandboxResourceKind>,
    enforcement: EnforcementLevel,
) {
    let sequence = *next_sequence;
    let Some(next) = sequence.checked_add(1) else {
        return;
    };
    let status = match enforcement {
        EnforcementLevel::Hard => ObservationStatus::Enforced,
        EnforcementLevel::Supervisor => ObservationStatus::Supervised,
        EnforcementLevel::Unsupported | EnforcementLevel::Incomplete => {
            ObservationStatus::Incomplete
        }
    };
    observations.push(MacosObservation::new(
        sequence,
        manifest.plan_digest(),
        manifest.descriptor_digest(),
        manifest.preparation_digest(),
        manifest.profile_digest(),
        event,
        Some(domain),
        resource,
        Some(enforcement),
        status,
    ));
    *next_sequence = next;
}
