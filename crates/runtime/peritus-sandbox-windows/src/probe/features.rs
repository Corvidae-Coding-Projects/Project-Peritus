//! Independent native-feature projection and production resource enforcement levels.
use super::{MINIMUM_WINDOWS_BUILD, ProbeEvidence};
use crate::EnforcementLevel;
use peritus_sandbox::{FeatureSet, SandboxFeature};

/// Expected Windows split between Job Object and C2 supervisor enforcement.
#[must_use]
pub const fn production_resource_levels() -> [EnforcementLevel; 8] {
    [
        EnforcementLevel::Supervisor,
        EnforcementLevel::Hard,
        EnforcementLevel::Hard,
        EnforcementLevel::Supervisor,
        EnforcementLevel::Supervisor,
        EnforcementLevel::Supervisor,
        EnforcementLevel::Hard,
        EnforcementLevel::Supervisor,
    ]
}

pub(super) fn supported_features(evidence: &ProbeEvidence) -> FeatureSet {
    let mut features = FeatureSet::empty();
    let baseline = evidence.platform
        && evidence.os_build.is_some_and(|build| build >= MINIMUM_WINDOWS_BUILD)
        && evidence.architecture
        && evidence.helper
        && evidence.helper_digest.is_some()
        && evidence.restricted_token
        && evidence.low_integrity;
    if !baseline {
        return features;
    }
    if evidence.acl && evidence.reparse {
        for feature in [
            SandboxFeature::FilesystemDiscover,
            SandboxFeature::FilesystemMetadata,
            SandboxFeature::FilesystemRead,
            SandboxFeature::FilesystemExecute,
            SandboxFeature::FilesystemCreate,
            SandboxFeature::FilesystemWrite,
            SandboxFeature::FilesystemRemove,
        ] {
            features.insert(feature);
        }
    }
    if evidence.job_object && evidence.kill_on_close && evidence.inherited_handle_list {
        for feature in [
            SandboxFeature::ProcessRoot,
            SandboxFeature::ProcessDescendants,
            SandboxFeature::ProcessSignals,
            SandboxFeature::ProcessTree,
        ] {
            features.insert(feature);
        }
    }
    for feature in [SandboxFeature::EnvironmentClear, SandboxFeature::EnvironmentAllowList] {
        features.insert(feature);
    }
    if evidence.inherited_handle_list {
        features.insert(SandboxFeature::Pipes);
        features.insert(SandboxFeature::Stdin);
    }
    if evidence.app_container && evidence.app_container_sid_exact && evidence.deny_network {
        features.insert(SandboxFeature::NetworkDeny);
    }
    for (index, feature) in RESOURCE_FEATURES.into_iter().enumerate() {
        if matches!(
            evidence.resources[index],
            EnforcementLevel::Hard | EnforcementLevel::Supervisor
        ) {
            features.insert(feature);
        }
    }
    if evidence.conpty && evidence.inherited_handle_list {
        features.insert(SandboxFeature::Pty);
        features.insert(SandboxFeature::TerminalResize);
        features.insert(SandboxFeature::TerminalSignals);
    }
    if evidence.managed_network
        && evidence.app_container
        && evidence.app_container_sid_exact
        && evidence.deny_network
    {
        features.insert(SandboxFeature::NetworkEgress);
    }
    if evidence.credential_manager && evidence.inherited_handle_list {
        features.insert(SandboxFeature::SecretEnvironment);
        features.insert(SandboxFeature::SecretFile);
        features.insert(SandboxFeature::SecretHandle);
    }
    features
}

const RESOURCE_FEATURES: [SandboxFeature; 8] = [
    SandboxFeature::WallTime,
    SandboxFeature::CpuTime,
    SandboxFeature::Memory,
    SandboxFeature::Disk,
    SandboxFeature::Output,
    SandboxFeature::OpenHandles,
    SandboxFeature::ProcessCount,
    SandboxFeature::Concurrency,
];
