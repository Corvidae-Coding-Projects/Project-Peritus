//! Acceptance-criterion map assembly from admitted evidence references.

use crate::{AcceptanceCriterion, CriterionEvidenceMap, CriterionMapping};

use super::{OperatorError, admission::EvidenceStore, plan::CriterionSpec};

pub(super) fn assemble(
    specs: &[CriterionSpec],
    evidence: &EvidenceStore,
) -> Result<CriterionEvidenceMap, OperatorError> {
    let mut mappings = Vec::with_capacity(specs.len());
    for spec in specs {
        let criterion = parse_criterion(&spec.criterion)?;
        let references = spec
            .evidence
            .iter()
            .map(|selector| {
                evidence.record(selector).map(|record| record.evidence_reference().clone())
            })
            .collect::<Result<Vec<_>, _>>()?;
        mappings.push(CriterionMapping::new(criterion, references)?);
    }
    CriterionEvidenceMap::new(mappings).map_err(OperatorError::from)
}

fn parse_criterion(value: &str) -> Result<AcceptanceCriterion, OperatorError> {
    let number = value
        .strip_prefix("AC-")
        .filter(|digits| digits.len() == 2)
        .and_then(|digits| digits.parse::<u8>().ok())
        .ok_or_else(|| OperatorError::integrity("criterion must use AC-01 through AC-25"))?;
    AcceptanceCriterion::all()
        .into_iter()
        .find(|criterion| criterion.number() == number)
        .ok_or_else(|| OperatorError::integrity("criterion must use AC-01 through AC-25"))
}
