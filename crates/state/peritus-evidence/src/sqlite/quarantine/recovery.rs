//! Exact-identity dependency reconciliation and reconstruction of quarantined records.

use super::{
    EvidenceQuarantineId, RawEvidence, corrupt, fixed_digest, mark_reconciled,
    quarantine_row_by_id, record_row_by_raw,
};
use crate::sqlite::{
    EvidenceStore,
    row::{integer, load_record, load_record_uncontained},
};
use crate::{
    EvidenceCancellation, EvidenceError, EvidenceErrorKind, EvidenceRecord, RecoveryAction,
};
use peritus_artifact_store::{ArtifactStore, ReferenceOwner};
use peritus_types::EvidenceId;
use rusqlite::{Transaction, TransactionBehavior, params};

impl EvidenceStore {
    /// Revalidates an unchanged active row after its external dependency was repaired.
    ///
    /// The permanent quarantine identity is the authorization fence. Reconciliation is recorded
    /// separately while every copied corrupt byte remains immutable.
    ///
    /// # Errors
    ///
    /// Rejects a mismatched identity, changed active row, unresolved corruption, or tampered audit
    /// metadata.
    pub fn reconcile_quarantined(
        &mut self,
        id: EvidenceId,
        quarantine_id: EvidenceQuarantineId,
        artifacts: &ArtifactStore,
        cancellation: &EvidenceCancellation,
    ) -> Result<EvidenceRecord, EvidenceError> {
        cancellation.check("reconcile quarantined evidence")?;
        let observed = {
            let transaction =
                Transaction::new_unchecked(&self.connection, TransactionBehavior::Deferred)
                    .map_err(|e| EvidenceError::sqlite("read reconciliation candidate", e))?;
            let raw = quarantine_row_by_id(&transaction, quarantine_id)?
                .ok_or_else(|| reconciliation_error("quarantine identity does not exist"))?;
            if raw.audit()?.evidence_id != Some(id) {
                return Err(reconciliation_error("quarantine identity does not own evidence"));
            }
            let record = load_record_uncontained(&transaction, id)?
                .ok_or_else(|| reconciliation_error("active record is absent"))?;
            transaction
                .commit()
                .map_err(|e| EvidenceError::sqlite("finish reconciliation candidate", e))?;
            record
        };
        verify_artifacts(&observed, artifacts, cancellation)?;

        cancellation.check("reconcile quarantined evidence")?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| EvidenceError::sqlite("begin evidence reconciliation", error))?;
        let raw = quarantine_row_by_id(&transaction, quarantine_id)?
            .ok_or_else(|| reconciliation_error("quarantine identity does not exist"))?;
        let audit = raw.audit()?;
        if audit.evidence_id != Some(id) {
            return Err(reconciliation_error(
                "quarantine identity does not own the selected evidence identity",
            ));
        }
        if let Some(reconciled) = audit.reconciled_record_digest {
            let record = load_record(&transaction, id)?
                .ok_or_else(|| corrupt("reconciled evidence row is missing"))?;
            if record.record_digest() != reconciled {
                return Err(corrupt(
                    "reconciled evidence row disagrees with its resolution digest",
                ));
            }
            validate_dependencies(&transaction, &record)?;
            transaction.commit().map_err(|error| {
                EvidenceError::sqlite("finish idempotent evidence reconciliation", error)
            })?;
            return Ok(record);
        }
        let active = record_row_by_raw(&transaction, id.as_bytes(), String::new())?
            .ok_or_else(|| reconciliation_error("active evidence row does not exist"))?;
        if !raw.same_active_record(&active) {
            return Err(reconciliation_error(
                "active evidence row changed after its quarantine copy was made",
            ));
        }
        let record = load_record_uncontained(&transaction, id)?
            .ok_or_else(|| reconciliation_error("active evidence row does not exist"))?;
        if record.record_digest() != observed.record_digest() {
            return Err(reconciliation_error("reconciliation candidate changed"));
        }
        validate_dependencies(&transaction, &record)?;
        mark_reconciled(&transaction, quarantine_id, record.record_digest())?;
        cancellation.check("reconcile quarantined evidence")?;
        transaction
            .commit()
            .map_err(|error| EvidenceError::sqlite("commit evidence reconciliation", error))?;
        Ok(record)
    }

    /// Rebuilds one quarantined active row from canonical bytes under its original identity.
    ///
    /// The replacement must match the originally indexed record digest and provenance. Causes,
    /// artifact links, and evidence-owned artifact roots are reconstructed from those canonical
    /// bytes in the same transaction, then fully revalidated before reconciliation is recorded.
    ///
    /// # Errors
    ///
    /// Rejects a mismatched quarantine identity, replacement identity, digest, provenance,
    /// dependency, artifact root, or canonical encoding.
    pub fn rebuild_quarantined(
        &mut self,
        id: EvidenceId,
        quarantine_id: EvidenceQuarantineId,
        repaired_record_bytes: &[u8],
        artifacts: &ArtifactStore,
        cancellation: &EvidenceCancellation,
    ) -> Result<EvidenceRecord, EvidenceError> {
        let replacement = EvidenceRecord::verify_portable(repaired_record_bytes).map_err(|_| {
            reconciliation_error("replacement evidence bytes are not a canonical record")
        })?;
        if replacement.canonical_bytes() != repaired_record_bytes || replacement.id() != id {
            return Err(reconciliation_error(
                "replacement bytes do not preserve the selected evidence identity",
            ));
        }

        verify_artifacts(&replacement, artifacts, cancellation)?;
        cancellation.check("rebuild quarantined evidence")?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| EvidenceError::sqlite("begin evidence rebuild", error))?;
        let raw = quarantine_row_by_id(&transaction, quarantine_id)?
            .ok_or_else(|| reconciliation_error("quarantine identity does not exist"))?;
        let audit = raw.audit()?;
        if audit.evidence_id != Some(id) {
            return Err(reconciliation_error(
                "quarantine identity does not own the selected evidence identity",
            ));
        }
        validate_replacement_binding(&raw, &replacement)?;
        if let Some(reconciled) = audit.reconciled_record_digest {
            let record = load_record(&transaction, id)?
                .ok_or_else(|| corrupt("reconciled evidence row is missing"))?;
            if record.record_digest() != reconciled
                || record.canonical_bytes() != repaired_record_bytes
            {
                return Err(reconciliation_error(
                    "completed rebuild does not match this exact replacement",
                ));
            }
            validate_dependencies(&transaction, &record)?;
            transaction.commit().map_err(|error| {
                EvidenceError::sqlite("finish idempotent evidence rebuild", error)
            })?;
            return Ok(record);
        }
        let active = record_row_by_raw(&transaction, id.as_bytes(), String::new())?
            .ok_or_else(|| reconciliation_error("active evidence row does not exist"))?;
        if !raw.same_active_record(&active) {
            return Err(reconciliation_error(
                "active evidence row changed after its quarantine copy was made",
            ));
        }
        rebuild_record(&transaction, &replacement, repaired_record_bytes, Some(cancellation))?;
        let record = load_record_uncontained(&transaction, id)?
            .ok_or_else(|| corrupt("rebuilt evidence row disappeared"))?;
        validate_dependencies(&transaction, &record)?;
        mark_reconciled(&transaction, quarantine_id, record.record_digest())?;
        cancellation.check("rebuild quarantined evidence")?;
        transaction
            .commit()
            .map_err(|error| EvidenceError::sqlite("commit evidence rebuild", error))?;
        Ok(record)
    }
}
fn validate_replacement_binding(
    raw: &RawEvidence,
    replacement: &EvidenceRecord,
) -> Result<(), EvidenceError> {
    let indexed_digest = fixed_digest(&raw.record_digest, "indexed record digest")?;
    let provenance = replacement.provenance();
    if replacement.record_digest() != indexed_digest
        || u64::try_from(raw.global_position).ok() != Some(provenance.global_position())
        || raw.event_id.as_slice() != provenance.event_id().as_bytes()
        || raw.batch_hash.as_slice() != provenance.batch_hash().as_bytes()
        || raw.revision_digest.as_slice() != provenance.revision_digest().as_bytes()
    {
        return Err(reconciliation_error(
            "replacement does not preserve the original digest and indexed provenance",
        ));
    }
    Ok(())
}

