//! Closed production-pointer command/event semantic payload codec.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};

use crate::{EvolutionError, PointerCommandKind, PromotionId, RollbackId};

use super::{super::scalar, binding, proposal};

pub(crate) fn encode_kind(kind: &PointerCommandKind) -> Result<Vec<u8>, EvolutionError> {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    match kind {
        PointerCommandKind::InitializeProductionHarness {
            initial,
            policy,
            limits,
            evidence_artifact,
            evidence_digest,
        } => {
            writer.write_u8(1).map_err(scalar::codec)?;
            binding::write_production(&mut writer, *initial)?;
            binding::write_limits(&mut writer, *limits)?;
            binding::write_policy(&mut writer, policy)?;
            writer.write_fixed(evidence_artifact.as_bytes()).map_err(scalar::codec)?;
            writer.write_fixed(evidence_digest.as_bytes()).map_err(scalar::codec)?;
        }
        PointerCommandKind::PreparePromotion(value) => {
            writer.write_u8(2).map_err(scalar::codec)?;
            proposal::write_promotion(&mut writer, value)?;
        }
        PointerCommandKind::ActivatePromotion {
            promotion_id,
            campaign_terminal_digest,
            authorization,
        } => {
            writer.write_u8(3).map_err(scalar::codec)?;
            writer.write_fixed(promotion_id.as_bytes()).map_err(scalar::codec)?;
            writer.write_fixed(campaign_terminal_digest.as_bytes()).map_err(scalar::codec)?;
            binding::write_authorization(&mut writer, *authorization)?;
        }
        PointerCommandKind::PrepareRollback(value) => {
            writer.write_u8(4).map_err(scalar::codec)?;
            proposal::write_rollback(&mut writer, value)?;
        }
        PointerCommandKind::ActivateRollback { rollback_id, authorization } => {
            writer.write_u8(5).map_err(scalar::codec)?;
            writer.write_fixed(rollback_id.as_bytes()).map_err(scalar::codec)?;
            binding::write_authorization(&mut writer, *authorization)?;
        }
        PointerCommandKind::CancelPending { reason_digest } => {
            writer.write_u8(6).map_err(scalar::codec)?;
            writer.write_fixed(reason_digest.as_bytes()).map_err(scalar::codec)?;
        }
    }
    Ok(writer.into_bytes())
}

pub(crate) fn decode_kind(bytes: &[u8]) -> Result<PointerCommandKind, EvolutionError> {
    let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
    let kind = match reader.read_u8().map_err(scalar::codec)? {
        1 => {
            let initial = binding::production(&mut reader)?;
            let limits = binding::limits(&mut reader)?;
            let policy = binding::policy(&mut reader, limits)?;
            PointerCommandKind::InitializeProductionHarness {
                initial,
                policy,
                limits,
                evidence_artifact: scalar::digest(&mut reader)?,
                evidence_digest: scalar::digest(&mut reader)?,
            }
        }
        2 => PointerCommandKind::PreparePromotion(proposal::promotion(&mut reader)?),
        3 => PointerCommandKind::ActivatePromotion {
            promotion_id: PromotionId::new(reader.read_fixed().map_err(scalar::codec)?)?,
            campaign_terminal_digest: scalar::digest(&mut reader)?,
            authorization: binding::authorization(&mut reader)?,
        },
        4 => PointerCommandKind::PrepareRollback(proposal::rollback(&mut reader)?),
        5 => PointerCommandKind::ActivateRollback {
            rollback_id: RollbackId::new(reader.read_fixed().map_err(scalar::codec)?)?,
            authorization: binding::authorization(&mut reader)?,
        },
        6 => PointerCommandKind::CancelPending { reason_digest: scalar::digest(&mut reader)? },
        _ => return Err(scalar::protocol()),
    };
    reader.finish().map_err(scalar::codec)?;
    Ok(kind)
}
