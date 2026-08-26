//! Canonical schema-v1 selection-manifest encoding.

use peritus_trace::{
    CausalBinding, RedactedValue, SafeAttribute, SafeAttributeKey, SafeAttributeValue,
};
use peritus_types::Sha256Digest;

use super::{SelectedArtifact, SelectedEvidence, TraceSelectionManifest};

const MANIFEST_CANONICAL_DOMAIN: &[u8] = b"peritus-e2-selection-manifest-v1\0";

pub(super) fn encode_manifest(manifest: &TraceSelectionManifest) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MANIFEST_CANONICAL_DOMAIN);
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(manifest.query_digest().as_bytes());
    crate::query::encode_len(&mut bytes, manifest.subjects().len());
    for subject in manifest.subjects() {
        crate::query::encode_blob(&mut bytes, &subject.canonical_bytes());
    }
    crate::query::encode_len(&mut bytes, manifest.entries().len());
    for entry in manifest.entries() {
        encode_entry(&mut bytes, entry);
    }
    crate::query::encode_len(&mut bytes, manifest.artifacts().len());
    for artifact in manifest.artifacts() {
        encode_artifact(&mut bytes, artifact);
    }
    let counts = manifest.counts();
    for count in [
        counts.subjects(),
        counts.traces(),
        counts.events(),
        counts.causal_edges(),
        counts.artifacts(),
        counts.artifact_bytes(),
    ] {
        bytes.extend_from_slice(&count.to_be_bytes());
    }
    bytes
}

fn encode_artifact(bytes: &mut Vec<u8>, artifact: &SelectedArtifact) {
    bytes.extend_from_slice(artifact.digest().as_bytes());
    bytes.extend_from_slice(&artifact.size().to_be_bytes());
    bytes.extend_from_slice(artifact.creating_event().as_bytes());
    bytes.push(u8::from(artifact.source_event().is_some()));
    if let Some(event) = artifact.source_event() {
        bytes.extend_from_slice(event.as_bytes());
    }
}

fn encode_entry(bytes: &mut Vec<u8>, entry: &SelectedEvidence) {
    bytes.extend_from_slice(entry.subject().id().as_bytes());
    bytes.extend_from_slice(&entry.journal_position().to_be_bytes());
    bytes.extend_from_slice(entry.event_id().as_bytes());
    bytes.extend_from_slice(entry.trace_id().as_bytes());
    bytes.extend_from_slice(entry.span_id().as_bytes());
    bytes.extend_from_slice(&entry.span_sequence().to_be_bytes());
    bytes.push(u8::from(entry.parent_span_id().is_some()));
    if let Some(parent) = entry.parent_span_id() {
        bytes.extend_from_slice(parent.as_bytes());
    }
    crate::query::encode_len(bytes, entry.causal_events().len());
    for cause in entry.causal_events() {
        bytes.extend_from_slice(cause.as_bytes());
    }
    encode_binding(bytes, entry.binding());
    bytes.extend_from_slice(&entry.time().unix_nanos().to_be_bytes());
    bytes.extend_from_slice(&entry.time().monotonic_tick().to_be_bytes());
    crate::query::encode_observation_kind(bytes, entry.kind());
    crate::query::encode_len(bytes, entry.attributes().len());
    for attribute in entry.attributes() {
        encode_attribute(bytes, *attribute);
    }
    crate::query::encode_len(bytes, entry.redactions().len());
    for redaction in entry.redactions() {
        encode_redaction(bytes, *redaction);
    }
    bytes.extend_from_slice(entry.frame_digest().as_bytes());
    bytes.extend_from_slice(&entry.frame_length().to_be_bytes());
}

fn encode_binding(bytes: &mut Vec<u8>, binding: CausalBinding) {
    bytes.extend_from_slice(binding.session_id().as_bytes());
    encode_optional_id(bytes, binding.run_id().map(|id| *id.as_bytes()));
    encode_optional_id(bytes, binding.attempt_id().map(|id| *id.as_bytes()));
    encode_optional_id(bytes, binding.turn_id().map(|id| *id.as_bytes()));
    encode_optional_id(bytes, binding.action_id().map(|id| *id.as_bytes()));
    encode_optional_id(bytes, binding.provider_profile_id().map(|id| *id.as_bytes()));
    encode_optional_digest(bytes, binding.tool_descriptor_digest());
    encode_optional_id(bytes, binding.gate_id().map(|id| *id.as_bytes()));
    encode_optional_id(bytes, binding.gate_execution_id().map(|id| *id.as_bytes()));
}

