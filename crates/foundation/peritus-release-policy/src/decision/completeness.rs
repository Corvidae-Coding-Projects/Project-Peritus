//! Executable refinements of final assessment completeness.

use super::{CriterionAssessment, EvidenceAssessment, QualificationAssessment};
use vstd::prelude::*;

verus! {

pub(super) const fn criteria_complete(
    values: &[CriterionAssessment; 25],
) -> (complete: bool)
    ensures complete == (
        values[0].spec_is_satisfied() && values[1].spec_is_satisfied()
            && values[2].spec_is_satisfied() && values[3].spec_is_satisfied()
            && values[4].spec_is_satisfied() && values[5].spec_is_satisfied()
            && values[6].spec_is_satisfied() && values[7].spec_is_satisfied()
            && values[8].spec_is_satisfied() && values[9].spec_is_satisfied()
            && values[10].spec_is_satisfied() && values[11].spec_is_satisfied()
            && values[12].spec_is_satisfied() && values[13].spec_is_satisfied()
            && values[14].spec_is_satisfied() && values[15].spec_is_satisfied()
            && values[16].spec_is_satisfied() && values[17].spec_is_satisfied()
            && values[18].spec_is_satisfied() && values[19].spec_is_satisfied()
            && values[20].spec_is_satisfied() && values[21].spec_is_satisfied()
            && values[22].spec_is_satisfied() && values[23].spec_is_satisfied()
            && values[24].spec_is_satisfied())
{
    values[0].is_satisfied() && values[1].is_satisfied()
        && values[2].is_satisfied() && values[3].is_satisfied()
        && values[4].is_satisfied() && values[5].is_satisfied()
        && values[6].is_satisfied() && values[7].is_satisfied()
        && values[8].is_satisfied() && values[9].is_satisfied()
        && values[10].is_satisfied() && values[11].is_satisfied()
        && values[12].is_satisfied() && values[13].is_satisfied()
        && values[14].is_satisfied() && values[15].is_satisfied()
        && values[16].is_satisfied() && values[17].is_satisfied()
        && values[18].is_satisfied() && values[19].is_satisfied()
        && values[20].is_satisfied() && values[21].is_satisfied()
        && values[22].is_satisfied() && values[23].is_satisfied()
        && values[24].is_satisfied()
}

pub(super) const fn evidence_complete(
    values: &[EvidenceAssessment; 44],
) -> (complete: bool)
    ensures complete == (
        values[0].spec_is_satisfied() && values[1].spec_is_satisfied()
            && values[2].spec_is_satisfied() && values[3].spec_is_satisfied()
            && values[4].spec_is_satisfied() && values[5].spec_is_satisfied()
            && values[6].spec_is_satisfied() && values[7].spec_is_satisfied()
            && values[8].spec_is_satisfied() && values[9].spec_is_satisfied()
            && values[10].spec_is_satisfied() && values[11].spec_is_satisfied()
            && values[12].spec_is_satisfied() && values[13].spec_is_satisfied()
            && values[14].spec_is_satisfied() && values[15].spec_is_satisfied()
            && values[16].spec_is_satisfied() && values[17].spec_is_satisfied()
            && values[18].spec_is_satisfied() && values[19].spec_is_satisfied()
            && values[20].spec_is_satisfied() && values[21].spec_is_satisfied()
            && values[22].spec_is_satisfied() && values[23].spec_is_satisfied()
            && values[24].spec_is_satisfied() && values[25].spec_is_satisfied()
            && values[26].spec_is_satisfied() && values[27].spec_is_satisfied()
            && values[28].spec_is_satisfied() && values[29].spec_is_satisfied()
            && values[30].spec_is_satisfied() && values[31].spec_is_satisfied()
            && values[32].spec_is_satisfied() && values[33].spec_is_satisfied()
            && values[34].spec_is_satisfied() && values[35].spec_is_satisfied()
            && values[36].spec_is_satisfied() && values[37].spec_is_satisfied()
            && values[38].spec_is_satisfied() && values[39].spec_is_satisfied()
            && values[40].spec_is_satisfied() && values[41].spec_is_satisfied()
            && values[42].spec_is_satisfied() && values[43].spec_is_satisfied())
{
    values[0].is_satisfied() && values[1].is_satisfied()
        && values[2].is_satisfied() && values[3].is_satisfied()
        && values[4].is_satisfied() && values[5].is_satisfied()
        && values[6].is_satisfied() && values[7].is_satisfied()
        && values[8].is_satisfied() && values[9].is_satisfied()
        && values[10].is_satisfied() && values[11].is_satisfied()
        && values[12].is_satisfied() && values[13].is_satisfied()
        && values[14].is_satisfied() && values[15].is_satisfied()
        && values[16].is_satisfied() && values[17].is_satisfied()
        && values[18].is_satisfied() && values[19].is_satisfied()
        && values[20].is_satisfied() && values[21].is_satisfied()
        && values[22].is_satisfied() && values[23].is_satisfied()
        && values[24].is_satisfied() && values[25].is_satisfied()
        && values[26].is_satisfied() && values[27].is_satisfied()
        && values[28].is_satisfied() && values[29].is_satisfied()
        && values[30].is_satisfied() && values[31].is_satisfied()
        && values[32].is_satisfied() && values[33].is_satisfied()
        && values[34].is_satisfied() && values[35].is_satisfied()
        && values[36].is_satisfied() && values[37].is_satisfied()
        && values[38].is_satisfied() && values[39].is_satisfied()
        && values[40].is_satisfied() && values[41].is_satisfied()
        && values[42].is_satisfied() && values[43].is_satisfied()
}

pub(super) const fn qualifications_complete(
    values: &[QualificationAssessment; 4],
) -> (complete: bool)
    ensures complete == (
        values[0].spec_is_satisfied() && values[1].spec_is_satisfied()
            && values[2].spec_is_satisfied() && values[3].spec_is_satisfied())
{
    values[0].is_satisfied() && values[1].is_satisfied()
        && values[2].is_satisfied() && values[3].is_satisfied()
}

} // verus!
