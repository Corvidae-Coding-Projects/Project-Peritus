//! Incremental schema-v1 report encoding with bounded physical payload pages.

use crate::{
    AlternativeCauses, AnalysisContext, AnalysisControl, AnalysisStage, DebuggerError,
    DebuggerOperation, DiagnosticStatus, EvidenceCitation, ReportClaim,
};
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};

use super::{claim::ClaimContent, validation::DebuggerReport};

const REPORT_DIGEST_DOMAIN: &[u8] = b"peritus-e2-debugger-report-digest-v1\0";

pub(super) struct StreamedReport {
    pub(super) digest: Sha256Digest,
    pub(super) payload_size: u64,
    pub(super) chunks: Vec<Vec<u8>>,
}

struct PageSink {
    hash: Sha256,
    chunks: Vec<Vec<u8>>,
    current: Vec<u8>,
    bound: usize,
    size: u64,
}

impl PageSink {
    fn new(bound: usize) -> Self {
        let mut hash = Sha256::new();
        hash.update(REPORT_DIGEST_DOMAIN);
        Self { hash, chunks: Vec::new(), current: Vec::new(), bound: bound.max(1), size: 0 }
    }

    fn write(&mut self, mut value: &[u8]) {
        self.hash.update(value);
        self.size = self
            .size
            .saturating_add(u64::try_from(value.len()).unwrap_or(u64::MAX));
        while !value.is_empty() {
            let available = self.bound.saturating_sub(self.current.len());
            if available == 0 {
                self.chunks.push(core::mem::replace(
                    &mut self.current,
                    Vec::with_capacity(self.bound),
                ));
                continue;
            }
            let take = available.min(value.len());
            self.current.extend_from_slice(&value[..take]);
            value = &value[take..];
        }
    }

    fn byte(&mut self, value: u8) {
        self.write(&[value]);
    }

    fn finish(mut self) -> StreamedReport {
        if !self.current.is_empty() {
            self.chunks.push(self.current);
        }
        StreamedReport {
            digest: Sha256Digest::new(self.hash.finalize().into()),
            payload_size: self.size,
            chunks: self.chunks,
        }
    }
}

pub(super) fn encode_report(
    report: &DebuggerReport,
    page_bound: usize,
    context: AnalysisContext,
    control: &mut impl AnalysisControl,
) -> Result<StreamedReport, DebuggerError> {
    let total = report
        .timelines
        .len()
        .saturating_add(report.causes.len())
        .saturating_add(report.patterns.len())
        .saturating_add(report.correlations.len())
        .saturating_add(report.claims.len())
        .saturating_add(1);
    let mut completed = 0_usize;
    let mut output = PageSink::new(page_bound);
    output.write(b"peritus-e2-debugger-report-v1\0");
    output.write(&1_u16.to_be_bytes());
    output.write(report.manifest_id.as_bytes());
    output.write(report.manifest_digest.as_bytes());
    output.write(report.query_digest.as_bytes());
    encode_optional_id(&mut output, report.supersedes.map(|value| *value.as_bytes()));
    encode_len(&mut output, report.timelines.len());
    for timeline in &report.timelines {
        encode_timeline(&mut output, timeline);
        completed = completed.saturating_add(1);
        crate::work::checkpoint(
            control,
            context,
            AnalysisStage::EncodeReport,
            completed,
            total,
            DebuggerOperation::ValidateReport,
        )?;
    }
    encode_len(&mut output, report.causes.len());
    for cause in &report.causes {
        encode_cause(&mut output, cause);
        completed = completed.saturating_add(1);
        crate::work::checkpoint(
            control,
            context,
            AnalysisStage::EncodeReport,
            completed,
            total,
            DebuggerOperation::ValidateReport,
        )?;
    }
    encode_len(&mut output, report.patterns.len());
    for pattern in &report.patterns {
        encode_pattern(&mut output, pattern);
        completed = completed.saturating_add(1);
        crate::work::checkpoint(
            control,
            context,
            AnalysisStage::EncodeReport,
            completed,
            total,
            DebuggerOperation::ValidateReport,
        )?;
    }
    encode_len(&mut output, report.correlations.len());
    for correlation in &report.correlations {
        encode_correlation(&mut output, correlation);
        completed = completed.saturating_add(1);
        crate::work::checkpoint(
            control,
            context,
            AnalysisStage::EncodeReport,
            completed,
            total,
            DebuggerOperation::ValidateReport,
        )?;
    }
    encode_health(&mut output, &report.health);
    completed = completed.saturating_add(1);
    crate::work::checkpoint(
        control,
        context,
        AnalysisStage::EncodeReport,
        completed,
        total,
        DebuggerOperation::ValidateReport,
    )?;
    encode_len(&mut output, report.claims.len());
    for claim in &report.claims {
        encode_claim(&mut output, claim);
        completed = completed.saturating_add(1);
        crate::work::checkpoint(
            control,
            context,
            AnalysisStage::EncodeReport,
            completed,
            total,
            DebuggerOperation::ValidateReport,
        )?;
    }
    Ok(output.finish())
}