fn encode_attribute(bytes: &mut Vec<u8>, attribute: SafeAttribute) {
    bytes.extend_from_slice(&attribute_key_tag(attribute.key()).to_be_bytes());
    match attribute.value() {
        SafeAttributeValue::Count(value) => {
            bytes.push(1);
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        SafeAttributeValue::DurationNanos(value) => {
            bytes.push(2);
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        SafeAttributeValue::Identifier(value) => {
            bytes.push(3);
            bytes.extend_from_slice(&value);
        }
        SafeAttributeValue::Digest(value) => {
            bytes.push(4);
            bytes.extend_from_slice(value.as_bytes());
        }
        SafeAttributeValue::Status(value) => {
            bytes.push(5);
            bytes.push(status_tag(value));
        }
        SafeAttributeValue::Vault(value) => {
            bytes.push(6);
            encode_vault(bytes, value);
        }
    }
}

fn encode_redaction(bytes: &mut Vec<u8>, redaction: RedactedValue) {
    match redaction {
        RedactedValue::Omitted { class, observed_bytes } => {
            bytes.push(1);
            bytes.push(sensitivity_tag(class));
            bytes.extend_from_slice(&observed_bytes.to_be_bytes());
        }
        RedactedValue::Vault { class, reference } => {
            bytes.push(2);
            bytes.push(sensitivity_tag(class));
            encode_vault(bytes, reference);
        }
    }
}

fn encode_vault(bytes: &mut Vec<u8>, value: peritus_trace::ArtifactVaultReference) {
    bytes.extend_from_slice(value.digest().as_bytes());
    bytes.extend_from_slice(&value.size().to_be_bytes());
    bytes.extend_from_slice(value.creating_event().as_bytes());
    bytes.extend_from_slice(value.key_reference().as_bytes());
    bytes.extend_from_slice(value.parameters_digest().as_bytes());
}

fn encode_optional_id(bytes: &mut Vec<u8>, value: Option<[u8; 16]>) {
    bytes.push(u8::from(value.is_some()));
    if let Some(value) = value {
        bytes.extend_from_slice(&value);
    }
}
fn encode_optional_digest(bytes: &mut Vec<u8>, value: Option<Sha256Digest>) {
    bytes.push(u8::from(value.is_some()));
    if let Some(value) = value {
        bytes.extend_from_slice(value.as_bytes());
    }
}

const fn attribute_key_tag(key: SafeAttributeKey) -> u16 {
    match key {
        SafeAttributeKey::ProviderRequest => 1,
        SafeAttributeKey::ToolInvocation => 2,
        SafeAttributeKey::GateEvaluation => 3,
        SafeAttributeKey::BudgetUnits => 4,
        SafeAttributeKey::RetryAttempt => 5,
        SafeAttributeKey::Cancellation => 6,
        SafeAttributeKey::Recovery => 7,
        SafeAttributeKey::CpuNanos => 8,
        SafeAttributeKey::MemoryBytes => 9,
        SafeAttributeKey::InputTokens => 10,
        SafeAttributeKey::OutputTokens => 11,
        SafeAttributeKey::CostMicrounits => 12,
        SafeAttributeKey::QueueDepth => 13,
        SafeAttributeKey::DroppedCount => 14,
        SafeAttributeKey::Status => 15,
        SafeAttributeKey::ArtifactEvidence => 16,
    }
}
const fn status_tag(status: peritus_trace::StatusCode) -> u8 {
    use peritus_trace::StatusCode as S;
    match status {
        S::Pending => 1,
        S::Success => 2,
        S::Failure => 3,
        S::InfrastructureFailure => 4,
        S::Cancelled => 5,
        S::TimedOut => 6,
        S::Indeterminate => 7,
    }
}
const fn sensitivity_tag(class: peritus_trace::SensitivityClass) -> u8 {
    use peritus_trace::SensitivityClass as S;
    match class {
        S::Prompt => 1,
        S::ModelOutput => 2,
        S::ToolArguments => 3,
        S::Secret => 4,
        S::Credential => 5,
        S::Environment => 6,
        S::WorkspaceContent => 7,
    }
}
