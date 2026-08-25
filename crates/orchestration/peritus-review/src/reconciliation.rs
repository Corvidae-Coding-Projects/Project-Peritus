//! Provenance-preserving duplicate and confirmed-supersession reconciliation.

use peritus_types::{EventId, FindingId, RevisionTuple, Sha256Digest};

use crate::binding::canonical_nonempty;
use crate::error::{ReviewError, ReviewErrorKind, reject};
use crate::state::mutation;
use crate::{DispositionKind, DispositionRecord, ReviewRunState};

/// Reconciles nonempty canonical duplicate identities under one existing current finding.
///
/// # Errors
/// Rejects self/cyclic/conflicting supersession, stale/category-mismatched findings, noncanonical
/// identities, non-open histories, or provenance bounds that cannot be retained completely.
#[allow(
    clippy::too_many_lines,
    reason = "the atomic reconciliation validation and mutation sequence is intentionally contiguous"
)]
pub fn reconcile_duplicates(
    state: &mut ReviewRunState,
    event_id: EventId,
    canonical: FindingId,
    duplicates: &[FindingId],
    reconciliation_digest: Sha256Digest,
) -> Result<(), ReviewError> {
    canonical_nonempty(duplicates, "duplicate finding identities are not canonical")?;
    if duplicates.binary_search(&canonical).is_ok() {
        return Err(reject(
            ReviewErrorKind::IdentityConflict,
            "a finding cannot be reconciled with itself",
        ));
    }
    let target = state.finding(canonical).ok_or_else(|| {
        reject(ReviewErrorKind::UnknownIdentity, "canonical finding does not exist")
    })?;
    if !state.finding_is_current(target)
        || target.superseded_by().is_some()
        || target.current_disposition() != DispositionKind::Open
    {
        return Err(reject(
            ReviewErrorKind::IllegalTransition,
            "canonical finding is stale, superseded, or no longer open",
        ));
    }
    let category = target.category();
    let revision = target.revision();
    let snapshots = duplicates
        .iter()
        .map(|identity| {
            let finding = state.finding(*identity).ok_or_else(|| {
                reject(ReviewErrorKind::UnknownIdentity, "duplicate finding does not exist")
            })?;
            if !state.finding_is_current(finding)
                || finding.category() != category
                || finding.revision() != revision
                || finding.superseded_by().is_some()
                || finding.current_disposition() != DispositionKind::Open
            {
                return Err(reject(
                    ReviewErrorKind::IllegalTransition,
                    "duplicate finding is stale, conflicting, or no longer open",
                ));
            }
            Ok(finding.clone())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let source_count = snapshots.iter().try_fold(target.sources().len(), |total, finding| {
        total.checked_add(finding.sources().len()).ok_or_else(|| {
            reject(ReviewErrorKind::LimitExceeded, "reconciled provenance count overflowed")
        })
    })?;
    let evidence_count = snapshots.iter().try_fold(target.evidence().len(), |total, finding| {
        total.checked_add(finding.evidence().len()).ok_or_else(|| {
            reject(ReviewErrorKind::LimitExceeded, "reconciled evidence count overflowed")
        })
    })?;
    let disposition_count = snapshots.iter().try_fold(
        target.dispositions().len().saturating_add(1),
        |total, finding| {
            total.checked_add(finding.dispositions().len()).ok_or_else(|| {
                reject(ReviewErrorKind::LimitExceeded, "reconciled history count overflowed")
            })
        },
    )?;
    if source_count > usize::from(state.limits().provenance_sources())
        || evidence_count > usize::from(state.limits().evidence_references())
        || disposition_count > usize::from(state.limits().disposition_records())
    {
        return Err(reject(
            ReviewErrorKind::LimitExceeded,
            "reconciliation cannot retain complete provenance under current limits",
        ));
    }

    for snapshot in &snapshots {
        let target = mutation::finding_mut(state, canonical).ok_or_else(|| {
            reject(ReviewErrorKind::UnknownIdentity, "canonical finding disappeared")
        })?;
        mutation::merge_sources_and_evidence(target, snapshot);
    }
    let target = mutation::finding_mut(state, canonical)
        .ok_or_else(|| reject(ReviewErrorKind::UnknownIdentity, "canonical finding disappeared"))?;
    mutation::push_disposition(
        target,
        reconciliation_record(
            event_id,
            DispositionKind::Open,
            revision,
            None,
            reconciliation_digest,
        ),
    );
    for duplicate in duplicates {
        let finding = mutation::finding_mut(state, *duplicate).ok_or_else(|| {
            reject(ReviewErrorKind::UnknownIdentity, "duplicate finding disappeared")
        })?;
        mutation::set_superseded_by(finding, canonical);
        mutation::push_disposition(
            finding,
            reconciliation_record(
                event_id,
                DispositionKind::Superseded,
                revision,
                Some(canonical),
                reconciliation_digest,
            ),
        );
    }
    Ok(())
}

/// Absorbs one confirmed superseded finding into an existing current replacement.
pub fn confirm_supersession(
    state: &mut ReviewRunState,
    event_id: EventId,
    finding_id: FindingId,
    superseding: FindingId,
    reviewer_cycle: peritus_types::ReviewCycleId,
    evidence: Vec<peritus_evidence::EvidenceId>,
    digest: Sha256Digest,
) -> Result<(), ReviewError> {
    if finding_id == superseding {
        return Err(reject(ReviewErrorKind::IdentityConflict, "a finding cannot supersede itself"));
    }
    let source = state.finding(finding_id).ok_or_else(|| {
        reject(ReviewErrorKind::UnknownIdentity, "superseded finding does not exist")
    })?;
    let target = state.finding(superseding).ok_or_else(|| {
        reject(ReviewErrorKind::UnknownIdentity, "superseding finding does not exist")
    })?;
    if !state.finding_is_current(source)
        || !state.finding_is_current(target)
        || source.revision() != target.revision()
        || source.category() != target.category()
        || source.superseded_by().is_some()
        || target.superseded_by().is_some()
        || target.current_disposition() != DispositionKind::Open
    {
        return Err(reject(
            ReviewErrorKind::IllegalTransition,
            "confirmed supersession is stale, conflicting, or category-mismatched",
        ));
    }
    if target.sources().len().saturating_add(source.sources().len())
        > usize::from(state.limits().provenance_sources())
        || target.evidence().len().saturating_add(source.evidence().len())
            > usize::from(state.limits().evidence_references())
        || target.dispositions().len().saturating_add(source.dispositions().len()).saturating_add(1)
            > usize::from(state.limits().disposition_records())
    {
        return Err(reject(
            ReviewErrorKind::LimitExceeded,
            "confirmed supersession cannot retain complete provenance under current limits",
        ));
    }
    let snapshot = source.clone();
    let revision = source.revision();
    let target = mutation::finding_mut(state, superseding).ok_or_else(|| {
        reject(ReviewErrorKind::UnknownIdentity, "superseding finding disappeared")
    })?;
    mutation::merge_sources_and_evidence(target, &snapshot);
    mutation::push_disposition(
        target,
        reconciliation_record(event_id, DispositionKind::Open, revision, Some(finding_id), digest),
    );
    let source = mutation::finding_mut(state, finding_id).ok_or_else(|| {
        reject(ReviewErrorKind::UnknownIdentity, "superseded finding disappeared")
    })?;
    mutation::set_superseded_by(source, superseding);
    mutation::push_disposition(
        source,
        DispositionRecord::from_wire(
            event_id,
            DispositionKind::Superseded,
            None,
            Some(reviewer_cycle),
            revision,
            evidence,
            Some(superseding),
            None,
            None,
            None,
            digest,
        ),
    );
    Ok(())
}

const fn reconciliation_record(
    event_id: EventId,
    kind: DispositionKind,
    revision: RevisionTuple,
    related: Option<FindingId>,
    digest: Sha256Digest,
) -> DispositionRecord {
    DispositionRecord::from_wire(
        event_id,
        kind,
        None,
        None,
        revision,
        Vec::new(),
        related,
        None,
        None,
        None,
        digest,
    )
}
