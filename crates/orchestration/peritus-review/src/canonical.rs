//! Domain-canonical D2 hashing.

mod encoder;

use peritus_spec::{FindingSeverity, WaiverPolicy};
use peritus_types::Sha256Digest;

use crate::{
    DispositionKind, DispositionRecord, Finding, OscillationKind, OscillationReport,
    QuorumDimension, QuorumReport, ReviewAssignment, ReviewBinding, ReviewCycle, ReviewCyclePhase,
    ReviewLimits, ReviewRunPhase, ReviewRunState, ReviewSubmission, ReviewTerminal,
    ReviewTerminalKind,
};

use encoder::Encoder;

/// Hashes every immutable review-binding field except the digest itself.
#[must_use]
pub fn binding_digest(binding: &ReviewBinding) -> Sha256Digest {
    let mut out = Encoder::new(b"peritus-d2-review-binding-v1\0");
    out.raw(binding.contract_id().as_bytes());
    out.digest(binding.contract_digest());
    out.revision(binding.revision());
    out.len(binding.required_categories().len());
    for category in binding.required_categories() {
        out.digest(category.digest());
    }
    out.u16(binding.reviewer_quorum());
    let independence = binding.independence();
    out.bool(independence.distinct_reviewers());
    out.bool(independence.independent_from_producer());
    out.bool(independence.distinct_contexts());
    out.bool(independence.distinct_model_families());
    out.bool(independence.distinct_providers());
    out.bool(independence.no_shared_ancestry());
    out.bool(independence.fresh_context());
    out.u8(severity_tag(binding.blocking_severity()));
    out.u16(binding.maximum_cycles());
    match binding.waiver_policy() {
        WaiverPolicy::Forbidden => out.u8(1),
        WaiverPolicy::Allowed { authority, evidence } => {
            out.u8(2);
            out.digest(authority.digest());
            out.digest(evidence.digest());
        }
    }
    out.digest(binding.candidate_digest());
    out.digest(binding.tree_digest());
    out.len(binding.producer_actors().len());
    for actor in binding.producer_actors() {
        out.raw(actor.as_bytes());
    }
    out.len(binding.producer_ancestries().len());
    for ancestry in binding.producer_ancestries() {
        out.digest(*ancestry);
    }
    out.hash()
}

/// Hashes the semantic defect fingerprint, independent of identity, provenance, and revision.
#[must_use]
pub fn finding_digest(finding: &Finding) -> Sha256Digest {
    let mut out = Encoder::new(b"peritus-d2-finding-v1\0");
    encode_finding_body(&mut out, finding);
    out.hash()
}

/// Hashes one complete normalized submission except its digest field.
#[must_use]
pub fn submission_digest(submission: &ReviewSubmission) -> Sha256Digest {
    let mut out = Encoder::new(b"peritus-d2-review-submission-v1\0");
    encode_submission(&mut out, submission);
    out.hash()
}

/// Hashes every complete state field while logically zeroing the state-digest field.
#[must_use]
pub fn state_digest(state: &ReviewRunState) -> Sha256Digest {
    let mut out = Encoder::new(b"peritus-d2-review-state-v1\0");
    out.raw(state.run_id().as_bytes());
    encode_limits(&mut out, state.limits());
    encode_binding(&mut out, state.binding());
    out.u8(run_phase_tag(state.phase()));
    out.u64(state.sequence().get());
    out.raw(state.last_event_id().as_bytes());
    out.len(state.cycles().len());
    for cycle in state.cycles() {
        encode_cycle(&mut out, cycle);
    }
    out.len(state.findings().len());
    for finding in state.findings() {
        encode_finding(&mut out, finding);
    }
    out.len(state.waivers().len());
    for waiver in state.waivers() {
        encode_waiver(&mut out, *waiver);
    }
    encode_quorum(&mut out, state.quorum());
    encode_oscillation(&mut out, state.oscillation());
    out.len(state.used_commands().len());
    for command in state.used_commands() {
        out.raw(command.as_bytes());
    }
    out.option(state.terminal(), encode_terminal);
    out.hash()
}

/// Hashes one terminal summary except its digest field.
#[must_use]
pub fn terminal_digest(terminal: &ReviewTerminal) -> Sha256Digest {
    let mut out = Encoder::new(b"peritus-d2-review-terminal-v1\0");
    encode_terminal_fields(&mut out, terminal);
    out.hash()
}

pub const fn severity_tag(value: FindingSeverity) -> u8 {
    match value {
        FindingSeverity::Advisory => 1,
        FindingSeverity::Low => 2,
        FindingSeverity::Medium => 3,
        FindingSeverity::High => 4,
        FindingSeverity::Critical => 5,
    }
}

