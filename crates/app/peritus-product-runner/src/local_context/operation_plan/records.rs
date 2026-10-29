//! Exact host records are evidence, never native assistant calls or literal user inputs.

use super::{PREFIX, PlanRecord, RecordEnvelope, error};
use crate::local_context::{memory::LocalMemory, record::ArchiveKind};
use peritus_agent::DeveloperLoopError;
use peritus_model_protocol::{
    BoundedText, ContentBlock, Message, ProtocolLimits, Role, decode_messages,
};

pub(in crate::local_context) fn encode_message(
    record: &RecordEnvelope,
) -> Result<Message, DeveloperLoopError> {
    let text = format!(
        "{PREFIX}{}",
        serde_json::to_string(record).map_err(|_| error("encode plan record"))?
    );
    Message::new(
        Role::User,
        vec![ContentBlock::Text(BoundedText::new(text, ProtocolLimits::PRODUCTION)?)],
        ProtocolLimits::PRODUCTION,
    )
    .map_err(Into::into)
}

pub(in crate::local_context) fn decode_message(
    bytes: &[u8],
) -> Result<Option<RecordEnvelope>, DeveloperLoopError> {
    let messages = decode_messages(bytes, ProtocolLimits::PRODUCTION)?;
    let mut record = None;
    for message in &messages {
        for block in message.content() {
            if let ContentBlock::Text(text) = block
                && let Some(body) = text.expose_for_wire().strip_prefix(PREFIX)
            {
                if record.is_some()
                    || messages.len() != 1
                    || message.role() != Role::User
                    || message.content().len() != 1
                {
                    return Err(error("invalid plan record framing"));
                }
                let decoded: RecordEnvelope =
                    serde_json::from_str(body).map_err(|_| error("invalid plan record"))?;
                if decoded.version != 1
                    || serde_json::to_string(&decoded).map_err(|_| error("encode plan record"))?
                        != body
                {
                    return Err(error("unsupported or noncanonical plan record"));
                }
                record = Some(decoded);
            }
        }
    }
    Ok(record)
}

pub(in crate::local_context) fn load(
    memory: &LocalMemory,
    sequence: u64,
) -> Result<PlanRecord, DeveloperLoopError> {
    maybe_load(memory, sequence)?.ok_or_else(|| error("plan record unavailable"))
}

pub(super) fn maybe_load(
    memory: &LocalMemory,
    sequence: u64,
) -> Result<Option<PlanRecord>, DeveloperLoopError> {
    if memory.archived(sequence)?.kind != ArchiveKind::HostObservation {
        return Ok(None);
    }
    let Some(record) = decode_message(&memory.artifact(sequence)?)? else {
        return Ok(None);
    };
    if memory.checkpoint_schema < crate::local_context::record::PLAN_CHECKPOINT_SCHEMA_VERSION
        || record.scope != crate::local_context::tools::hex(memory.store.scope_digest().as_bytes())
    {
        return Err(error("plan record scope or reader guard mismatch"));
    }
    Ok(Some(record.record))
}

pub(super) fn archive(
    memory: &mut LocalMemory,
    record: PlanRecord,
    dependencies: &[peritus_types::Sha256Digest],
) -> Result<u64, DeveloperLoopError> {
    let envelope = RecordEnvelope {
        version: 1,
        scope: crate::local_context::tools::hex(memory.store.scope_digest().as_bytes()),
        record,
    };
    memory.observe_plan_record(&encode_message(&envelope)?, dependencies)
}
