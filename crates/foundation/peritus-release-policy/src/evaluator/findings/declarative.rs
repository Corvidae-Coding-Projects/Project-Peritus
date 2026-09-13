//! Quantified finding and waiver admission over the supplied observations.

use crate::{FindingObservation, ReleaseCandidate, ReleaseEvidence, WaiverObservation};
use vstd::prelude::*;

verus! {

/// Every supplied finding has an exact current, nonblocking disposition and valid waiver when used.
pub open spec fn all_finding_inputs_ready(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    forall |index: int| 0 <= index < evidence.spec_findings().len() ==>
        super::model::finding_base_clear(
            evidence,
            #[trigger] evidence.spec_findings()[index],
            candidate,
            evaluated_at,
        )
}

/// Every earlier finding with the same current identity agrees with the later observation.
pub open spec fn all_finding_pairs_ready(
    findings: Seq<FindingObservation>,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    forall |left: int, right: int| 0 <= left < right < findings.len() ==>
        #[trigger] super::model::finding_pair_clear(
            findings[left], findings[right], candidate, evaluated_at)
}

/// Every supplied waiver is current, approved, and targets an eligible finding.
pub open spec fn all_waiver_inputs_ready(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    forall |index: int| 0 <= index < evidence.spec_waivers().len() ==>
        super::model::waiver_base_clear(
            evidence,
            #[trigger] evidence.spec_waivers()[index],
            candidate,
            evaluated_at,
        )
}

/// Every pair of current waivers for one finding identity agrees exactly.
pub open spec fn all_waiver_pairs_ready(
    waivers: Seq<WaiverObservation>,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    forall |left: int, right: int| 0 <= left < right < waivers.len() ==>
        #[trigger] super::model::waiver_pair_clear(
            waivers[left], waivers[right], candidate, evaluated_at)
}

/// Every current nonblocking waiver request is backed by a valid waiver.
pub open spec fn all_requests_ready(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    forall |index: int| 0 <= index < evidence.spec_findings().len() ==>
        super::model::request_clear(
            evidence,
            #[trigger] evidence.spec_findings()[index],
            candidate,
            evaluated_at,
        )
}

/// Complete declarative finding and waiver readiness over the actual supplied collections.
pub open spec fn finding_inputs_ready(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    all_finding_inputs_ready(evidence, candidate, evaluated_at)
        && all_finding_pairs_ready(evidence.spec_findings(), candidate, evaluated_at)
        && all_waiver_inputs_ready(evidence, candidate, evaluated_at)
        && all_waiver_pairs_ready(evidence.spec_waivers(), candidate, evaluated_at)
        && all_requests_ready(evidence, candidate, evaluated_at)
}

proof fn finding_pairs_prefix_matches_inputs(
    findings: Seq<FindingObservation>,
    current: FindingObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
)
    requires end <= findings.len(),
    ensures super::model::finding_pairs_clear_through(
        findings, current, candidate, evaluated_at, end)
        == (forall |index: int| 0 <= index < end ==>
            #[trigger] super::model::finding_pair_clear(
                findings[index], current, candidate, evaluated_at)),
    decreases end,
{
    if end > 0 {
        finding_pairs_prefix_matches_inputs(
            findings, current, candidate, evaluated_at, (end - 1) as nat);
        reveal(super::model::finding_pairs_clear_through);
    } else {
        reveal(super::model::finding_pairs_clear_through);
    }
}

proof fn findings_prefix_matches_inputs(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
)
    requires end <= evidence.spec_findings().len(),
    ensures super::model::findings_clear_through(evidence, candidate, evaluated_at, end)
        == ((forall |index: int| 0 <= index < end ==>
                super::model::finding_base_clear(
                    evidence,
                    #[trigger] evidence.spec_findings()[index],
                    candidate,
                    evaluated_at,
                ))
            && (forall |left: int, right: int| 0 <= left < right < end ==>
                #[trigger] super::model::finding_pair_clear(
                    evidence.spec_findings()[left],
                    evidence.spec_findings()[right],
                    candidate,
                    evaluated_at,
                ))),
    decreases end,
{
    if end > 0 {
        findings_prefix_matches_inputs(evidence, candidate, evaluated_at, (end - 1) as nat);
        finding_pairs_prefix_matches_inputs(
            evidence.spec_findings(),
            evidence.spec_findings()[(end - 1) as int],
            candidate,
            evaluated_at,
            (end - 1) as nat,
        );
        reveal(super::model::findings_clear_through);
    } else {
        reveal(super::model::findings_clear_through);
    }
}