pub const fn disposition_tag(value: DispositionKind) -> u8 {
    match value {
        DispositionKind::Open => 1,
        DispositionKind::Fixed => 2,
        DispositionKind::Disputed => 3,
        DispositionKind::SupersessionProposed => 4,
        DispositionKind::WaiverRequested => 5,
        DispositionKind::ResolutionConfirmed => 6,
        DispositionKind::InvalidationConfirmed => 7,
        DispositionKind::Superseded => 8,
        DispositionKind::Waived => 9,
    }
}

pub const fn run_phase_tag(value: ReviewRunPhase) -> u8 {
    value as u8
}

pub const fn terminal_kind_tag(value: ReviewTerminalKind) -> u8 {
    match value {
        ReviewTerminalKind::Completed => 1,
        ReviewTerminalKind::NeedsHuman => 2,
        ReviewTerminalKind::Failed => 3,
        ReviewTerminalKind::Cancelled => 4,
    }
}

pub const fn cycle_phase_tag(value: ReviewCyclePhase) -> u8 {
    match value {
        ReviewCyclePhase::Assigned => 1,
        ReviewCyclePhase::Submitted => 2,
        ReviewCyclePhase::Cancelled => 3,
        ReviewCyclePhase::Invalidated => 4,
    }
}

pub const fn oscillation_tag(value: OscillationKind) -> u8 {
    match value {
        OscillationKind::RepeatedFindingSet => 1,
        OscillationKind::SeverityStagnation => 2,
        OscillationKind::SeverityRegression => 3,
        OscillationKind::Disagreement => 4,
        OscillationKind::ReviewCyclesExhausted => 5,
    }
}

fn encode_limits(out: &mut Encoder, value: ReviewLimits) {
    out.u16(value.cycles());
    out.u16(value.assignments());
    out.u16(value.submissions());
    out.u32(value.findings());
    out.u16(value.categories());
    out.u16(value.requirements());
    out.u16(value.locations());
    out.u16(value.evidence_references());
    out.u16(value.provenance_sources());
    out.u16(value.disposition_records());
    out.u32(value.path_bytes());
    out.u32(value.text_bytes());
    out.u32(value.opaque_bytes());
    out.u64(value.payload_bytes());
    out.u64(value.state_bytes());
}

fn encode_binding(out: &mut Encoder, value: &ReviewBinding) {
    out.raw(value.contract_id().as_bytes());
    out.digest(value.contract_digest());
    out.revision(value.revision());
    out.len(value.required_categories().len());
    for category in value.required_categories() {
        out.digest(category.digest());
    }
    out.u16(value.reviewer_quorum());
    encode_independence(out, value.independence());
    out.u8(severity_tag(value.blocking_severity()));
    out.u16(value.maximum_cycles());
    match value.waiver_policy() {
        WaiverPolicy::Forbidden => out.u8(1),
        WaiverPolicy::Allowed { authority, evidence } => {
            out.u8(2);
            out.digest(authority.digest());
            out.digest(evidence.digest());
        }
    }
    out.digest(value.candidate_digest());
    out.digest(value.tree_digest());
    out.len(value.producer_actors().len());
    for actor in value.producer_actors() {
        out.raw(actor.as_bytes());
    }
    out.len(value.producer_ancestries().len());
    for ancestry in value.producer_ancestries() {
        out.digest(*ancestry);
    }
    out.digest(value.digest());
}

fn encode_independence(out: &mut Encoder, value: peritus_role::ReviewIndependenceView) {
    out.bool(value.distinct_reviewers());
    out.bool(value.independent_from_producer());
    out.bool(value.distinct_contexts());
    out.bool(value.distinct_model_families());
    out.bool(value.distinct_providers());
    out.bool(value.no_shared_ancestry());
    out.bool(value.fresh_context());
}

fn encode_assignment(out: &mut Encoder, value: &ReviewAssignment) {
    out.raw(value.cycle_id().as_bytes());
    out.u16(value.ordinal().get());
    out.digest(value.binding_digest());
    out.revision(value.revision());
    let reviewer = value.reviewer();
    out.raw(reviewer.actor_id().as_bytes());
    out.digest(reviewer.provider());
    out.digest(reviewer.model_family());
    out.digest(reviewer.prompt_revision());
    out.digest(reviewer.context());
    out.digest(reviewer.ancestry());
    out.bool(reviewer.independent_from_producer());
    out.len(value.categories().len());
    for category in value.categories() {
        out.digest(category.digest());
    }
    out.digest(value.context_plan_id().digest());
    out.bool(value.fresh_context());
    encode_independence(out, value.independence());
}

fn encode_cycle(out: &mut Encoder, value: &ReviewCycle) {
    encode_assignment(out, value.assignment());
    out.u8(cycle_phase_tag(value.phase()));
    out.option(value.submission(), encode_submission);
}

