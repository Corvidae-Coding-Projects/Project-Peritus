//! Direct universal meaning of the production qualification verdict.

mod alternatives;
mod ordinary;

use super::model;
use crate::{ConditionObservation, QualificationReport, RequirementEvidence, RequirementLedger};
use peritus_run_settlement::CandidateIdentity;
use vstd::prelude::*;

verus! {

/// Every active ordinary obligation has satisfying current evidence, every relevant condition
/// is resolved, and every represented alternative group has a complete witnessed branch.
/// This predicate refers directly to ledger entries and supplied evidence, without report counts.
pub open spec fn qualification_satisfied(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    conditions: Seq<ConditionObservation>,
    evidence: Seq<RequirementEvidence>,
) -> bool {
    &&& ordinary::active_entries_satisfied(
        ledger, candidate, ledger.spec_entries(), conditions, evidence)
    &&& ordinary::all_conditions_resolved(ledger.spec_entries(), conditions)
    &&& alternatives::alternative_entries_satisfied(
        ledger, candidate, evidence, ledger.spec_entries())
}

pub(super) proof fn report_verdict_complete(
    report: &QualificationReport,
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    conditions: Seq<ConditionObservation>,
    evidence: Seq<RequirementEvidence>,
)
    requires model::report_refines(report, ledger, candidate, conditions, evidence),
    ensures report.spec_qualified() == qualification_satisfied(ledger, candidate, conditions, evidence),
{
    let entries = ledger.spec_entries();
    let groups = model::alternative_groups(entries);
    ordinary::ordinary_accounting_complete(ledger, candidate, entries, conditions, evidence);
    alternatives::group_accounting_complete(ledger, candidate, evidence, groups);
    alternatives::group_enumeration_complete(ledger, candidate, evidence, entries);
}

} // verus!
