//! Canonical schema-v1 report encoding.

use crate::{AlternativeCauses, DiagnosticStatus, EvidenceCitation, ReportClaim};

use super::{claim::ClaimContent, validation::DebuggerReport};

pub(super) fn encode_report(report: &DebuggerReport) -> Vec<u8> {
    let mut bytes = b"peritus-e2-debugger-report-v1\0".to_vec();
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(report.manifest_id.as_bytes());
    bytes.extend_from_slice(report.manifest_digest.as_bytes());
    bytes.extend_from_slice(report.query_digest.as_bytes());
    encode_optional_id(&mut bytes, report.supersedes.map(|value| *value.as_bytes()));
    crate::query::encode_len(&mut bytes, report.timelines.len());
    for timeline in &report.timelines {
        encode_timeline(&mut bytes, timeline);
    }
    crate::query::encode_len(&mut bytes, report.causes.len());
    for cause in &report.causes {
        encode_cause(&mut bytes, cause);
    }
    crate::query::encode_len(&mut bytes, report.patterns.len());
    for pattern in &report.patterns {
        encode_pattern(&mut bytes, pattern);
    }
    crate::query::encode_len(&mut bytes, report.correlations.len());
    for correlation in &report.correlations {
        encode_correlation(&mut bytes, correlation);
    }
    encode_health(&mut bytes, &report.health);
    crate::query::encode_len(&mut bytes, report.claims.len());
    for claim in &report.claims {
        encode_claim(&mut bytes, claim);
    }
    bytes
}

pub(super) fn encode_claim_content(bytes: &mut Vec<u8>, content: &ClaimContent) {
    match content {
        ClaimContent::Observation { statement, support } => {
            bytes.push(1);
            encode_text(bytes, statement);
            encode_citations(bytes, support);
        }
        ClaimContent::Inference {
            statement,
            support,
            contrary,
            alternatives,
            confidence,
            category,
        } => {
            bytes.push(2);
            encode_text(bytes, statement);
            encode_citations(bytes, support);
            encode_citations(bytes, contrary);
            encode_alternatives(bytes, alternatives);
            bytes.extend_from_slice(&confidence.value().to_be_bytes());
            encode_confidence_basis(bytes, confidence.basis());
            bytes.extend_from_slice(&category.tag().to_be_bytes());
        }
        ClaimContent::Recommendation { statement, support, parent, affected_components } => {
            bytes.push(3);
            encode_text(bytes, statement);
            encode_citations(bytes, support);
            bytes.extend_from_slice(parent.as_bytes());
            crate::query::encode_len(bytes, affected_components.len());
            for component in affected_components {
                bytes.push(component.tag());
            }
        }
        ClaimContent::Unsupported(value) => {
            bytes.push(4);
            bytes.extend_from_slice(value.proposal_digest().as_bytes());
            bytes.push(value.reason() as u8);
        }
    }
}

fn encode_claim(bytes: &mut Vec<u8>, claim: &ReportClaim) {
    bytes.extend_from_slice(claim.id.as_bytes());
    encode_claim_content(bytes, &claim.content);
}

fn encode_timeline(bytes: &mut Vec<u8>, timeline: &crate::Timeline) {
    bytes.extend_from_slice(timeline.subject_id().as_bytes());
    crate::query::encode_len(bytes, timeline.entries().len());
    for entry in timeline.entries() {
        entry.citation().encode(bytes);
        bytes.extend_from_slice(entry.span_id().as_bytes());
        encode_boundary(bytes, entry.boundary());
        bytes.extend_from_slice(&entry.outcome().map_or(0, crate::OutcomeClass::tag).to_be_bytes());
        crate::query::encode_len(bytes, entry.resources().len());
        for resource in entry.resources() {
            bytes.extend_from_slice(&safe_key_tag(resource.key()).to_be_bytes());
            encode_safe_value(bytes, resource.value());
        }
        crate::query::encode_len(bytes, entry.predecessor_indices().len());
        for index in entry.predecessor_indices() {
            bytes.extend_from_slice(&index.to_be_bytes());
        }
        crate::query::encode_len(bytes, entry.missing_predecessors().len());
        for event in entry.missing_predecessors() {
            bytes.extend_from_slice(event.as_bytes());
        }
        bytes.extend_from_slice(&entry.monotonic_tick().to_be_bytes());
        bytes.extend_from_slice(&entry.unix_nanos().to_be_bytes());
    }
    crate::query::encode_len(bytes, timeline.clock_ambiguities().len());
    for ambiguity in timeline.clock_ambiguities() {
        ambiguity.earlier().encode(bytes);
        ambiguity.later().encode(bytes);
        bytes.extend_from_slice(&ambiguity.earlier_unix_nanos().to_be_bytes());
        bytes.extend_from_slice(&ambiguity.later_unix_nanos().to_be_bytes());
    }
}

