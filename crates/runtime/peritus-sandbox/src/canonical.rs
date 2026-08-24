//! Versioned canonical bytes for sandbox and backend identities.

use crate::{
    BackendDescriptor, DescendantPolicy, EnvironmentContract, EnvironmentMode, FeatureSet,
    HostMatcher, IsolationRequirement, NetworkHost, NetworkRule, NetworkTarget, PathSemantics,
    ProcessContract, ResourceFidelity, ResourceLimits, SandboxBinding, SandboxContract,
    SandboxOperationClass, SandboxRequirements, SandboxResourceKind, SecretDelivery, SecretGrant,
    TerminalContract, TerminalLimits, TerminalRequirements,
};
use peritus_types::{RevisionTuple, Sha256Digest};

const PLAN_DOMAIN: &[u8] = b"PERITUS-SANDBOX-PLAN-V1\0";
const DESCRIPTOR_DOMAIN: &[u8] = b"PERITUS-SANDBOX-BACKEND-V1\0";
const PREPARATION_DOMAIN: &[u8] = b"PERITUS-SANDBOX-PREPARATION-V1\0";

/// Encodes one complete canonical checked plan representation.
pub fn plan_bytes(
    binding: SandboxBinding,
    isolation: IsolationRequirement,
    operation_class: SandboxOperationClass,
    contract: &SandboxContract,
    requirements: &SandboxRequirements,
    required_features: FeatureSet,
) -> Vec<u8> {
    let mut bytes = Vec::from(PLAN_DOMAIN);
    binding_bytes(&mut bytes, binding);
    byte(&mut bytes, isolation.ordinal());
    byte(&mut bytes, operation_class.ordinal());
    contract_bytes(&mut bytes, contract);
    requirements_bytes(&mut bytes, requirements);
    u64_value(&mut bytes, required_features.bits());
    bytes
}

/// Encodes one complete canonical backend descriptor.
pub fn descriptor_bytes(descriptor: &BackendDescriptor) -> Vec<u8> {
    let mut bytes = Vec::from(DESCRIPTOR_DOMAIN);
    text(&mut bytes, descriptor.name().as_str());
    text(&mut bytes, descriptor.version().as_str());
    byte(&mut bytes, descriptor.kind().ordinal());
    byte(&mut bytes, descriptor.path_semantics().ordinal());
    byte(&mut bytes, descriptor.resource_fidelity().ordinal());
    u64_value(&mut bytes, descriptor.supported_features().bits());
    bytes
}

/// Digests the enforcement-relevant portion of a backend descriptor.
pub fn support_digest(
    features: FeatureSet,
    path_semantics: PathSemantics,
    resource_fidelity: ResourceFidelity,
) -> Sha256Digest {
    let mut bytes = Vec::from(b"PERITUS-SANDBOX-SUPPORT-V1\0".as_slice());
    u64_value(&mut bytes, features.bits());
    byte(&mut bytes, path_semantics.ordinal());
    byte(&mut bytes, resource_fidelity.ordinal());
    peritus_codec::sha256(&bytes)
}

/// Binds plan, descriptor, and support identities into one preparation identity.
pub fn preparation_digest(
    plan: Sha256Digest,
    descriptor: Sha256Digest,
    support: Sha256Digest,
) -> Sha256Digest {
    let mut bytes = Vec::from(PREPARATION_DOMAIN);
    bytes.extend_from_slice(plan.as_bytes());
    bytes.extend_from_slice(descriptor.as_bytes());
    bytes.extend_from_slice(support.as_bytes());
    peritus_codec::sha256(&bytes)
}

fn binding_bytes(bytes: &mut Vec<u8>, binding: SandboxBinding) {
    bytes.extend_from_slice(binding.process_id().as_bytes());
    bytes.extend_from_slice(binding.resource_id().as_bytes());
    bytes.extend_from_slice(binding.environment_id().as_bytes());
    revision_bytes(bytes, binding.revision());
}

