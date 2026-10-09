//! Fixed portable bundle framing helpers.

use crate::{EvidenceError, EvidenceErrorKind, EvidenceRecord, RecoveryAction};
use std::collections::BTreeMap;

pub(super) const MAGIC: &[u8; 8] = b"PEREVB1\0";

pub(super) fn invalid(detail: impl Into<String>) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::InvalidBundle,
        RecoveryAction::CorrectInput,
        "verify portable evidence bundle",
        detail,
    )
}

pub(super) fn overflow(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::ArithmeticOverflow,
        RecoveryAction::CorrectInput,
        "plan portable evidence bundle",
        detail,
    )
}

pub(super) fn validate_ancestry(records: &[EvidenceRecord]) -> Result<(), EvidenceError> {
    let positions: BTreeMap<_, _> =
        records.iter().map(|record| (record.id(), record.provenance().global_position())).collect();
    for record in records {
        for cause in record.causes() {
            let position = positions
                .get(cause)
                .ok_or_else(|| invalid("bundle omits a direct causal parent"))?;
            if !crate::verified::causal_position(*position, record.provenance().global_position()) {
                return Err(invalid("bundle causal ancestry is not strictly ordered"));
            }
        }
    }
    Ok(())
}
