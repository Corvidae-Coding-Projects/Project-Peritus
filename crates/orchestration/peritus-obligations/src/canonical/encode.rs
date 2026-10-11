//! Verified executable encoder for the canonical ledger preimage.

use crate::{ObligationSpec, PathMention, RequirementEntry, RequirementLedger, SchemaField};

fn append_slice(output: &mut Vec<u8>, value: &[u8]) {
    let mut index = 0;
    while index < value.len() {
        output.push(value[index]);
        index += 1;
    }
}
fn append_u64(output: &mut Vec<u8>, value: u64) {
    let little = value.to_le_bytes();
    output.push(little[7]);
    output.push(little[6]);
    output.push(little[5]);
    output.push(little[4]);
    output.push(little[3]);
    output.push(little[2]);
    output.push(little[1]);
    output.push(little[0]);
}
fn append_u32(output: &mut Vec<u8>, value: u32) {
    let little = value.to_le_bytes();
    output.push(little[3]);
    output.push(little[2]);
    output.push(little[1]);
    output.push(little[0]);
}
fn append_usize(output: &mut Vec<u8>, value: usize) {
    let bounded = value as u64;
    append_u64(output, bounded);
}
fn append_bytes(output: &mut Vec<u8>, value: &[u8]) {
    append_usize(output, value.len());
    append_slice(output, value);
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
fn append_performance_expectation(output: &mut Vec<u8>, value: crate::PerformanceExpectation) {
    let (tag, threshold) = match value {
        crate::PerformanceExpectation::CandidateAtMost(value) => (1, value),
        crate::PerformanceExpectation::CandidateAtLeast(value) => (2, value),
        crate::PerformanceExpectation::ImprovementAtLeast(value) => (3, value),
        crate::PerformanceExpectation::RegressionAtMost(value) => (4, value),
    };
    output.push(tag);
    append_u64(output, threshold);
}
fn append_fields(output: &mut Vec<u8>, fields: &[SchemaField]) {
    let mut index = 0;
    while index < fields.len() {
        let field = &fields[index];
        append_slice(output, field.id().digest().as_bytes());
        append_bytes(output, field.exact_name());
        index += 1;
    }
}
fn append_specification(output: &mut Vec<u8>, specification: &ObligationSpec) {
    output.push(specification_tag(specification));
    match specification {
        ObligationSpec::Conditional { condition_id } => {
            append_slice(output, condition_id.digest().as_bytes());
        }
        ObligationSpec::Alternative { group_id, branch_id } => {
            append_slice(output, group_id.digest().as_bytes());
            append_slice(output, branch_id.digest().as_bytes());
        }
        ObligationSpec::Performance(requirement) => {
            append_slice(output, requirement.workload_identity().as_bytes());
            output.push(performance_statistic_tag(requirement.statistic()));
            append_u32(output, requirement.minimum_repetitions());
            append_performance_expectation(output, requirement.public_threshold());
        }
        ObligationSpec::LifecycleIngress(requirement) => {
            append_slice(output, requirement.named_ingress().as_bytes());
            append_slice(output, requirement.control_event().as_bytes());
            append_slice(output, requirement.expected_transition().as_bytes());
            append_slice(output, requirement.final_state().as_bytes());
        }
        ObligationSpec::RequestSchema(requirement)
        | ObligationSpec::ResponseSchema(requirement) => {
            output.push(schema_direction_tag(requirement.direction()));
            append_usize(output, requirement.fields().len());
            append_fields(output, requirement.fields());
        }
        ObligationSpec::BrowserSemantics(requirement) => {
            append_slice(output, requirement.oracle_identity().as_bytes());
        }
        ObligationSpec::ExternalEffect { effect_identity } => {
            append_slice(output, effect_identity.as_bytes());
        }
        ObligationSpec::Hard | ObligationSpec::Example | ObligationSpec::GeneratedOutput => {}
    }
}
fn append_paths(output: &mut Vec<u8>, paths: &[PathMention]) {
    let mut index = 0;
    while index < paths.len() {
        let path = &paths[index];
        append_slice(output, path.id().digest().as_bytes());
        output.push(path_role_tag(path.role()));
        append_bytes(output, path.exact());
        index += 1;
    }
}
fn append_entry(output: &mut Vec<u8>, entry: &RequirementEntry) {
    append_slice(output, entry.id().digest().as_bytes());
    append_bytes(output, entry.clause().exact());
    let provenance = entry.clause().provenance();
    append_slice(output, provenance.source_digest().as_bytes());
    append_u64(output, provenance.conversation_revision());
    append_u32(output, provenance.ordinal());
    append_usize(output, provenance.byte_start());
    append_usize(output, provenance.byte_end());
    append_specification(output, entry.specification());
    append_usize(output, entry.paths().len());
    append_paths(output, entry.paths());
}
fn append_entries(output: &mut Vec<u8>, entries: &[RequirementEntry]) {
    let mut index = 0;
    while index < entries.len() {
        let entry = &entries[index];
        append_entry(output, entry);
        index += 1;
    }
}
pub fn ledger_bytes(ledger: &RequirementLedger) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(1_024);
    let domain = vec![
        112u8, 101u8, 114u8, 105u8, 116u8, 117u8, 115u8, 45u8, 114u8, 101u8, 113u8, 117u8, 105u8,
        114u8, 101u8, 109u8, 101u8, 110u8, 116u8, 45u8, 108u8, 101u8, 100u8, 103u8, 101u8, 114u8,
        45u8, 118u8, 49u8, 0u8,
    ];
    append_slice(&mut bytes, domain.as_slice());
    append_slice(&mut bytes, ledger.source_digest().as_bytes());
    append_u64(&mut bytes, ledger.conversation_revision());
    append_usize(&mut bytes, ledger.entries().len());
    append_entries(&mut bytes, ledger.entries());
    bytes
}