fn revision_bytes(bytes: &mut Vec<u8>, revision: RevisionTuple) {
    bytes.extend_from_slice(revision.acceptance_spec_id().as_bytes());
    bytes.extend_from_slice(revision.harness_id().as_bytes());
    bytes.extend_from_slice(revision.workspace_id().as_bytes());
    u64_value(bytes, revision.workspace_generation().get());
    u64_value(bytes, revision.workspace_revision().get());
    bytes.extend_from_slice(revision.policy_id().as_bytes());
    bytes.extend_from_slice(revision.provider_profile_id().as_bytes());
}

fn contract_bytes(bytes: &mut Vec<u8>, contract: &SandboxContract) {
    sequence(bytes, contract.filesystem().rules(), |bytes, rule| {
        byte(bytes, rule.effect().ordinal());
        text(bytes, rule.path().as_str());
        byte(bytes, rule.scope().ordinal());
        byte(bytes, rule.operations().bits());
    });
    process_contract_bytes(bytes, contract.process());
    environment_contract_bytes(bytes, contract.environment());
    sequence(bytes, contract.network().rules(), network_rule_bytes);
    sequence(bytes, contract.secrets().grants(), secret_grant_bytes);
    resource_limits_bytes(bytes, contract.resources());
    terminal_contract_bytes(bytes, contract.terminal());
}

fn process_contract_bytes(bytes: &mut Vec<u8>, contract: &ProcessContract) {
    sequence(bytes, contract.root_programs(), |bytes, path| text(bytes, path.as_str()));
    match contract.descendants() {
        DescendantPolicy::Denied => byte(bytes, 0),
        DescendantPolicy::Bounded(limit) => {
            byte(bytes, 1);
            u32_value(bytes, limit);
        }
    }
    byte(bytes, contract.signals().ordinal());
    byte(bytes, contract.containment().ordinal());
    u32_value(bytes, contract.maximum_processes());
}

fn environment_contract_bytes(bytes: &mut Vec<u8>, contract: &EnvironmentContract) {
    match contract.mode() {
        EnvironmentMode::Cleared => byte(bytes, 0),
        EnvironmentMode::AllowListed(names) => {
            byte(bytes, 1);
            sequence(bytes, names, |bytes, name| text(bytes, name.as_str()));
        }
    }
    sequence(bytes, contract.literal_names(), |bytes, name| text(bytes, name.as_str()));
}

fn network_rule_bytes(bytes: &mut Vec<u8>, rule: &NetworkRule) {
    byte(bytes, rule.effect().ordinal());
    host_matcher_bytes(bytes, rule.host());
    byte(bytes, rule.transport().ordinal());
    u16_value(bytes, rule.ports().start());
    u16_value(bytes, rule.ports().end());
}

fn host_matcher_bytes(bytes: &mut Vec<u8>, matcher: &HostMatcher) {
    match matcher {
        HostMatcher::DnsExact(name) => {
            byte(bytes, 0);
            text(bytes, name.as_str());
        }
        HostMatcher::DnsSuffix(name) => {
            byte(bytes, 1);
            text(bytes, name.as_str());
        }
        HostMatcher::IpPrefix { address, prefix_length } => {
            byte(bytes, 2);
            text(bytes, &address.to_string());
            byte(bytes, *prefix_length);
        }
    }
}

fn secret_grant_bytes(bytes: &mut Vec<u8>, grant: &SecretGrant) {
    bytes.extend_from_slice(grant.reference().resource_id().as_bytes());
    bytes.extend_from_slice(grant.reference().version().as_bytes());
    match grant.delivery() {
        SecretDelivery::Environment(name) => {
            byte(bytes, 0);
            text(bytes, name.as_str());
        }
        SecretDelivery::File(path) => {
            byte(bytes, 1);
            text(bytes, path.as_str());
        }
        SecretDelivery::BrokeredHandle(label) => {
            byte(bytes, 2);
            text(bytes, label.as_str());
        }
    }
}

