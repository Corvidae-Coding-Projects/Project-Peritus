//! Detached-signature admission for a complete retained evidence set.

use std::{collections::BTreeMap, path::Path};

use peritus_release_artifacts::{
    BoundedId, Ed25519PublicKey, Ed25519Signature, ReleaseBinding, ReleasePath,
};

use crate::{EvidenceKind, EvidenceSignature, SignedEvidenceRecord};

use super::{
    OperatorError, files,
    plan::{EvidenceSelector, EvidenceSpec},
};

type EvidenceKey = (EvidenceKind, String);

pub(super) struct EvidenceStore {
    records: BTreeMap<EvidenceKey, AdmittedEvidence>,
}

struct AdmittedEvidence {
    record: SignedEvidenceRecord,
    payload: Vec<u8>,
}

impl EvidenceStore {
    pub(super) fn verify_all(
        binding: &ReleaseBinding,
        evidence_root: &Path,
        specs: &[EvidenceSpec],
    ) -> Result<Self, OperatorError> {
        let mut records = BTreeMap::new();
        for spec in specs {
            let retained_path = ReleasePath::new(&spec.path)?;
            let payload = files::read_rooted(evidence_root, &retained_path, "evidence payload")?;
            let public_path = ReleasePath::new(&spec.public_key_path)?;
            let signature_path = ReleasePath::new(&spec.signature_path)?;
            let public_key = files::read_rooted_material::<32>(
                evidence_root,
                &public_path,
                "Ed25519 public key",
            )?;
            let signature = files::read_rooted_material::<64>(
                evidence_root,
                &signature_path,
                "Ed25519 signature",
            )?;
            let record = SignedEvidenceRecord::verify(
                binding.clone(),
                spec.kind,
                spec.disposition,
                retained_path,
                &payload,
                EvidenceSignature::new(
                    BoundedId::new(&spec.key_id)?,
                    Ed25519PublicKey::from_bytes(public_key),
                    Ed25519Signature::from_bytes(signature),
                ),
            )?;
            let key = (spec.kind, spec.path.clone());
            if records.insert(key, AdmittedEvidence { record, payload }).is_some() {
                return Err(OperatorError::integrity("evidence plan repeats a kind and path"));
            }
        }
        for kind in EvidenceKind::required_signed_inputs() {
            let count = records.keys().filter(|(candidate, _)| *candidate == kind).count();
            if count != 1 {
                return Err(OperatorError::integrity(format!(
                    "evidence plan must contain exactly one {kind:?} record"
                )));
            }
        }
        if records.len() != EvidenceKind::required_signed_inputs().len() {
            return Err(OperatorError::integrity(
                "evidence plan contains records outside the closed required set",
            ));
        }
        Ok(Self { records })
    }

    pub(super) fn record(
        &self,
        selector: &EvidenceSelector,
    ) -> Result<&SignedEvidenceRecord, OperatorError> {
        self.records
            .get(&(selector.kind, selector.path.clone()))
            .map(|evidence| &evidence.record)
            .ok_or_else(|| OperatorError::integrity("evidence selector is not admitted"))
    }

    pub(super) fn unique_kind(
        &self,
        kind: EvidenceKind,
    ) -> Result<&SignedEvidenceRecord, OperatorError> {
        let mut matches = self
            .records
            .values()
            .filter(|evidence| evidence.record.evidence_reference().kind() == kind);
        let first = matches
            .next()
            .ok_or_else(|| OperatorError::integrity(format!("missing {kind:?} evidence")))?;
        if matches.next().is_some() {
            return Err(OperatorError::integrity(format!("duplicate {kind:?} evidence")));
        }
        Ok(&first.record)
    }

    pub(super) fn records(&self) -> impl Iterator<Item = &SignedEvidenceRecord> {
        self.records.values().map(|evidence| &evidence.record)
    }

    pub(super) fn payload_for_kind(&self, kind: EvidenceKind) -> Result<&[u8], OperatorError> {
        let mut matches = self
            .records
            .values()
            .filter(|evidence| evidence.record.evidence_reference().kind() == kind);
        let first = matches
            .next()
            .ok_or_else(|| OperatorError::integrity(format!("missing {kind:?} evidence")))?;
        if matches.next().is_some() {
            return Err(OperatorError::integrity(format!("duplicate {kind:?} evidence")));
        }
        Ok(&first.payload)
    }
}
