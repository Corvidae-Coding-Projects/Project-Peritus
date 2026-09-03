//! Canonical digest preimage for one exact requirement ledger.

use crate::{ObligationSpec, RequirementLedger};
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};

const DOMAIN: &[u8] = b"peritus-requirement-ledger-v1\0";

/// Hashes exact canonical bytes without assigning authenticity semantics.
pub fn sha256(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::new(Sha256::digest(bytes).into())
}

pub fn ledger_bytes(ledger: &RequirementLedger) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(1_024);
    bytes.extend_from_slice(DOMAIN);
    bytes.extend_from_slice(ledger.source_digest().as_bytes());
    bytes.extend_from_slice(&ledger.conversation_revision().to_be_bytes());
    append_len(&mut bytes, ledger.entries().len());
    for entry in ledger.entries() {
        bytes.extend_from_slice(entry.id().digest().as_bytes());
        append_bytes(&mut bytes, entry.clause().exact());
        let provenance = entry.clause().provenance();
        bytes.extend_from_slice(provenance.source_digest().as_bytes());
        bytes.extend_from_slice(&provenance.conversation_revision().to_be_bytes());
        bytes.extend_from_slice(&provenance.ordinal().to_be_bytes());
        append_usize(&mut bytes, provenance.byte_start());
        append_usize(&mut bytes, provenance.byte_end());
        append_specification(&mut bytes, entry.specification());
        append_len(&mut bytes, entry.paths().len());
        for path in entry.paths() {
            bytes.extend_from_slice(path.id().digest().as_bytes());
            bytes.push(path_role_tag(path.role()));
            append_bytes(&mut bytes, path.exact());
        }
    }
    bytes
}

fn append_specification(bytes: &mut Vec<u8>, specification: &ObligationSpec) {
    bytes.push(specification_tag(specification));
    match specification {
        ObligationSpec::Conditional { condition_id } => {
            bytes.extend_from_slice(condition_id.digest().as_bytes());
        }
        ObligationSpec::Alternative { group_id, branch_id } => {
            bytes.extend_from_slice(group_id.digest().as_bytes());
            bytes.extend_from_slice(branch_id.digest().as_bytes());
        }
        ObligationSpec::Performance(requirement) => {
            bytes.extend_from_slice(requirement.workload_identity().as_bytes());
            bytes.push(performance_statistic_tag(requirement.statistic()));
            bytes.extend_from_slice(&requirement.minimum_repetitions().to_be_bytes());
            append_performance_expectation(bytes, requirement.public_threshold());
        }
        ObligationSpec::LifecycleIngress(requirement) => {
            bytes.extend_from_slice(requirement.named_ingress().as_bytes());
            bytes.extend_from_slice(requirement.control_event().as_bytes());
            bytes.extend_from_slice(requirement.expected_transition().as_bytes());
            bytes.extend_from_slice(requirement.final_state().as_bytes());
        }
        ObligationSpec::RequestSchema(requirement)
        | ObligationSpec::ResponseSchema(requirement) => {
            bytes.push(schema_direction_tag(requirement.direction()));
            append_len(bytes, requirement.fields().len());
            for field in requirement.fields() {
                bytes.extend_from_slice(field.id().digest().as_bytes());
                append_bytes(bytes, field.exact_name());
            }
        }
        ObligationSpec::BrowserSemantics(requirement) => {
            bytes.extend_from_slice(requirement.oracle_identity().as_bytes());
        }
        ObligationSpec::ExternalEffect { effect_identity } => {
            bytes.extend_from_slice(effect_identity.as_bytes());
        }
        ObligationSpec::Hard | ObligationSpec::Example | ObligationSpec::GeneratedOutput => {}
    }
}

const fn specification_tag(value: &ObligationSpec) -> u8 {
    match value {
        ObligationSpec::Hard => 1,
        ObligationSpec::Conditional { .. } => 2,
        ObligationSpec::Alternative { .. } => 3,
        ObligationSpec::Example => 4,
        ObligationSpec::GeneratedOutput => 5,
        ObligationSpec::Performance(_) => 6,
        ObligationSpec::LifecycleIngress(_) => 7,
        ObligationSpec::RequestSchema(_) => 8,
        ObligationSpec::ResponseSchema(_) => 9,
        ObligationSpec::BrowserSemantics(_) => 10,
        ObligationSpec::ExternalEffect { .. } => 11,
    }
}

const fn path_role_tag(value: crate::PathRole) -> u8 {
    match value {
        crate::PathRole::RequiredOutput => 1,
        crate::PathRole::RequiredModification => 2,
        crate::PathRole::RequiredInput => 3,
        crate::PathRole::Reference => 4,
        crate::PathRole::Example => 5,
    }
}

const fn schema_direction_tag(value: crate::SchemaDirection) -> u8 {
    match value {
        crate::SchemaDirection::Request => 1,
        crate::SchemaDirection::Response => 2,
    }
}

const fn performance_statistic_tag(value: crate::PerformanceStatistic) -> u8 {
    match value {
        crate::PerformanceStatistic::Mean => 1,
        crate::PerformanceStatistic::Median => 2,
        crate::PerformanceStatistic::Minimum => 3,
        crate::PerformanceStatistic::Maximum => 4,
        crate::PerformanceStatistic::Percentile95 => 5,
        crate::PerformanceStatistic::Percentile99 => 6,
    }
}

fn append_performance_expectation(bytes: &mut Vec<u8>, value: crate::PerformanceExpectation) {
    let (tag, threshold) = match value {
        crate::PerformanceExpectation::CandidateAtMost(value) => (1, value),
        crate::PerformanceExpectation::CandidateAtLeast(value) => (2, value),
        crate::PerformanceExpectation::ImprovementAtLeast(value) => (3, value),
        crate::PerformanceExpectation::RegressionAtMost(value) => (4, value),
    };
    bytes.push(tag);
    bytes.extend_from_slice(&threshold.to_be_bytes());
}

fn append_bytes(output: &mut Vec<u8>, value: &[u8]) {
    append_len(output, value.len());
    output.extend_from_slice(value);
}

fn append_len(output: &mut Vec<u8>, value: usize) {
    append_usize(output, value);
}

fn append_usize(output: &mut Vec<u8>, value: usize) {
    output.extend_from_slice(&u64::try_from(value).unwrap_or(u64::MAX).to_be_bytes());
}