fn rebuild_record(
    transaction: &Transaction<'_>,
    record: &EvidenceRecord,
    record_bytes: &[u8],
    cancellation: Option<&EvidenceCancellation>,
) -> Result<(), EvidenceError> {
    transaction
        .execute(
            "DELETE FROM peritus_evidence_causes WHERE child_id = ?1",
            [record.id().as_bytes().as_slice()],
        )
        .map_err(|error| EvidenceError::sqlite("clear quarantined evidence causes", error))?;
    transaction
        .execute(
            "DELETE FROM peritus_evidence_artifacts WHERE evidence_id = ?1",
            [record.id().as_bytes().as_slice()],
        )
        .map_err(|error| EvidenceError::sqlite("clear quarantined evidence artifacts", error))?;
    transaction
        .execute(
            "DELETE FROM artifact_references WHERE owner_kind = 2 AND owner_identity = ?1",
            [record.record_digest().as_bytes().as_slice()],
        )
        .map_err(|error| EvidenceError::sqlite("clear quarantined evidence roots", error))?;
    let updated = transaction
        .execute(
            "UPDATE peritus_evidence_records SET record_bytes = ?1 WHERE evidence_id = ?2",
            params![record_bytes, record.id().as_bytes().as_slice()],
        )
        .map_err(|error| EvidenceError::sqlite("replace quarantined evidence bytes", error))?;
    if updated != 1 {
        return Err(reconciliation_error("active evidence row does not exist"));
    }
    for (ordinal, parent) in record.causes().iter().enumerate() {
        ensure_live(cancellation, "rebuild quarantined evidence causes")?;
        let parent_record = load_record(transaction, *parent)?
            .ok_or_else(|| dependency_error("causal parent does not exist"))?;
        if !crate::verified::causal_position(
            parent_record.provenance().global_position(),
            record.provenance().global_position(),
        ) {
            return Err(dependency_error("causal parent is not older than rebuilt evidence"));
        }
        let ordinal = u64::try_from(ordinal)
            .map_err(|_| reconciliation_error("cause ordinal exceeds u64"))?;
        transaction
            .execute(
                "INSERT INTO peritus_evidence_causes(child_id, parent_id, ordinal)
                 VALUES (?1, ?2, ?3)",
                params![
                    record.id().as_bytes().as_slice(),
                    parent.as_bytes().as_slice(),
                    integer(ordinal, "rebuild cause ordinal")?,
                ],
            )
            .map_err(|error| EvidenceError::sqlite("rebuild evidence cause", error))?;
    }
    for (ordinal, artifact) in record.artifacts().iter().enumerate() {
        ensure_live(cancellation, "rebuild quarantined evidence artifacts")?;
        if !peritus_artifact_store::sqlite_interop::is_referenceable(transaction, *artifact)
            .map_err(|error| EvidenceError::sqlite("check rebuilt evidence artifact", error))?
        {
            return Err(artifact_error("rebuilt evidence artifact is not referenceable"));
        }
        let ordinal = u64::try_from(ordinal)
            .map_err(|_| reconciliation_error("artifact ordinal exceeds u64"))?;
        transaction
            .execute(
                "INSERT INTO peritus_evidence_artifacts(evidence_id, artifact_digest, ordinal)
                 VALUES (?1, ?2, ?3)",
                params![
                    record.id().as_bytes().as_slice(),
                    artifact.as_bytes().as_slice(),
                    integer(ordinal, "rebuild artifact ordinal")?,
                ],
            )
            .map_err(|error| EvidenceError::sqlite("rebuild evidence artifact", error))?;
        peritus_artifact_store::sqlite_interop::insert_reference(
            transaction,
            ReferenceOwner::evidence(record.record_digest()),
            *artifact,
        )
        .map_err(|error| EvidenceError::sqlite("rebuild evidence artifact roots", error))?;
    }
    Ok(())
}