fn encode_claim(output: &mut PageSink, claim: &ReportClaim) {
    output.write(claim.id.as_bytes());
    encode_claim_content(output, &claim.content);
}

fn encode_claim_content(output: &mut PageSink, content: &ClaimContent) {
    match content {
        ClaimContent::Observation { statement, support } => {
            output.byte(1);
            encode_text(output, statement);
            encode_citations(output, support);
        }
        ClaimContent::Inference {
            statement,
            support,
            contrary,
            alternatives,
            confidence,
            category,
        } => {
            output.byte(2);
            encode_text(output, statement);
            encode_citations(output, support);
            encode_citations(output, contrary);
            encode_alternatives(output, alternatives);
            output.write(&confidence.value().to_be_bytes());
            encode_confidence_basis(output, confidence.basis());
            output.write(&category.tag().to_be_bytes());
        }
        ClaimContent::Recommendation { statement, support, parent, affected_components } => {
            output.byte(3);
            encode_text(output, statement);
            encode_citations(output, support);
            output.write(parent.as_bytes());
            encode_len(output, affected_components.len());
            for component in affected_components {
                output.byte(component.tag());
            }
        }
        ClaimContent::Unsupported(value) => {
            output.byte(4);
            output.write(value.proposal_digest().as_bytes());
            output.byte(value.reason() as u8);
        }
    }
}

fn encode_timeline(output: &mut PageSink, timeline: &crate::Timeline) {
    output.write(timeline.subject_id().as_bytes());
    encode_len(output, timeline.entries().len());
    for entry in timeline.entries() {
        encode_citation(output, entry.citation());
        output.write(entry.span_id().as_bytes());
        encode_boundary(output, entry.boundary());
        output.write(&entry.outcome().map_or(0, crate::OutcomeClass::tag).to_be_bytes());
        encode_len(output, entry.resources().len());
        for resource in entry.resources() {
            output.write(&safe_key_tag(resource.key()).to_be_bytes());
            encode_safe_value(output, resource.value());
        }
        encode_len(output, entry.predecessor_indices().len());
        for index in entry.predecessor_indices() {
            output.write(&index.to_be_bytes());
        }
        encode_len(output, entry.missing_predecessors().len());
        for event in entry.missing_predecessors() {
            output.write(event.as_bytes());
        }
        output.write(&entry.monotonic_tick().to_be_bytes());
        output.write(&entry.unix_nanos().to_be_bytes());
    }
    encode_len(output, timeline.clock_ambiguities().len());
    for ambiguity in timeline.clock_ambiguities() {
        encode_citation(output, ambiguity.earlier());
        encode_citation(output, ambiguity.later());
        output.write(&ambiguity.earlier_unix_nanos().to_be_bytes());
        output.write(&ambiguity.later_unix_nanos().to_be_bytes());
    }
}

fn encode_cause(output: &mut PageSink, cause: &crate::RootCauseCandidate) {
    output.write(cause.id().as_bytes());
    output.write(&cause.category().tag().to_be_bytes());
    encode_text(output, cause.statement());
    encode_citations(output, cause.support());
    encode_citations(output, cause.contrary());
    encode_alternatives(output, cause.alternatives());
    output.write(&cause.confidence().value().to_be_bytes());
    encode_confidence_basis(output, cause.confidence().basis());
    encode_len(output, cause.ambiguities().len());
    for value in cause.ambiguities() {
        output.byte(ambiguity_tag(*value));
    }
    match cause.derivation() {
        crate::CauseDerivation::Deterministic => output.byte(1),
        crate::CauseDerivation::ValidatedModel(id) => {
            output.byte(2);
            output.write(id.as_bytes());
        }
    }
}

fn encode_pattern(output: &mut PageSink, pattern: &crate::PatternCluster) {
    output.write(pattern.id().as_bytes());
    output.byte(pattern.kind() as u8);
    output.write(pattern.fingerprint().digest().as_bytes());
    encode_len(output, pattern.source_fingerprints().len());
    for source in pattern.source_fingerprints() {
        output.write(source.digest().as_bytes());
    }
    encode_len(output, pattern.members().len());
    for member in pattern.members() {
        output.write(member.subject_id().as_bytes());
        output.write(&member.outcome().tag().to_be_bytes());
        output.write(&member.category().map_or(0, crate::FailureCategory::tag).to_be_bytes());
        output.byte(member.analyzer() as u8);
        output.write(member.environment_id().as_bytes());
        output.write(member.harness_revision().digest().as_bytes());
        output.write(&member.workspace_revision().get().to_be_bytes());
        output.write(member.provider_profile_id().as_bytes());
        output.byte(
            member.component_kind().map_or(0, peritus_harness::domain::ComponentKind::tag),
        );
        encode_citations(output, member.citations());
        output.write(member.fingerprint().digest().as_bytes());
    }
}

