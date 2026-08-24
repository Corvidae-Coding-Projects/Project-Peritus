//! Bounded fail-closed decoding of version-one canonical model requests.

mod content;
mod options;
mod primitive;

use peritus_codec::CanonicalReader;

use self::primitive::{codec, invalid, read_collection_len, reader_limits, unknown_tag};
use crate::{
    Message, ModelLimits, ModelRequest, NegotiatedCapabilities, ParallelToolPolicy, ProtocolError,
    ProtocolErrorKind, ProtocolLimits, ProtocolVersion, ProviderProfile, RequestId, ToolChoice,
    ToolDefinition, ToolName, WireDialect,
};

const CANONICAL_MAGIC: [u8; 4] = *b"P5MR";
const MAX_CANONICAL_REQUEST_BYTES: usize = 512 * 1024 * 1024;

/// Decodes exact canonical v1 request bytes against their immutable profile revision.
///
/// The canonical form deliberately excludes caller request identity and profile lifecycle facts,
/// so callers must supply the exact profile and the request ID for the reconstructed request.
/// Decoding is fail-closed: unsupported versions, profile drift, unknown tags, invalid nested
/// values, noncanonical encodings, trailing bytes, and exceeded bounds are rejected.
///
/// # Errors
///
/// Returns a protocol error when bytes are malformed, noncanonical, incompatible with `profile`,
/// outside `limits`, or fail complete [`ModelRequest`] validation.
pub fn decode_request(
    bytes: &[u8],
    profile: &ProviderProfile,
    request_id: RequestId,
    limits: ProtocolLimits,
) -> Result<ModelRequest, ProtocolError> {
    if bytes.len() > MAX_CANONICAL_REQUEST_BYTES {
        return Err(invalid(
            "canonical_request",
            "canonical request exceeds its maximum byte bound",
        ));
    }
    let mut reader = CanonicalReader::new(bytes, reader_limits(limits));
    decode_magic_and_profile(&mut reader, profile)?;
    let negotiated = decode_negotiated(&mut reader, profile)?;
    let messages = decode_messages(&mut reader, limits)?;
    let tools = decode_tools(&mut reader, limits)?;
    let tool_choice = decode_tool_choice(&mut reader)?;
    let parallel = decode_parallel_policy(&mut reader)?;
    let options = options::request_options(&mut reader, limits)?;
    reader.finish().map_err(codec)?;

    let request = ModelRequest::new(
        profile,
        negotiated,
        request_id,
        messages,
        tools,
        tool_choice,
        parallel,
        options,
        limits,
    )?;
    if request.canonical_bytes()?.as_slice() != bytes {
        return Err(invalid(
            "canonical_request",
            "decoded request bytes are not the canonical v1 representation",
        ));
    }
    Ok(request)
}

fn decode_magic_and_profile(
    reader: &mut CanonicalReader<'_>,
    profile: &ProviderProfile,
) -> Result<(), ProtocolError> {
    if reader.read_fixed::<4>().map_err(codec)? != CANONICAL_MAGIC {
        return Err(invalid("canonical_request.magic", "canonical request magic is invalid"));
    }
    let version =
        ProtocolVersion::new(reader.read_u16().map_err(codec)?, reader.read_u16().map_err(codec)?)?;
    if version != profile.protocol() {
        return Err(ProtocolError::at(
            ProtocolErrorKind::UnsupportedVersion,
            "canonical_request.version",
            "canonical request version does not match the supplied profile",
        ));
    }
    let profile_id = reader.read_fixed::<16>().map_err(codec)?;
    let revision = reader.read_u64().map_err(codec)?;
    let provider = reader.read_str().map_err(codec)?;
    let dialect = decode_dialect(reader.read_u8().map_err(codec)?)?;
    let model = reader.read_str().map_err(codec)?;
    if profile_id != *profile.profile_id().as_bytes()
        || revision != profile.revision()
        || provider != profile.provider().as_str()
        || dialect != profile.dialect()
        || model != profile.model().as_str()
    {
        return Err(invalid(
            "canonical_request.profile",
            "canonical request profile identity or revision does not match",
        ));
    }
    Ok(())
}

fn decode_negotiated(
    reader: &mut CanonicalReader<'_>,
    profile: &ProviderProfile,
) -> Result<NegotiatedCapabilities, ProtocolError> {
    let selected = reader.read_u64().map_err(codec)?;
    let model_limits = ModelLimits::new(
        reader.read_u64().map_err(codec)?,
        reader.read_u64().map_err(codec)?,
        reader.read_u32().map_err(codec)?,
        reader.read_u32().map_err(codec)?,
        reader.read_u64().map_err(codec)?,
    )?;
    NegotiatedCapabilities::from_canonical(profile, selected, model_limits)
}

fn decode_messages(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<Vec<Message>, ProtocolError> {
    let count = read_collection_len(reader, limits.max_messages(), "messages")?;
    let mut messages = Vec::with_capacity(count);
    for _ in 0..count {
        messages.push(content::message(reader, limits)?);
    }
    Ok(messages)
}

fn decode_tools(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<Vec<ToolDefinition>, ProtocolError> {
    let count = read_collection_len(reader, limits.max_tools(), "tools")?;
    let mut tools = Vec::with_capacity(count);
    for _ in 0..count {
        let name = ToolName::new(reader.read_str().map_err(codec)?.to_owned())?;
        let description = primitive::optional_text(reader, limits)?;
        let parameters = primitive::schema(reader, limits)?;
        let strict = reader.read_bool().map_err(codec)?;
        tools.push(ToolDefinition::new(name, description, parameters, strict));
    }
    Ok(tools)
}

fn decode_tool_choice(reader: &mut CanonicalReader<'_>) -> Result<ToolChoice, ProtocolError> {
    match reader.read_u8().map_err(codec)? {
        1 => Ok(ToolChoice::Auto),
        2 => Ok(ToolChoice::None),
        3 => Ok(ToolChoice::Required),
        4 => ToolName::new(reader.read_str().map_err(codec)?.to_owned()).map(ToolChoice::Specific),
        _ => Err(unknown_tag("tool_choice")),
    }
}

fn decode_parallel_policy(
    reader: &mut CanonicalReader<'_>,
) -> Result<ParallelToolPolicy, ProtocolError> {
    match reader.read_u8().map_err(codec)? {
        1 => Ok(ParallelToolPolicy::Disabled),
        2 => Ok(ParallelToolPolicy::Allowed(reader.read_u32().map_err(codec)?)),
        _ => Err(unknown_tag("parallel_tool_policy")),
    }
}

fn decode_dialect(tag: u8) -> Result<WireDialect, ProtocolError> {
    match tag {
        1 => Ok(WireDialect::OpenAiResponses),
        2 => Ok(WireDialect::AnthropicMessages),
        3 => Ok(WireDialect::GeminiInteractionsV1),
        4 => Ok(WireDialect::GeminiGenerateContentV1),
        5 => Ok(WireDialect::CompatibleResponses),
        6 => Ok(WireDialect::CompatibleChatCompletions),
        7 => Ok(WireDialect::OpenAiCodexRuntime),
        8 => Ok(WireDialect::AnthropicClaudeRuntime),
        _ => Err(unknown_tag("wire_dialect")),
    }
}