fn ensure_live(
    cancellation: Option<&EvidenceCancellation>,
    operation: &'static str,
) -> Result<(), EvidenceError> {
    if cancellation.is_some_and(EvidenceCancellation::is_cancelled) {
        Err(EvidenceError::cancelled(operation))
    } else {
        Ok(())
    }
}

fn reconciliation_error(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::InvalidInput,
        RecoveryAction::CorrectInput,
        "reconcile quarantined evidence",
        detail,
    )
}

fn dependency_error(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::InvalidCause,
        RecoveryAction::RepairDependency,
        "rebuild quarantined evidence",
        detail,
    )
}

fn artifact_error(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::MissingArtifact,
        RecoveryAction::RepairDependency,
        "rebuild quarantined evidence",
        detail,
    )
}

fn verify_artifacts(
    record: &EvidenceRecord,
    store: &ArtifactStore,
    cancellation: &EvidenceCancellation,
) -> Result<(), EvidenceError> {
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    for digest in record.artifacts() {
        cancellation.check("verify reconciliation dependency")?;
        let mut input = crate::artifact::ArtifactInput::open(store, *digest)?;
        loop {
            cancellation.check("hash reconciliation dependency")?;
            if input.read(&mut buffer)? == 0 {
                break;
            }
        }
    }
    Ok(())
}

