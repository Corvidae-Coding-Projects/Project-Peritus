//! Protected-handle validation and complete native control observations.

use peritus_sandbox::CapabilityDomain;

use crate::{
    EnforcementLevel, HelperManifest, MacosError, MacosErrorKind, MacosObservation,
    MacosOperation, ObservationEvent, ObservationStatus, RecoveryAction,
};

const LIFECYCLE_OBSERVATIONS: usize = 5;
const BASE_CONTROL_MAPPINGS: usize = 5;

pub(super) fn initial_native_observations(
    manifest: &HelperManifest,
) -> Result<(Vec<MacosObservation>, u64, usize), MacosError> {
    let mapping_count = BASE_CONTROL_MAPPINGS
        .checked_add(manifest.resources().controls().len())
        .and_then(|count| count.checked_add(if manifest.proxy().is_some() { 1 } else { 0 }))
        .and_then(|count| {
            count.checked_add(if manifest.secrets().is_empty() { 0 } else { 1 })
        })
        .ok_or_else(observation_capacity_error)?;
    let capacity = LIFECYCLE_OBSERVATIONS
        .checked_add(mapping_count)
        .ok_or_else(observation_capacity_error)?;
    let mut observations = Vec::new();
    observations
        .try_reserve_exact(capacity)
        .map_err(|_| observation_capacity_error())?;
    observations.push(MacosObservation::new(
        1,
        manifest.plan_digest(),
        manifest.descriptor_digest(),
        manifest.preparation_digest(),
        manifest.profile_digest(),
        ObservationEvent::Prepared,
        None,
        None,
        None,
        ObservationStatus::Completed,
    ));
    let mut next_sequence = 2_u64;
    for (domain, enforcement) in [
        (CapabilityDomain::Filesystem, EnforcementLevel::Hard),
        (CapabilityDomain::Process, EnforcementLevel::Supervisor),
        (CapabilityDomain::Environment, EnforcementLevel::Supervisor),
        (CapabilityDomain::Network, EnforcementLevel::Hard),
        (CapabilityDomain::Terminal, EnforcementLevel::Supervisor),
    ] {
        push_native_mapping(
            &mut observations,
            &mut next_sequence,
            manifest,
            ObservationEvent::ControlMapped,
            domain,
            None,
            enforcement,
        )?;
    }
    for control in manifest.resources().controls() {
        push_native_mapping(
            &mut observations,
            &mut next_sequence,
            manifest,
            ObservationEvent::ResourceMapped,
            CapabilityDomain::Resource,
            Some(control.kind()),
            control.level(),
        )?;
    }
    if manifest.proxy().is_some() {
        push_native_mapping(
            &mut observations,
            &mut next_sequence,
            manifest,
            ObservationEvent::ProxyMapped,
            CapabilityDomain::Network,
            None,
            EnforcementLevel::Supervisor,
        )?;
    }
    if !manifest.secrets().is_empty() {
        push_native_mapping(
            &mut observations,
            &mut next_sequence,
            manifest,
            ObservationEvent::ControlMapped,
            CapabilityDomain::Secret,
            None,
            EnforcementLevel::Supervisor,
        )?;
    }
    if observations
        .len()
        .checked_add(LIFECYCLE_OBSERVATIONS - 1)
        != Some(capacity)
    {
        return Err(observation_capacity_error());
    }
    Ok((observations, next_sequence, capacity))
}

fn push_native_mapping(
    observations: &mut Vec<MacosObservation>,
    next_sequence: &mut u64,
    manifest: &HelperManifest,
    event: ObservationEvent,
    domain: CapabilityDomain,
    resource: Option<peritus_sandbox::SandboxResourceKind>,
    enforcement: EnforcementLevel,
) -> Result<(), MacosError> {
    let sequence = *next_sequence;
    let next = sequence.checked_add(1).ok_or_else(observation_capacity_error)?;
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
    Ok(())
}

fn observation_capacity_error() -> MacosError {
    MacosError::new(
        MacosErrorKind::LimitExceeded,
        MacosOperation::Prepare,
        RecoveryAction::CorrectRequest,
        "complete macOS observation evidence cannot be represented",
    )
}