fn encode_cause(bytes: &mut Vec<u8>, cause: &crate::RootCauseCandidate) {
    bytes.extend_from_slice(cause.id().as_bytes());
    bytes.extend_from_slice(&cause.category().tag().to_be_bytes());
    encode_text(bytes, cause.statement());
    encode_citations(bytes, cause.support());
    encode_citations(bytes, cause.contrary());
    encode_alternatives(bytes, cause.alternatives());
    bytes.extend_from_slice(&cause.confidence().value().to_be_bytes());
    encode_confidence_basis(bytes, cause.confidence().basis());
    crate::query::encode_len(bytes, cause.ambiguities().len());
    for value in cause.ambiguities() {
        bytes.push(ambiguity_tag(*value));
    }
    match cause.derivation() {
        crate::CauseDerivation::Deterministic => bytes.push(1),
        crate::CauseDerivation::ValidatedModel(id) => {
            bytes.push(2);
            bytes.extend_from_slice(id.as_bytes());
        }
    }
}

fn encode_pattern(bytes: &mut Vec<u8>, pattern: &crate::PatternCluster) {
    bytes.extend_from_slice(pattern.id().as_bytes());
    bytes.push(pattern.kind() as u8);
    bytes.extend_from_slice(pattern.fingerprint().digest().as_bytes());
    crate::query::encode_len(bytes, pattern.source_fingerprints().len());
    for source in pattern.source_fingerprints() {
        bytes.extend_from_slice(source.digest().as_bytes());
    }
    crate::query::encode_len(bytes, pattern.members().len());
    for member in pattern.members() {
        bytes.extend_from_slice(member.subject_id().as_bytes());
        bytes.extend_from_slice(&member.outcome().tag().to_be_bytes());
        bytes.extend_from_slice(
            &member.category().map_or(0, crate::FailureCategory::tag).to_be_bytes(),
        );
        bytes.push(member.analyzer() as u8);
        bytes.extend_from_slice(member.environment_id().as_bytes());
        bytes.extend_from_slice(member.harness_revision().digest().as_bytes());
        bytes.extend_from_slice(&member.workspace_revision().get().to_be_bytes());
        bytes.extend_from_slice(member.provider_profile_id().as_bytes());
        bytes.push(member.component_kind().map_or(0, peritus_harness::domain::ComponentKind::tag));
        encode_citations(bytes, member.citations());
        bytes.extend_from_slice(member.fingerprint().digest().as_bytes());
    }
}

fn encode_correlation(bytes: &mut Vec<u8>, value: &crate::ComponentCorrelation) {
    bytes.extend_from_slice(value.pattern_id().as_bytes());
    bytes.push(u8::from(value.component_id().is_some()));
    if let Some(id) = value.component_id() {
        crate::query::encode_blob(bytes, id.as_str().as_bytes());
    }
    bytes.push(value.component_kind().tag());
    bytes.push(u8::from(value.content_digest().is_some()));
    if let Some(digest) = value.content_digest() {
        bytes.extend_from_slice(digest.as_bytes());
    }
    bytes.push(value.protection_class().tag());
    bytes.push(value.basis() as u8);
    encode_subjects(bytes, value.supporting_subjects());
    encode_subjects(bytes, value.contrary_subjects());
    bytes.push(value.constraint() as u8);
    bytes.push(u8::from(value.class_only()));
}

fn encode_health(bytes: &mut Vec<u8>, value: &crate::HarnessHealthSummary) {
    bytes.push(match value.status() {
        DiagnosticStatus::DiagnosticOnly => 1,
    });
    crate::query::encode_len(bytes, value.revisions().len());
    for revision in value.revisions() {
        bytes.extend_from_slice(revision.harness_id().as_bytes());
        bytes.extend_from_slice(&revision.number().get().to_be_bytes());
        bytes.extend_from_slice(revision.digest().as_bytes());
    }
    for count in [
        value.subject_count(),
        value.successful_attempts(),
        value.failed_attempts(),
        value.indeterminate_attempts(),
        value.exact_component_correlations(),
        value.class_only_correlations(),
    ] {
        bytes.extend_from_slice(&count.to_be_bytes());
    }
    for ratio in [
        value.subject_coverage_millionths(),
        value.infrastructure_share_millionths(),
        value.repeated_pattern_share_millionths(),
        value.citation_coverage_millionths(),
        value.ambiguity_share_millionths(),
    ] {
        bytes.extend_from_slice(&ratio.to_be_bytes());
    }
    crate::query::encode_len(bytes, value.category_counts().len());
    for count in value.category_counts() {
        bytes.extend_from_slice(&count.category().tag().to_be_bytes());
        bytes.extend_from_slice(&count.count().to_be_bytes());
    }
}

