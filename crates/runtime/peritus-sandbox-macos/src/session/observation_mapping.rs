//! Protected-handle validation and native control observations.

use peritus_process::NativeLaunchDescription;
use peritus_sandbox::CapabilityDomain;

use crate::{
    EXEC_STATUS_LABEL, EnforcementLevel, HelperManifest, MacosObservation, ObservationEvent,
    ObservationStatus,
};

pub(super) fn protected_handles_match(
    launch: &NativeLaunchDescription,
    manifest: &HelperManifest,
) -> bool {
    let handles = launch.protected_handles();
    let expected =
        1 + usize::from(manifest.proxy_descriptor().is_some()) + manifest.secrets().len();
    handles.len() == expected
        && handles.iter().any(|handle| {
            handle.label() == EXEC_STATUS_LABEL
                && handle.raw_handle() == u64::from(manifest.exec_status_descriptor())
                && handle.payload_len().is_none()
        })
        && manifest.proxy_descriptor().is_none_or(|proxy| {
            handles.iter().any(|handle| {
                handle.label() == proxy.label()
                    && handle.raw_handle() == u64::from(proxy.route().routing_handle())
                    && handle.payload_len()
                        == Some(usize::try_from(proxy.payload_len()).unwrap_or(usize::MAX))
            })
        })
        && manifest.secrets().iter().all(|secret| {
            handles.iter().any(|handle| {
                handle.label() == secret.label()
                    && handle.raw_handle() == u64::from(secret.descriptor())
                    && handle.payload_len()
                        == Some(usize::try_from(secret.payload_len()).unwrap_or(usize::MAX))
            })
        })
}

pub(super) fn push_native_mapping(
    observations: &mut Vec<MacosObservation>,
    manifest: &HelperManifest,
    event: ObservationEvent,
    domain: CapabilityDomain,
    resource: Option<peritus_sandbox::SandboxResourceKind>,
    enforcement: EnforcementLevel,
) {
    let sequence = u64::try_from(observations.len()).unwrap_or(u64::MAX).saturating_add(1);
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
}
