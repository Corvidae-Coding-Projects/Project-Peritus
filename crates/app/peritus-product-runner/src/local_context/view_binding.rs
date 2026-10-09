//! Canonical binding for one persisted provider view and the policy/accounting that selected it.

use super::{
    error,
    record::{
        CHECKPOINT_SCHEMA_VERSION, CheckpointManifest, LEGACY_CHECKPOINT_SCHEMA_VERSION,
        ViewValidation, encode,
    },
};
use peritus_agent::DeveloperLoopError;
use peritus_codec::{CanonicalWriter, CodecLimits, sha256};
use peritus_model_protocol::{SchemaDialect, ToolDefinition};

const TOOL_POLICY_MAGIC: &[u8; 4] = b"P4TP";
const VIEW_BINDING_MAGIC: &[u8; 4] = b"P4VB";
const MAX_BINDING_BYTES: usize = 128 * 1024 * 1024;

const fn writer() -> CanonicalWriter {
    CanonicalWriter::new(CodecLimits::new(
        MAX_BINDING_BYTES,
        MAX_BINDING_BYTES,
        1_100_000,
        32 * 1024 * 1024,
        32 * 1024 * 1024,
        16,
    ))
}

fn binding_error(_: peritus_codec::CodecError) -> DeveloperLoopError {
    error("encode durable provider-view binding")
}

/// Hashes tool definitions in their provider-protocol order and canonical field encoding.
pub(super) fn tool_policy(tools: &[ToolDefinition]) -> Result<[u8; 32], DeveloperLoopError> {
    let mut encoded = writer();
    encoded.write_fixed(TOOL_POLICY_MAGIC).map_err(binding_error)?;
    encoded.write_collection_len(tools.len()).map_err(binding_error)?;
    for tool in tools {
        encoded.write_str(tool.name().as_str()).map_err(binding_error)?;
        encoded.write_option_tag(tool.description().is_some()).map_err(binding_error)?;
        if let Some(description) = tool.description() {
            encoded.write_str(description.expose_for_wire()).map_err(binding_error)?;
        }
        encoded
            .write_u8(match tool.parameters().dialect() {
                SchemaDialect::Draft202012 => 1,
                SchemaDialect::Draft7 => 2,
                SchemaDialect::GeminiSubset => 3,
                SchemaDialect::ProfiledSubset => 4,
            })
            .map_err(binding_error)?;
        encoded.write_bytes(tool.parameters().canonical_bytes()).map_err(binding_error)?;
        encoded.write_bool(tool.strict()).map_err(binding_error)?;
    }
    Ok(sha256(&encoded.into_bytes()).into_bytes())
}

/// Binds exact provider-view bytes to their canonical selection, policy, and accounting record.
pub(super) fn checkpoint(
    scope: [u8; 32],
    generation: u64,
    through_event: u64,
    render_policy: [u8; 32],
    view: &[u8],
    validation: &ViewValidation,
) -> Result<[u8; 32], DeveloperLoopError> {
    let validation = encode(validation)?;
    let mut encoded = writer();
    encoded.write_fixed(VIEW_BINDING_MAGIC).map_err(binding_error)?;
    encoded.write_u16(CHECKPOINT_SCHEMA_VERSION).map_err(binding_error)?;
    encoded.write_fixed(&scope).map_err(binding_error)?;
    encoded.write_u64(generation).map_err(binding_error)?;
    encoded.write_u64(through_event).map_err(binding_error)?;
    encoded.write_fixed(&render_policy).map_err(binding_error)?;
    encoded.write_bytes(view).map_err(binding_error)?;
    encoded.write_bytes(&validation).map_err(binding_error)?;
    Ok(sha256(&encoded.into_bytes()).into_bytes())
}

/// Verifies a versioned checkpoint's exact provider-view binding.
pub(super) fn verify(
    manifest: &CheckpointManifest,
    view: &[u8],
    validation: &ViewValidation,
) -> Result<(), DeveloperLoopError> {
    match manifest.schema_version {
        LEGACY_CHECKPOINT_SCHEMA_VERSION
            if manifest.view_binding.is_none() && validation.tool_policy.is_none() =>
        {
            Ok(())
        }
        CHECKPOINT_SCHEMA_VERSION if validation.tool_policy.is_some() => {
            let expected = checkpoint(
                manifest.scope,
                manifest.generation,
                manifest.through_event,
                manifest.render_policy,
                view,
                validation,
            )?;
            if manifest.view_binding == Some(expected) {
                Ok(())
            } else {
                Err(error("checkpoint provider-view binding mismatch"))
            }
        }
        _ => Err(error("checkpoint provider-view schema mismatch")),
    }
}
