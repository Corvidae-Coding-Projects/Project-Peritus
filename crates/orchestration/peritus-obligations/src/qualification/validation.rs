//! Verified condition and evidence admission in production error order.

use crate::{
    ConditionObservation, ObligationError, ObligationErrorKind, RequirementEvidence,
    RequirementLedger,
};
use core::cmp::Ordering;

pub(super) fn validate_inputs(
    ledger: &RequirementLedger,
    conditions: &[ConditionObservation],
    evidence: &[RequirementEvidence],
) -> Result<(), ObligationError> {
    validate_conditions(conditions)?;
    validate_evidence(ledger, evidence)?;
    Ok(())
}
fn validate_conditions(conditions: &[ConditionObservation]) -> Result<(), ObligationError> {
    if conditions.len() < 2 {
        return Ok(());
    }
    let mut index = 1;
    while index < conditions.len() {
        let order = crate::order::compare(
            conditions[index - 1].condition_id().digest().as_bytes(),
            conditions[index].condition_id().digest().as_bytes(),
        );
        match order {
            Ordering::Equal => {
                return Err(ObligationError::plain(ObligationErrorKind::InvalidCondition));
            }
            Ordering::Greater => {
                return Err(ObligationError::plain(ObligationErrorKind::NonCanonicalOrder));
            }
            Ordering::Less => {}
        }
        index += 1;
    }
    Ok(())
}
fn validate_evidence(
    ledger: &RequirementLedger,
    evidence: &[RequirementEvidence],
) -> Result<(), ObligationError> {
    if evidence.len() > ledger.limits().max_evidence() {
        return Err(ObligationError::numbers(
            ObligationErrorKind::LimitExceeded,
            ledger.limits().max_evidence() as u64,
            evidence.len() as u64,
        ));
    }
    if evidence.len() < 2 {
        return validate_known_requirements(ledger, evidence);
    }
    let mut index = 1;
    while index < evidence.len() {
        let order = crate::order::compare(
            evidence[index - 1].requirement_id().digest().as_bytes(),
            evidence[index].requirement_id().digest().as_bytes(),
        );
        match order {
            Ordering::Equal => {
                return Err(ObligationError::plain(ObligationErrorKind::DuplicateValue));
            }
            Ordering::Greater => {
                return Err(ObligationError::plain(ObligationErrorKind::NonCanonicalOrder));
            }
            Ordering::Less => {}
        }
        index += 1;
    }
    validate_known_requirements(ledger, evidence)
}

fn validate_known_requirements(
    ledger: &RequirementLedger,
    evidence: &[RequirementEvidence],
) -> Result<(), ObligationError> {
    let mut index = 0;
    while index < evidence.len() {
        let id = evidence[index].requirement_id();
        match ledger.entry(id) {
            None => {
                return Err(ObligationError::requirement(
                    ObligationErrorKind::UnknownRequirement,
                    id,
                ));
            }
            Some(_entry) => {}
        }
        index += 1;
    }
    Ok(())
}
