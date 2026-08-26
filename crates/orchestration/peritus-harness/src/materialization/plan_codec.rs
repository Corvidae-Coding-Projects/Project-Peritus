//! Canonical materialization plan encoding and decoding.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};
use peritus_patch::{FileMode, Preimage, WorkspacePath};
use peritus_types::{CommandId, EventId, HarnessId, Sha256Digest};

use crate::domain::RevisionDigest;

use super::{
    MaterializationError, MaterializationErrorKind, MaterializationPlan, MaterializationPlanId,
    MaterializationReason, MaterializationReceiptId, MaterializationRecovery, PlannedFileOperation,
    WorkspaceSnapshot,
};

const PLAN_DOMAIN: &[u8] = b"peritus.harness.materialization-plan.v1\0";

impl MaterializationPlan {
    /// Returns exact canonical plan bytes.
    ///
    /// # Errors
    /// Returns a codec failure if the configured E1 frame bounds cannot represent the plan.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, MaterializationError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_fixed(PLAN_DOMAIN).map_err(codec)?;
        writer.write_fixed(self.id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.digest.as_bytes()).map_err(codec)?;
        self.encode_fields(&mut writer)?;
        Ok(writer.into_bytes())
    }

    /// Decodes and revalidates exact canonical plan bytes.
    ///
    /// # Errors
    /// Rejects malformed, noncanonical, trailing, or digest-mismatched bytes.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, MaterializationError> {
        let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
        if reader.read_fixed::<40>().map_err(codec)?.as_slice() != PLAN_DOMAIN {
            return Err(invalid("materialization plan domain separator differs"));
        }
        let id = MaterializationPlanId::decode(reader.read_fixed().map_err(codec)?)?;
        let digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
        let plan = Self::decode_fields(id, digest, &mut reader)?;
        reader.finish().map_err(codec)?;
        if peritus_codec::sha256(&plan.canonical_identity_bytes()?) != digest
            || MaterializationPlanId::from_digest(digest) != id
        {
            return Err(MaterializationError::new(
                MaterializationErrorKind::Conflict,
                MaterializationRecovery::Quarantine,
                "materialization plan identity or digest does not bind its fields",
            ));
        }
        Ok(plan)
    }

    pub(super) fn canonical_identity_bytes(&self) -> Result<Vec<u8>, MaterializationError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_fixed(PLAN_DOMAIN).map_err(codec)?;
        self.encode_fields(&mut writer)?;
        Ok(writer.into_bytes())
    }

    fn encode_fields(&self, writer: &mut CanonicalWriter) -> Result<(), MaterializationError> {
        writer.write_fixed(self.command_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.causal_event_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.harness_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.revision_digest.as_bytes()).map_err(codec)?;
        writer.write_u64(self.revision_number).map_err(codec)?;
        writer.write_fixed(self.graph_digest.as_bytes()).map_err(codec)?;
        self.target.encode(writer).map_err(codec)?;
        self.reason.encode(writer).map_err(codec)?;
        writer.write_option_tag(self.prior_receipt.is_some()).map_err(codec)?;
        if let Some(receipt) = self.prior_receipt {
            writer.write_fixed(receipt.as_bytes()).map_err(codec)?;
        }
        writer.write_collection_len(self.operations.len()).map_err(codec)?;
        for operation in &self.operations {
            encode_operation(writer, operation)?;
        }
        writer.write_u64(self.total_bytes).map_err(codec)
    }

    fn decode_fields(
        id: MaterializationPlanId,
        digest: Sha256Digest,
        reader: &mut CanonicalReader<'_>,
    ) -> Result<Self, MaterializationError> {
        let command_id = CommandId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| invalid("plan command identity is zero"))?;
        let causal_event_id = EventId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| invalid("plan event identity is zero"))?;
        let harness_id = HarnessId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| invalid("plan harness identity is zero"))?;
        let revision_digest =
            RevisionDigest::new(Sha256Digest::new(reader.read_fixed().map_err(codec)?));
        let revision_number = reader.read_u64().map_err(codec)?;
        if revision_number == 0 {
            return Err(invalid("plan revision number is zero"));
        }
        let graph_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
        let target = WorkspaceSnapshot::decode(reader)?;
        let reason = MaterializationReason::decode(reader)?;
        let prior_receipt = reader
            .read_option_tag()
            .map_err(codec)?
            .then(|| MaterializationReceiptId::decode(reader.read_fixed().map_err(codec)?))
            .transpose()?;
        let count = reader.read_collection_len().map_err(codec)?;
        if count == 0 {
            return Err(invalid("decoded plan has no operations"));
        }
        let mut operations = Vec::with_capacity(count);
        let mut previous: Option<WorkspacePath> = None;
        let mut computed_total = 0_u64;
        for _ in 0..count {
            let operation = decode_operation(reader)?;
            if previous.as_ref().is_some_and(|path| path >= operation.path()) {
                return Err(invalid("decoded plan operations are not in strict path order"));
            }
            if let PlannedFileOperation::Install { byte_length, .. } = operation {
                computed_total = computed_total
                    .checked_add(byte_length)
                    .ok_or_else(|| invalid("decoded plan byte total overflowed"))?;
            }
            previous = Some(operation.path().clone());
            operations.push(operation);
        }
        let total_bytes = reader.read_u64().map_err(codec)?;
        if computed_total != total_bytes {
            return Err(invalid("decoded plan byte total differs"));
        }
        Ok(Self {
            id,
            digest,
            command_id,
            causal_event_id,
            harness_id,
            revision_digest,
            revision_number,
            graph_digest,
            target,
            reason,
            prior_receipt,
            operations,
            total_bytes,
        })
    }
}

