//! Canonical materialization receipts for qualification harness bindings.

use peritus_codec::{CanonicalWriter, CodecLimits};
use peritus_harness::{MaterializationReceipt, domain::HarnessRevision};
use peritus_journal::StoreId;
use peritus_types::{Generation, WorkspaceId};

use crate::EvolutionError;

use super::super::identity::{digest, invalid, nominal};

pub(super) fn receipt(
    revision: &HarnessRevision,
    workspace: WorkspaceId,
    installed_revision: u64,
    seed: u8,
    store: StoreId,
) -> Result<MaterializationReceipt, EvolutionError> {
    let mut fields = CanonicalWriter::new(CodecLimits::PRODUCTION);
    fields
        .write_fixed(&nominal(b"peritus/h1/promotion/plan/v1\0", store))
        .map_err(|_| invalid("encode qualification plan identity"))?;
    fields
        .write_fixed(digest(b"peritus/h1/promotion/plan-digest/v1\0", store).as_bytes())
        .map_err(|_| invalid("encode qualification plan digest"))?;
    fields
        .write_fixed(revision.harness_id().as_bytes())
        .map_err(|_| invalid("encode qualification harness identity"))?;
    fields
        .write_fixed(revision.digest().as_bytes())
        .map_err(|_| invalid("encode qualification revision digest"))?;
    fields.write_option_tag(false).map_err(|_| invalid("encode qualification prior receipt"))?;
    fields
        .write_fixed(digest(b"peritus/h1/promotion/patch/v1\0", store).as_bytes())
        .map_err(|_| invalid("encode qualification patch identity"))?;
    fields.write_fixed(&[seed; 16]).map_err(|_| invalid("encode qualification patch action"))?;
    fields
        .write_fixed(digest(b"peritus/h1/promotion/patch-auth/v1\0", store).as_bytes())
        .map_err(|_| invalid("encode qualification patch authorization"))?;
    fields
        .write_fixed(&[seed.wrapping_add(1); 16])
        .map_err(|_| invalid("encode qualification candidate action"))?;
    fields
        .write_fixed(digest(b"peritus/h1/promotion/candidate-auth/v1\0", store).as_bytes())
        .map_err(|_| invalid("encode qualification candidate authorization"))?;
    write_snapshot(
        &mut fields,
        workspace,
        installed_revision
            .checked_sub(1)
            .ok_or_else(|| invalid("qualification receipt has no predecessor"))?,
        seed.wrapping_add(2),
    )?;
    write_snapshot(&mut fields, workspace, installed_revision, seed.wrapping_add(4))?;
    fields
        .write_fixed(&[seed.wrapping_add(6); 16])
        .map_err(|_| invalid("encode qualification snapshot identity"))?;
    fields
        .write_fixed(digest(b"peritus/h1/promotion/workspace-manifest/v1\0", store).as_bytes())
        .map_err(|_| invalid("encode qualification workspace manifest"))?;
    fields.write_collection_len(0).map_err(|_| invalid("encode qualification inventory"))?;
    fields.write_u64(100).map_err(|_| invalid("encode qualification start time"))?;
    fields.write_u64(101).map_err(|_| invalid("encode qualification completion time"))?;
    fields
        .write_fixed(&nominal(b"peritus/h1/promotion/receipt-event/v1\0", store))
        .map_err(|_| invalid("encode qualification receipt event"))?;

    let domain = b"peritus.harness.materialization-receipt.v1\0";
    let mut preimage = CanonicalWriter::new(CodecLimits::PRODUCTION);
    preimage
        .write_fixed(domain)
        .and_then(|()| preimage.write_fixed(fields.as_slice()))
        .map_err(|_| invalid("encode qualification receipt preimage"))?;
    let receipt_digest = peritus_codec::sha256(preimage.as_slice());
    let mut receipt_id = [0_u8; 16];
    receipt_id.copy_from_slice(&receipt_digest.as_bytes()[..16]);
    receipt_id[0] |= 0x40;
    let mut encoded = CanonicalWriter::new(CodecLimits::PRODUCTION);
    encoded
        .write_fixed(domain)
        .and_then(|()| encoded.write_fixed(&receipt_id))
        .and_then(|()| encoded.write_fixed(receipt_digest.as_bytes()))
        .and_then(|()| encoded.write_fixed(fields.as_slice()))
        .map_err(|_| invalid("encode qualification receipt"))?;
    MaterializationReceipt::decode_canonical(&encoded.into_bytes())
        .map_err(|_| invalid("decode qualification materialization receipt"))
}

fn write_snapshot(
    writer: &mut CanonicalWriter,
    workspace: WorkspaceId,
    revision: u64,
    seed: u8,
) -> Result<(), EvolutionError> {
    writer
        .write_fixed(workspace.as_bytes())
        .and_then(|()| writer.write_u64(Generation::first().get()))
        .and_then(|()| writer.write_u64(revision))
        .and_then(|()| writer.write_u8(2))
        .and_then(|()| writer.write_bytes(&[seed; 32]))
        .and_then(|()| writer.write_u8(2))
        .and_then(|()| writer.write_bytes(&[seed.wrapping_add(1); 32]))
        .map_err(|_| invalid("encode qualification workspace snapshot"))
}