proof fn waiver_pairs_prefix_matches_inputs(
    waivers: Seq<WaiverObservation>,
    current: WaiverObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
)
    requires end <= waivers.len(),
    ensures super::model::waiver_pairs_clear_through(
        waivers, current, candidate, evaluated_at, end)
        == (forall |index: int| 0 <= index < end ==>
            #[trigger] super::model::waiver_pair_clear(
                waivers[index], current, candidate, evaluated_at)),
    decreases end,
{
    if end > 0 {
        waiver_pairs_prefix_matches_inputs(
            waivers, current, candidate, evaluated_at, (end - 1) as nat);
        reveal(super::model::waiver_pairs_clear_through);
    } else {
        reveal(super::model::waiver_pairs_clear_through);
    }
}

proof fn waivers_prefix_matches_inputs(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
)
    requires end <= evidence.spec_waivers().len(),
    ensures super::model::waivers_clear_through(evidence, candidate, evaluated_at, end)
        == ((forall |index: int| 0 <= index < end ==>
                super::model::waiver_base_clear(
                    evidence,
                    #[trigger] evidence.spec_waivers()[index],
                    candidate,
                    evaluated_at,
                ))
            && (forall |left: int, right: int| 0 <= left < right < end ==>
                #[trigger] super::model::waiver_pair_clear(
                    evidence.spec_waivers()[left],
                    evidence.spec_waivers()[right],
                    candidate,
                    evaluated_at,
                ))),
    decreases end,
{
    if end > 0 {
        waivers_prefix_matches_inputs(evidence, candidate, evaluated_at, (end - 1) as nat);
        waiver_pairs_prefix_matches_inputs(
            evidence.spec_waivers(),
            evidence.spec_waivers()[(end - 1) as int],
            candidate,
            evaluated_at,
            (end - 1) as nat,
        );
        reveal(super::model::waivers_clear_through);
    } else {
        reveal(super::model::waivers_clear_through);
    }
}

proof fn requests_prefix_matches_inputs(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
)
    requires end <= evidence.spec_findings().len(),
    ensures super::model::requests_clear_through(evidence, candidate, evaluated_at, end)
        == (forall |index: int| 0 <= index < end ==>
            super::model::request_clear(
                evidence,
                #[trigger] evidence.spec_findings()[index],
                candidate,
                evaluated_at,
            )),
    decreases end,
{
    if end > 0 {
        requests_prefix_matches_inputs(evidence, candidate, evaluated_at, (end - 1) as nat);
        reveal(super::model::requests_clear_through);
    } else {
        reveal(super::model::requests_clear_through);
    }
}

/// The production reduction accepts exactly the quantified supplied finding and waiver inputs.
pub proof fn reduction_matches_inputs(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
)
    ensures super::model::findings_satisfied(evidence, candidate, evaluated_at)
        == finding_inputs_ready(evidence, candidate, evaluated_at),
{
    findings_prefix_matches_inputs(
        evidence, candidate, evaluated_at, evidence.spec_findings().len() as nat);
    waivers_prefix_matches_inputs(
        evidence, candidate, evaluated_at, evidence.spec_waivers().len() as nat);
    requests_prefix_matches_inputs(
        evidence, candidate, evaluated_at, evidence.spec_findings().len() as nat);
    reveal(super::model::findings_satisfied);
    reveal(finding_inputs_ready);
    reveal(all_finding_inputs_ready);
    reveal(all_finding_pairs_ready);
    reveal(all_waiver_inputs_ready);
    reveal(all_waiver_pairs_ready);
    reveal(all_requests_ready);
}

} // verus!