fn encode_operation(
    writer: &mut CanonicalWriter,
    operation: &PlannedFileOperation,
) -> Result<(), MaterializationError> {
    match operation {
        PlannedFileOperation::Install { path, preimage, artifact_digest, byte_length, mode } => {
            writer.write_u8(1).map_err(codec)?;
            writer.write_str(path.as_str()).map_err(codec)?;
            encode_preimage(writer, *preimage)?;
            writer.write_fixed(artifact_digest.as_bytes()).map_err(codec)?;
            writer.write_u64(*byte_length).map_err(codec)?;
            writer.write_u8(mode_tag(*mode)).map_err(codec)
        }
        PlannedFileOperation::Delete { path, preimage } => {
            writer.write_u8(2).map_err(codec)?;
            writer.write_str(path.as_str()).map_err(codec)?;
            encode_preimage(writer, *preimage)
        }
    }
}

fn decode_operation(
    reader: &mut CanonicalReader<'_>,
) -> Result<PlannedFileOperation, MaterializationError> {
    let tag = reader.read_u8().map_err(codec)?;
    let path = WorkspacePath::new(reader.read_str().map_err(codec)?.to_owned())
        .map_err(|_| invalid("decoded plan path is invalid"))?;
    let preimage = decode_preimage(reader)?;
    match tag {
        1 => Ok(PlannedFileOperation::Install {
            path,
            preimage,
            artifact_digest: Sha256Digest::new(reader.read_fixed().map_err(codec)?),
            byte_length: reader.read_u64().map_err(codec)?,
            mode: decode_mode(reader.read_u8().map_err(codec)?)?,
        }),
        2 if matches!(preimage, Preimage::Present { .. }) => {
            Ok(PlannedFileOperation::Delete { path, preimage })
        }
        _ => Err(invalid("unknown or invalid planned file operation")),
    }
}

fn encode_preimage(
    writer: &mut CanonicalWriter,
    preimage: Preimage,
) -> Result<(), MaterializationError> {
    match preimage {
        Preimage::Absent => writer.write_u8(0).map_err(codec),
        Preimage::Present { digest, size, mode } => {
            writer.write_u8(1).map_err(codec)?;
            writer.write_fixed(digest.as_bytes()).map_err(codec)?;
            writer.write_u64(size).map_err(codec)?;
            writer.write_u8(mode_tag(mode)).map_err(codec)
        }
    }
}

fn decode_preimage(reader: &mut CanonicalReader<'_>) -> Result<Preimage, MaterializationError> {
    match reader.read_u8().map_err(codec)? {
        0 => Ok(Preimage::Absent),
        1 => Ok(Preimage::present(
            Sha256Digest::new(reader.read_fixed().map_err(codec)?),
            reader.read_u64().map_err(codec)?,
            decode_mode(reader.read_u8().map_err(codec)?)?,
        )),
        _ => Err(invalid("unknown preimage tag")),
    }
}

pub(super) const fn mode_tag(mode: FileMode) -> u8 {
    match mode {
        FileMode::Regular => 1,
        FileMode::Executable => 2,
    }
}

pub(super) fn decode_mode(tag: u8) -> Result<FileMode, MaterializationError> {
    match tag {
        1 => Ok(FileMode::Regular),
        2 => Ok(FileMode::Executable),
        _ => Err(invalid("unknown file mode")),
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
        MaterializationErrorKind::InvalidPlan,
        MaterializationRecovery::Quarantine,
        detail,
    )
}