fn resource_limits_bytes(bytes: &mut Vec<u8>, limits: &ResourceLimits) {
    for kind in SandboxResourceKind::ALL {
        u64_value(bytes, limits.limit(kind).get());
    }
}

fn terminal_contract_bytes(bytes: &mut Vec<u8>, contract: TerminalContract) {
    byte(bytes, contract.modes().bits());
    byte(bytes, contract.input().ordinal());
    byte(bytes, contract.resize().ordinal());
    byte(bytes, contract.signals().ordinal());
    terminal_limits_bytes(bytes, contract.limits());
}

fn terminal_limits_bytes(bytes: &mut Vec<u8>, limits: TerminalLimits) {
    match limits.maximum_initial_size() {
        Some(size) => {
            byte(bytes, 1);
            u16_value(bytes, size.columns());
            u16_value(bytes, size.rows());
        }
        None => byte(bytes, 0),
    }
    u64_value(bytes, limits.event_count().get());
    u64_value(bytes, limits.output_bytes().get());
}

fn requirements_bytes(bytes: &mut Vec<u8>, requirements: &SandboxRequirements) {
    sequence(bytes, requirements.files(), |bytes, requirement| {
        text(bytes, requirement.path().as_str());
        byte(bytes, requirement.operation().ordinal());
    });
    text(bytes, requirements.process().program().as_str());
    u32_value(bytes, requirements.process().descendant_count());
    byte(bytes, u8::from(requirements.process().requires_forced_termination()));
    sequence(bytes, requirements.environment().inherited_names(), |bytes, name| {
        text(bytes, name.as_str());
    });
    sequence(bytes, requirements.environment().literal_names(), |bytes, name| {
        text(bytes, name.as_str());
    });
    sequence(bytes, requirements.network(), network_target_bytes);
    sequence(bytes, requirements.secrets(), secret_grant_bytes);
    resource_limits_bytes(bytes, requirements.resources());
    terminal_requirements_bytes(bytes, requirements.terminal());
}

fn network_target_bytes(bytes: &mut Vec<u8>, target: &NetworkTarget) {
    match target.host() {
        NetworkHost::Dns(name) => {
            byte(bytes, 0);
            text(bytes, name.as_str());
        }
        NetworkHost::Ip(address) => {
            byte(bytes, 1);
            text(bytes, &address.to_string());
        }
    }
    byte(bytes, target.transport().ordinal());
    u16_value(bytes, target.port());
}

fn terminal_requirements_bytes(bytes: &mut Vec<u8>, requirements: TerminalRequirements) {
    byte(bytes, requirements.mode().ordinal());
    byte(bytes, requirements.input().ordinal());
    byte(bytes, requirements.resize().ordinal());
    byte(bytes, requirements.signals().ordinal());
    match requirements.initial_size() {
        Some(size) => {
            byte(bytes, 1);
            u16_value(bytes, size.columns());
            u16_value(bytes, size.rows());
        }
        None => byte(bytes, 0),
    }
    u64_value(bytes, requirements.event_count().get());
    u64_value(bytes, requirements.output_bytes().get());
}

fn sequence<T>(bytes: &mut Vec<u8>, values: &[T], mut encode: impl FnMut(&mut Vec<u8>, &T)) {
    u32_value(bytes, u32::try_from(values.len()).expect("domain collection bounds fit u32"));
    for value in values {
        encode(bytes, value);
    }
}

fn text(bytes: &mut Vec<u8>, value: &str) {
    u32_value(bytes, u32::try_from(value.len()).expect("validated text bounds fit u32"));
    bytes.extend_from_slice(value.as_bytes());
}

fn byte(bytes: &mut Vec<u8>, value: u8) {
    bytes.push(value);
}
fn u16_value(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}
fn u32_value(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}
fn u64_value(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}