fn encode_waiver(out: &mut Encoder, value: crate::ObservedWaiver) {
    let observation = value.observation();
    out.raw(observation.finding_id().as_bytes());
    out.revision(observation.revision());
    out.raw(observation.approval_request_id().as_bytes());
    out.digest(observation.authority().digest());
    out.digest(observation.evidence_requirement_id().digest());
    out.digest(observation.waiver_digest());
    out.digest(value.request_digest());
}

fn encode_quorum(out: &mut Encoder, value: &QuorumReport) {
    out.u16(value.submitted_reviews());
    out.len(value.covered_categories().len());
    for category in value.covered_categories() {
        out.digest(category.digest());
    }
    for dimension in [
        QuorumDimension::SubmittedReviewCount,
        QuorumDimension::RequiredCategoryCoverage,
        QuorumDimension::DistinctReviewerIdentities,
        QuorumDimension::ProducerIndependence,
        QuorumDimension::DistinctContexts,
        QuorumDimension::DistinctModelFamilies,
        QuorumDimension::DistinctProviders,
        QuorumDimension::NoSharedAncestry,
        QuorumDimension::FreshContext,
    ] {
        out.bool(value.passes(dimension));
    }
}

fn encode_oscillation(out: &mut Encoder, value: &OscillationReport) {
    out.len(value.kinds().len());
    for kind in value.kinds() {
        out.u8(oscillation_tag(*kind));
    }
    out.u16(value.compared_bindings());
    out.u16(value.cycles_used());
}

fn encode_terminal(out: &mut Encoder, value: &ReviewTerminal) {
    encode_terminal_fields(out, value);
    out.digest(value.digest());
}

fn encode_terminal_fields(out: &mut Encoder, value: &ReviewTerminal) {
    out.u8(terminal_kind_tag(value.kind()));
    out.len(value.unconserved_findings().len());
    for finding in value.unconserved_findings() {
        out.raw(finding.as_bytes());
    }
    encode_quorum(out, value.quorum());
    encode_oscillation(out, value.oscillation());
    out.digest(value.cause_digest());
}

fn encode_submission(out: &mut Encoder, submission: &ReviewSubmission) {
    out.raw(submission.cycle_id().as_bytes());
    out.revision(submission.revision());
    out.len(submission.categories().len());
    for category in submission.categories() {
        out.digest(category.digest());
    }
    out.len(submission.findings().len());
    for finding in submission.findings() {
        encode_finding(out, finding);
    }
}

fn encode_finding(out: &mut Encoder, finding: &Finding) {
    out.raw(finding.id().as_bytes());
    encode_source(out, finding.origin());
    out.len(finding.sources().len());
    for source in finding.sources() {
        encode_source(out, *source);
    }
    encode_finding_body(out, finding);
    out.u16(finding.confidence().get());
    out.len(finding.evidence().len());
    for evidence in finding.evidence() {
        out.raw(evidence.as_bytes());
    }
    out.revision(finding.revision());
    out.digest(finding.normalized_digest());
    out.len(finding.dispositions().len());
    for disposition in finding.dispositions() {
        encode_disposition(out, disposition);
    }
    out.option(finding.superseded_by(), |out, id| out.raw(id.as_bytes()));
}

fn encode_finding_body(out: &mut Encoder, finding: &Finding) {
    out.digest(finding.category().digest());
    out.u8(severity_tag(finding.severity()));
    out.bool(finding.blocking());
    out.len(finding.requirements().len());
    for requirement in finding.requirements() {
        out.digest(requirement.digest());
    }
    out.len(finding.locations().len());
    for location in finding.locations() {
        out.text(location.path());
        out.u32(location.start_line());
        out.u32(location.start_column());
        out.u32(location.end_line());
        out.u32(location.end_column());
    }
    out.text(finding.description());
    out.text(finding.reproduction());
    out.text(finding.expected_behavior());
    out.text(finding.remediation());
}

fn encode_source(out: &mut Encoder, source: crate::FindingSource) {
    out.raw(source.cycle_id().as_bytes());
    out.raw(source.reviewer().as_bytes());
}

fn encode_disposition(out: &mut Encoder, record: &DispositionRecord) {
    out.raw(record.event_id().as_bytes());
    out.u8(disposition_tag(record.kind()));
    out.option(record.actor(), |out, actor| out.raw(actor.as_bytes()));
    out.option(record.reviewer_cycle(), |out, cycle| out.raw(cycle.as_bytes()));
    out.revision(record.revision());
    out.len(record.evidence().len());
    for evidence in record.evidence() {
        out.raw(evidence.as_bytes());
    }
    out.option(record.related_finding(), |out, finding| out.raw(finding.as_bytes()));
    out.option(record.approval_request_id(), |out, request| out.raw(request.as_bytes()));
    out.option(record.authority(), |out, authority| out.digest(authority.digest()));
    out.option(record.evidence_requirement_id(), |out, requirement| {
        out.digest(requirement.digest());
    });
    out.digest(record.record_digest());
}
