//! Fixed portable bundle framing helpers.

use crate::{EvidenceError, EvidenceErrorKind, EvidenceRecord, RecoveryAction};
use std::collections::{BTreeMap, BTreeSet};

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

pub(super) fn validate_ancestry(
    records: &[EvidenceRecord],
    authority_records: &[crate::EvidenceId],
) -> Result<(), EvidenceError> {
    let indexed: BTreeMap<_, _> = records.iter().map(|record| (record.id(), record)).collect();
    for record in records {
        for cause in record.causes() {
            let parent =
                indexed.get(cause).ok_or_else(|| invalid("bundle omits a direct causal parent"))?;
            if !crate::verified::causal_position(
                parent.provenance().global_position(),
                record.provenance().global_position(),
            ) {
                return Err(invalid("bundle causal ancestry is not strictly ordered"));
            }
        }
    }
    let mut reachable = BTreeSet::new();
    let mut pending: BTreeSet<_> = authority_records.iter().copied().collect();
    while let Some(id) = pending.pop_first() {
        let record =
            indexed.get(&id).ok_or_else(|| invalid("bundle authority record is absent"))?;
        if !reachable.insert(id) {
            continue;
        }
        pending.extend(record.causes().iter().copied());
    }
    if reachable.len() != records.len() {
        return Err(invalid("bundle contains records outside the authority causal closure"));
    }
    Ok(())
}
