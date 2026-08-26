//! Canonical materialization receipt encoding and checked reconstruction.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};
use peritus_patch::WorkspacePath;
use peritus_types::{ActionId, EventId, HarnessId, Sha256Digest, SnapshotId};

use crate::domain::RevisionDigest;

use super::{
    MaterializationError, MaterializationErrorKind, MaterializationPlanId, MaterializationReceipt,
    MaterializationReceiptId, MaterializationRecovery, ReceiptFile, WorkspaceSnapshot,
};

const RECEIPT_DOMAIN: &[u8] = b"peritus.harness.materialization-receipt.v1\0";

impl ReceiptFile {
    fn encode(&self, writer: &mut CanonicalWriter) -> Result<(), MaterializationError> {
        writer.write_str(self.path.as_str()).map_err(codec)?;
        writer.write_fixed(self.digest.as_bytes()).map_err(codec)?;
        writer.write_u64(self.byte_length).map_err(codec)?;
        writer.write_u8(super::plan_codec::mode_tag(self.mode)).map_err(codec)
    }

    fn decode(reader: &mut CanonicalReader<'_>) -> Result<Self, MaterializationError> {
        let path = WorkspacePath::new(reader.read_str().map_err(codec)?.to_owned())
            .map_err(|_| invalid("receipt contains an invalid target path"))?;
        Ok(Self {
            path,
            digest: Sha256Digest::new(reader.read_fixed().map_err(codec)?),
            byte_length: reader.read_u64().map_err(codec)?,
            mode: super::plan_codec::decode_mode(reader.read_u8().map_err(codec)?)?,
        })
    }
}

impl MaterializationReceipt {
    /// Returns complete canonical receipt bytes.
    ///
    /// # Errors
    /// Returns a codec error when configured E1 bounds cannot represent the receipt.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, MaterializationError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_fixed(RECEIPT_DOMAIN).map_err(codec)?;
        writer.write_fixed(self.id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.digest.as_bytes()).map_err(codec)?;
        self.encode_fields(&mut writer)?;
        Ok(writer.into_bytes())
    }

    /// Decodes and rechecks complete canonical receipt bytes.
    ///
    /// # Errors
    /// Rejects malformed, noncanonical, trailing, or digest-mismatched input.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, MaterializationError> {
        let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
        if reader.read_fixed::<43>().map_err(codec)?.as_slice() != RECEIPT_DOMAIN {
            return Err(invalid("materialization receipt domain separator differs"));
        }
        let id = MaterializationReceiptId::decode(reader.read_fixed().map_err(codec)?)?;
        let digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
        let receipt = Self::decode_fields(id, digest, &mut reader)?;
        reader.finish().map_err(codec)?;
        if peritus_codec::sha256(&receipt.encode_without_identity()?) != digest
            || MaterializationReceiptId::from_digest(digest) != id
        {
            return Err(MaterializationError::new(
                MaterializationErrorKind::Conflict,
                MaterializationRecovery::Quarantine,
                "materialization receipt identity or digest does not bind its fields",
            ));
        }
        Ok(receipt)
    }

    pub(super) fn encode_without_identity(&self) -> Result<Vec<u8>, MaterializationError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_fixed(RECEIPT_DOMAIN).map_err(codec)?;
        self.encode_fields(&mut writer)?;
        Ok(writer.into_bytes())
    }

    fn encode_fields(&self, writer: &mut CanonicalWriter) -> Result<(), MaterializationError> {
        writer.write_fixed(self.plan_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.plan_digest.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.harness_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.revision_digest.as_bytes()).map_err(codec)?;
        writer.write_option_tag(self.prior_receipt.is_some()).map_err(codec)?;
        if let Some(value) = self.prior_receipt {
            writer.write_fixed(value.as_bytes()).map_err(codec)?;
        }
        writer.write_fixed(self.patch_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.patch_action_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.patch_authorization_digest.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.candidate_action_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.candidate_authorization_digest.as_bytes()).map_err(codec)?;
        self.before.encode(writer).map_err(codec)?;
        self.after.encode(writer).map_err(codec)?;
        writer.write_fixed(self.snapshot_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.workspace_manifest_artifact.as_bytes()).map_err(codec)?;
        writer.write_collection_len(self.files.len()).map_err(codec)?;
        for file in &self.files {
            file.encode(writer)?;
        }
        writer.write_u64(self.started_at_millis).map_err(codec)?;
        writer.write_u64(self.completed_at_millis).map_err(codec)?;
        writer.write_fixed(self.causal_event_id.as_bytes()).map_err(codec)
    }

    fn decode_fields(
        id: MaterializationReceiptId,
        digest: Sha256Digest,
        reader: &mut CanonicalReader<'_>,
    ) -> Result<Self, MaterializationError> {
        let plan_id = MaterializationPlanId::decode(reader.read_fixed().map_err(codec)?)?;
        let plan_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
        let harness_id = HarnessId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| invalid("receipt harness identity is zero"))?;
        let revision_digest =
            RevisionDigest::new(Sha256Digest::new(reader.read_fixed().map_err(codec)?));
        let prior_receipt = reader
            .read_option_tag()
            .map_err(codec)?
            .then(|| MaterializationReceiptId::decode(reader.read_fixed().map_err(codec)?))
            .transpose()?;
        let patch_id = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
        let patch_action_id = ActionId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| invalid("receipt patch action identity is zero"))?;
        let patch_authorization_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
        let candidate_action_id = ActionId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| invalid("receipt candidate action identity is zero"))?;
        let candidate_authorization_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
        let before = WorkspaceSnapshot::decode(reader)?;
        let after = WorkspaceSnapshot::decode(reader)?;
        if before.workspace_id() != after.workspace_id()
            || before.generation() != after.generation()
            || after.revision().get() != before.revision().get().saturating_add(1)
        {
            return Err(invalid("receipt workspace successor is not exact"));
        }
        let snapshot_id = SnapshotId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| invalid("receipt snapshot identity is zero"))?;
        let workspace_manifest_artifact = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
        let count = reader.read_collection_len().map_err(codec)?;
        let mut files = Vec::with_capacity(count);
        for _ in 0..count {
            let file = ReceiptFile::decode(reader)?;
            if files.last().is_some_and(|prior: &ReceiptFile| prior.path >= file.path) {
                return Err(invalid("receipt files are not in strict target order"));
            }
            files.push(file);
        }
        let started_at_millis = reader.read_u64().map_err(codec)?;
        let completed_at_millis = reader.read_u64().map_err(codec)?;
        if completed_at_millis < started_at_millis {
            return Err(invalid("receipt completion precedes start"));
        }
        let causal_event_id = EventId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| invalid("receipt causal event identity is zero"))?;
        Ok(Self {
            id,
            digest,
            plan_id,
            plan_digest,
            harness_id,
            revision_digest,
            prior_receipt,
            patch_id,
            patch_action_id,
            patch_authorization_digest,
            candidate_action_id,
            candidate_authorization_digest,
            before,
            after,
            snapshot_id,
            workspace_manifest_artifact,
            files,
            started_at_millis,
            completed_at_millis,
            causal_event_id,
        })
    }
}

fn codec(error: peritus_codec::CodecError) -> MaterializationError {
    MaterializationError::new(
        MaterializationErrorKind::Codec,
        MaterializationRecovery::Quarantine,
        error.to_string(),
    )
}

fn invalid(detail: &'static str) -> MaterializationError {
    MaterializationError::new(
        MaterializationErrorKind::Receipt,
        MaterializationRecovery::Quarantine,
        detail,
    )
}