fn encode_correlation(output: &mut PageSink, value: &crate::ComponentCorrelation) {
    output.write(value.pattern_id().as_bytes());
    output.byte(u8::from(value.component_id().is_some()));
    if let Some(id) = value.component_id() {
        encode_blob(output, id.as_str().as_bytes());
    }
    output.byte(value.component_kind().tag());
    output.byte(u8::from(value.content_digest().is_some()));
    if let Some(digest) = value.content_digest() {
        output.write(digest.as_bytes());
    }
    output.byte(value.protection_class().tag());
    output.byte(value.basis() as u8);
    encode_subjects(output, value.supporting_subjects());
    encode_subjects(output, value.contrary_subjects());
    output.byte(value.constraint() as u8);
    output.byte(u8::from(value.class_only()));
}

fn encode_health(output: &mut PageSink, value: &crate::HarnessHealthSummary) {
    output.byte(match value.status() {
        DiagnosticStatus::DiagnosticOnly => 1,
    });
    encode_len(output, value.revisions().len());
    for revision in value.revisions() {
        output.write(revision.harness_id().as_bytes());
        output.write(&revision.number().get().to_be_bytes());
        output.write(revision.digest().as_bytes());
    }
    for count in [
        value.subject_count(),
        value.successful_attempts(),
        value.failed_attempts(),
        value.indeterminate_attempts(),
        value.exact_component_correlations(),
        value.class_only_correlations(),
    ] {
        output.write(&count.to_be_bytes());
    }
    for ratio in [
        value.subject_coverage_millionths(),
        value.infrastructure_share_millionths(),
        value.repeated_pattern_share_millionths(),
        value.citation_coverage_millionths(),
        value.ambiguity_share_millionths(),
    ] {
        output.write(&ratio.to_be_bytes());
    }
    encode_len(output, value.category_counts().len());
    for count in value.category_counts() {
        output.write(&count.category().tag().to_be_bytes());
        output.write(&count.count().to_be_bytes());
    }
}

fn encode_len(output: &mut PageSink, length: usize) {
    output.write(&u64::try_from(length).unwrap_or(u64::MAX).to_be_bytes());
}

fn encode_blob(output: &mut PageSink, value: &[u8]) {
    encode_len(output, value.len());
    output.write(value);
}

fn encode_text(output: &mut PageSink, text: &crate::DiagnosticText) {
    encode_blob(output, text.as_str().as_bytes());
}

fn encode_citation(output: &mut PageSink, value: &EvidenceCitation) {
    value.encode_to(|part| output.write(part));
}

fn encode_citations(output: &mut PageSink, values: &[EvidenceCitation]) {
    encode_len(output, values.len());
    for value in values {
        encode_citation(output, value);
    }
}

fn encode_subjects(output: &mut PageSink, values: &[crate::SubjectId]) {
    encode_len(output, values.len());
    for value in values {
        output.write(value.as_bytes());
    }
}

fn encode_alternatives(output: &mut PageSink, value: &AlternativeCauses) {
    match value {
        AlternativeCauses::NoneKnown => output.byte(0),
        AlternativeCauses::Categories(values) => {
            output.byte(1);
            encode_len(output, values.len());
            for value in values {
                output.write(&value.tag().to_be_bytes());
            }
        }
    }
}

fn encode_confidence_basis(output: &mut PageSink, value: crate::ConfidenceBasis) {
    for count in [
        value.support_count(),
        value.contrary_count(),
        value.ambiguity_count(),
        value.recurrence_count(),
        value.maximum_causal_distance(),
    ] {
        output.write(&count.to_be_bytes());
    }
}

fn encode_optional_id(output: &mut PageSink, value: Option<[u8; 16]>) {
    output.byte(u8::from(value.is_some()));
    if let Some(value) = value {
        output.write(&value);
    }
}

fn encode_boundary(output: &mut PageSink, value: crate::BoundaryKind) {
    match value {
        crate::BoundaryKind::Started(kind) => {
            output.byte(1);
            output.byte(crate::query::span_kind_tag(kind));
        }
        crate::BoundaryKind::Diagnostic(code) => {
            output.byte(2);
            output.write(&crate::query::diagnostic_tag(code).to_be_bytes());
        }
        crate::BoundaryKind::Ended(outcome) => {
            output.byte(3);
            output.byte(crate::query::span_outcome_tag(outcome));
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

fn encode_safe_value(output: &mut PageSink, value: peritus_trace::SafeAttributeValue) {
    use peritus_trace::SafeAttributeValue as V;
    match value {
        V::Count(value) => {
            output.byte(1);
            output.write(&value.to_be_bytes());
        }
        V::DurationNanos(value) => {
            output.byte(2);
            output.write(&value.to_be_bytes());
        }
        V::Identifier(value) => {
            output.byte(3);
            output.write(&value);
        }
        V::Digest(value) => {
            output.byte(4);
            output.write(value.as_bytes());
        }
        V::Status(value) => {
            output.byte(5);
            output.byte(status_tag(value));
        }
        V::Vault(value) => {
            output.byte(6);
            output.write(value.digest().as_bytes());
            output.write(&value.size().to_be_bytes());
            output.write(value.creating_event().as_bytes());
            output.write(value.key_reference().as_bytes());
            output.write(value.parameters_digest().as_bytes());
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
