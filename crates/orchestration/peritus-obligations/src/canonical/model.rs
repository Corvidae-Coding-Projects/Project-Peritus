//! Mathematical definition of the exact ledger byte encoding.

use crate::{
    ObligationSpec, PathMention, PathRole, PerformanceExpectation, PerformanceStatistic,
    RequirementEntry, RequirementLedger, SchemaDirection, SchemaField,
};
use vstd::prelude::*;

verus! {

pub open spec fn singleton_encoding(value: u8) -> Seq<u8> {
    Seq::empty().push(value)
}

pub open spec fn u64_encoding(value: u64) -> Seq<u8> {
    let little = vstd::bytes::spec_u64_to_le_bytes(value);
    singleton_encoding(little[7]) + singleton_encoding(little[6])
        + singleton_encoding(little[5]) + singleton_encoding(little[4])
        + singleton_encoding(little[3]) + singleton_encoding(little[2])
        + singleton_encoding(little[1]) + singleton_encoding(little[0])
}

pub open spec fn u32_encoding(value: u32) -> Seq<u8> {
    let little = vstd::bytes::spec_u32_to_le_bytes(value);
    singleton_encoding(little[3]) + singleton_encoding(little[2])
        + singleton_encoding(little[1]) + singleton_encoding(little[0])
}

pub open spec fn usize_encoding(value: usize) -> Seq<u8> {
    u64_encoding(value as u64)
}

pub open spec fn byte_string_encoding(value: Seq<u8>) -> Seq<u8> {
    usize_encoding(value.len() as usize) + value
}

pub open spec fn specification_tag(value: &ObligationSpec) -> u8 {
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

pub open spec fn path_role_tag(value: PathRole) -> u8 {
    match value {
        PathRole::RequiredOutput => 1,
        PathRole::RequiredModification => 2,
        PathRole::RequiredInput => 3,
        PathRole::Reference => 4,
        PathRole::Example => 5,
    }
}

pub open spec fn schema_direction_tag(value: SchemaDirection) -> u8 {
    match value { SchemaDirection::Request => 1, SchemaDirection::Response => 2 }
}

pub open spec fn performance_statistic_tag(value: PerformanceStatistic) -> u8 {
    match value {
        PerformanceStatistic::Mean => 1,
        PerformanceStatistic::Median => 2,
        PerformanceStatistic::Minimum => 3,
        PerformanceStatistic::Maximum => 4,
        PerformanceStatistic::Percentile95 => 5,
        PerformanceStatistic::Percentile99 => 6,
    }
}

pub open spec fn performance_expectation_encoding(
    value: PerformanceExpectation,
) -> Seq<u8> {
    match value {
        PerformanceExpectation::CandidateAtMost(threshold) =>
            singleton_encoding(1u8) + u64_encoding(threshold),
        PerformanceExpectation::CandidateAtLeast(threshold) =>
            singleton_encoding(2u8) + u64_encoding(threshold),
        PerformanceExpectation::ImprovementAtLeast(threshold) =>
            singleton_encoding(3u8) + u64_encoding(threshold),
        PerformanceExpectation::RegressionAtMost(threshold) =>
            singleton_encoding(4u8) + u64_encoding(threshold),
    }
}

pub open spec fn fields_encoding(fields: Seq<SchemaField>) -> Seq<u8>
    decreases fields.len(),
{
    if fields.len() == 0 { Seq::empty() }
    else { fields_encoding(fields.drop_last()) + field_encoding(&fields.last()) }
}

pub open spec fn field_encoding(field: &SchemaField) -> Seq<u8> {
    field.spec_id().spec_digest().spec_bytes()@
        + byte_string_encoding(field.spec_exact_name())
}

pub open spec fn specification_encoding(value: &ObligationSpec) -> Seq<u8> {
    let tag = singleton_encoding(specification_tag(value));
    match value {
        ObligationSpec::Conditional { condition_id } =>
            tag + condition_id.spec_digest().spec_bytes()@,
        ObligationSpec::Alternative { group_id, branch_id } => tag
            + group_id.spec_digest().spec_bytes()@
            + branch_id.spec_digest().spec_bytes()@,
        ObligationSpec::Performance(requirement) => tag
            + requirement.spec_workload().spec_bytes()@
            + singleton_encoding(performance_statistic_tag(requirement.spec_statistic()))
            + u32_encoding(requirement.spec_repetitions())
            + performance_expectation_encoding(requirement.spec_threshold()),
        ObligationSpec::LifecycleIngress(requirement) => tag
            + requirement.spec_named_ingress().spec_bytes()@
            + requirement.spec_control_event().spec_bytes()@
            + requirement.spec_expected_transition().spec_bytes()@
            + requirement.spec_final_state().spec_bytes()@,
        ObligationSpec::RequestSchema(requirement)
        | ObligationSpec::ResponseSchema(requirement) => tag
            + singleton_encoding(schema_direction_tag(requirement.spec_direction()))
            + usize_encoding(requirement.spec_fields().len() as usize)
            + fields_encoding(requirement.spec_fields()),
        ObligationSpec::BrowserSemantics(requirement) =>
            tag + requirement.spec_oracle_identity().spec_bytes()@,
        ObligationSpec::ExternalEffect { effect_identity } =>
            tag + effect_identity.spec_bytes()@,
        ObligationSpec::Hard | ObligationSpec::Example | ObligationSpec::GeneratedOutput => tag,
    }
}

pub open spec fn paths_encoding(paths: Seq<PathMention>) -> Seq<u8>
    decreases paths.len(),
{
    if paths.len() == 0 { Seq::empty() }
    else { paths_encoding(paths.drop_last()) + path_encoding(&paths.last()) }
}

pub open spec fn path_encoding(path: &PathMention) -> Seq<u8> {
    path.spec_id().spec_digest().spec_bytes()@
        + singleton_encoding(path_role_tag(path.spec_role()))
        + byte_string_encoding(path.spec_exact())
}

pub open spec fn entry_encoding(entry: &RequirementEntry) -> Seq<u8> {
    let clause = entry.spec_clause();
    let provenance = clause.spec_provenance();
    entry.spec_id().spec_digest().spec_bytes()@
        + byte_string_encoding(clause.spec_exact())
        + provenance.spec_source_digest().spec_bytes()@
        + u64_encoding(provenance.spec_conversation_revision())
        + u32_encoding(provenance.spec_ordinal())
        + usize_encoding(provenance.spec_byte_start())
        + usize_encoding(provenance.spec_byte_end())
        + specification_encoding(&entry.spec_specification())
        + usize_encoding(entry.spec_paths().len() as usize)
        + paths_encoding(entry.spec_paths())
}

pub open spec fn entries_encoding(entries: Seq<RequirementEntry>) -> Seq<u8>
    decreases entries.len(),
{
    if entries.len() == 0 { Seq::empty() }
    else { entries_encoding(entries.drop_last()) + entry_encoding(&entries.last()) }
}

pub open spec fn domain_encoding() -> Seq<u8> {
    singleton_encoding(112u8) + singleton_encoding(101u8)
        + singleton_encoding(114u8) + singleton_encoding(105u8)
        + singleton_encoding(116u8) + singleton_encoding(117u8)
        + singleton_encoding(115u8) + singleton_encoding(45u8)
        + singleton_encoding(114u8) + singleton_encoding(101u8)
        + singleton_encoding(113u8) + singleton_encoding(117u8)
        + singleton_encoding(105u8) + singleton_encoding(114u8)
        + singleton_encoding(101u8) + singleton_encoding(109u8)
        + singleton_encoding(101u8) + singleton_encoding(110u8)
        + singleton_encoding(116u8) + singleton_encoding(45u8)
        + singleton_encoding(108u8) + singleton_encoding(101u8)
        + singleton_encoding(100u8) + singleton_encoding(103u8)
        + singleton_encoding(101u8) + singleton_encoding(114u8)
        + singleton_encoding(45u8) + singleton_encoding(118u8)
        + singleton_encoding(49u8) + singleton_encoding(0u8)
}

/// Exact deterministic preimage bytes for every retained ledger field.
pub open spec fn ledger_encoding(ledger: &RequirementLedger) -> Seq<u8> {
    domain_encoding()
        + ledger.spec_source_digest().spec_bytes()@
        + u64_encoding(ledger.spec_conversation_revision())
        + usize_encoding(ledger.spec_entries().len() as usize)
        + entries_encoding(ledger.spec_entries())
}

pub(crate) proof fn fields_after_push(fields: Seq<SchemaField>, field: SchemaField)
    ensures fields_encoding(fields.push(field)) == fields_encoding(fields) + field_encoding(&field),
{
    assert(fields.push(field).drop_last() == fields);
    assert(fields.push(field).last() == field);
}

pub(crate) proof fn paths_after_push(paths: Seq<PathMention>, path: PathMention)
    ensures paths_encoding(paths.push(path)) == paths_encoding(paths) + path_encoding(&path),
{
    assert(paths.push(path).drop_last() == paths);
    assert(paths.push(path).last() == path);
}

pub(crate) proof fn entries_after_push(entries: Seq<RequirementEntry>, entry: RequirementEntry)
    ensures entries_encoding(entries.push(entry))
        == entries_encoding(entries) + entry_encoding(&entry),
{
    assert(entries.push(entry).drop_last() == entries);
    assert(entries.push(entry).last() == entry);
}

} // verus!