fn validate_dependencies(
    transaction: &Transaction<'_>,
    record: &EvidenceRecord,
) -> Result<(), EvidenceError> {
    let provenance = record.provenance();
    let durable =
        crate::sqlite::row::journal_observation(transaction, provenance.global_position())?;
    let frame = peritus_codec::decode_frame(&durable.frame, peritus_codec::CodecLimits::PRODUCTION)
        .map_err(|_| dependency_error("repaired journal frame is not canonical"))?;
    if (
        durable.event_id,
        durable.event_hash,
        durable.batch_hash,
        durable.revision_digest,
        durable.frame_digest,
        durable.frame_family,
        durable.frame_schema,
    ) != (
        provenance.event_id(),
        provenance.event_hash(),
        provenance.batch_hash(),
        provenance.revision_digest(),
        provenance.frame_digest(),
        provenance.frame_family(),
        provenance.frame_schema_version(),
    ) || peritus_codec::sha256(&durable.frame) != provenance.frame_digest()
        || frame.header().family() != provenance.frame_family()
        || frame.header().schema_version() != provenance.frame_schema_version()
        || crate::provenance::schema_digest(durable.frame_family, durable.frame_schema)?
            != provenance.schema_digest()
        || crate::revision_digest(record.revision()) != provenance.revision_digest()
        || durable.artifacts != record.artifacts()
    {
        return Err(dependency_error(
            "repaired dependencies disagree with immutable journal provenance",
        ));
    }
    for id in record.causes() {
        let parent = load_record(transaction, *id)?
            .ok_or_else(|| dependency_error("repaired causal parent is absent"))?;
        if !crate::verified::causal_position(
            parent.provenance().global_position(),
            provenance.global_position(),
        ) {
            return Err(dependency_error("repaired causal parent is not older than evidence"));
        }
    }
    for digest in record.artifacts() {
        if !peritus_artifact_store::sqlite_interop::is_referenceable(transaction, *digest)
            .map_err(|e| EvidenceError::sqlite("check reconciled artifact availability", e))?
        {
            return Err(artifact_error("repaired artifact is no longer referenceable"));
        }
    }
    Ok(())
}