fn encode_text(bytes: &mut Vec<u8>, text: &crate::DiagnosticText) {
    crate::query::encode_blob(bytes, text.as_str().as_bytes());
}
fn encode_citations(bytes: &mut Vec<u8>, values: &[EvidenceCitation]) {
    crate::query::encode_len(bytes, values.len());
    for value in values {
        value.encode(bytes);
    }
}
fn encode_subjects(bytes: &mut Vec<u8>, values: &[crate::SubjectId]) {
    crate::query::encode_len(bytes, values.len());
    for value in values {
        bytes.extend_from_slice(value.as_bytes());
    }
}
fn encode_alternatives(bytes: &mut Vec<u8>, value: &AlternativeCauses) {
    match value {
        AlternativeCauses::NoneKnown => bytes.push(0),
        AlternativeCauses::Categories(values) => {
            bytes.push(1);
            crate::query::encode_len(bytes, values.len());
            for value in values {
                bytes.extend_from_slice(&value.tag().to_be_bytes());
            }
        }
    }
}
fn encode_confidence_basis(bytes: &mut Vec<u8>, value: crate::ConfidenceBasis) {
    for count in [
        value.support_count(),
        value.contrary_count(),
        value.ambiguity_count(),
        value.recurrence_count(),
        value.maximum_causal_distance(),
    ] {
        bytes.extend_from_slice(&count.to_be_bytes());
    }
}
fn encode_optional_id(bytes: &mut Vec<u8>, value: Option<[u8; 16]>) {
    bytes.push(u8::from(value.is_some()));
    if let Some(value) = value {
        bytes.extend_from_slice(&value);
    }
}
fn encode_boundary(bytes: &mut Vec<u8>, value: crate::BoundaryKind) {
    match value {
        crate::BoundaryKind::Started(kind) => {
            bytes.push(1);
            bytes.push(crate::query::span_kind_tag(kind));
        }
        crate::BoundaryKind::Diagnostic(code) => {
            bytes.push(2);
            bytes.extend_from_slice(&crate::query::diagnostic_tag(code).to_be_bytes());
        }
        crate::BoundaryKind::Ended(outcome) => {
            bytes.push(3);
            bytes.push(crate::query::span_outcome_tag(outcome));
        }
    }
}
const fn safe_key_tag(key: peritus_trace::SafeAttributeKey) -> u16 {
    use peritus_trace::SafeAttributeKey as K;
    match key {
        K::ProviderRequest => 1,
        K::ToolInvocation => 2,
        K::GateEvaluation => 3,
        K::BudgetUnits => 4,
        K::RetryAttempt => 5,
        K::Cancellation => 6,
        K::Recovery => 7,
        K::CpuNanos => 8,
        K::MemoryBytes => 9,
        K::InputTokens => 10,
        K::OutputTokens => 11,
        K::CostMicrounits => 12,
        K::QueueDepth => 13,
        K::DroppedCount => 14,
        K::Status => 15,
        K::ArtifactEvidence => 16,
    }
}
fn encode_safe_value(bytes: &mut Vec<u8>, value: peritus_trace::SafeAttributeValue) {
    use peritus_trace::SafeAttributeValue as V;
    match value {
        V::Count(v) => {
            bytes.push(1);
            bytes.extend_from_slice(&v.to_be_bytes());
        }
        V::DurationNanos(v) => {
            bytes.push(2);
            bytes.extend_from_slice(&v.to_be_bytes());
        }
        V::Identifier(v) => {
            bytes.push(3);
            bytes.extend_from_slice(&v);
        }
        V::Digest(v) => {
            bytes.push(4);
            bytes.extend_from_slice(v.as_bytes());
        }
        V::Status(v) => {
            bytes.push(5);
            bytes.push(status_tag(v));
        }
        V::Vault(v) => {
            bytes.push(6);
            bytes.extend_from_slice(v.digest().as_bytes());
            bytes.extend_from_slice(&v.size().to_be_bytes());
            bytes.extend_from_slice(v.creating_event().as_bytes());
            bytes.extend_from_slice(v.key_reference().as_bytes());
            bytes.extend_from_slice(v.parameters_digest().as_bytes());
        }
    }
}
const fn status_tag(value: peritus_trace::StatusCode) -> u8 {
    use peritus_trace::StatusCode as S;
    match value {
        S::Pending => 1,
        S::Success => 2,
        S::Failure => 3,
        S::InfrastructureFailure => 4,
        S::Cancelled => 5,
        S::TimedOut => 6,
        S::Indeterminate => 7,
    }
}
const fn ambiguity_tag(value: crate::AmbiguityFlag) -> u8 {
    match value {
        crate::AmbiguityFlag::MissingCausalPredecessor => 1,
        crate::AmbiguityFlag::ClockDisagreement => 2,
        crate::AmbiguityFlag::MultiplePlausibleCauses => 3,
        crate::AmbiguityFlag::IncompleteSpan => 4,
        crate::AmbiguityFlag::CrossRevisionAttribution => 5,
    }
}
