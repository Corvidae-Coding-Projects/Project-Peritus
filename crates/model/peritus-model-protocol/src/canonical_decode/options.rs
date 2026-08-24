//! Checked request-option and policy decoding.

use peritus_codec::CanonicalReader;

use super::content::extension;
use super::primitive::{
    bounded_text, codec, optional_i64, optional_u32, optional_u64, read_collection_len, schema,
    unknown_tag,
};
use crate::{
    CacheKey, CachePolicy, Continuation, EventId, GenerationConfig, OutputName, PersistencePolicy,
    ProtocolError, ProtocolLimits, ReasoningEffort, ReasoningPolicy, RequestOptions, ResponseId,
    StructuredOutput, SummaryPolicy,
};

pub(super) fn request_options(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<RequestOptions, ProtocolError> {
    let output = output_policy(reader, limits)?;
    let reasoning = reasoning_policy(reader)?;
    let generation = generation(reader, limits)?;
    let cache = cache_policy(reader)?;
    let persistence = PersistencePolicy::new(
        reader.read_bool().map_err(codec)?,
        reader.read_bool().map_err(codec)?,
    )?;
    let continuation = continuation(reader)?;
    let count = read_collection_len(reader, 128, "extensions")?;
    let mut extensions = Vec::with_capacity(count);
    for _ in 0..count {
        extensions.push(extension(reader, limits)?);
    }
    Ok(RequestOptions::new(
        output,
        reasoning,
        generation,
        cache,
        persistence,
        continuation,
        extensions,
    ))
}

fn output_policy(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<StructuredOutput, ProtocolError> {
    match reader.read_u8().map_err(codec)? {
        1 => Ok(StructuredOutput::Text),
        2 => Ok(StructuredOutput::JsonObject),
        3 => Ok(StructuredOutput::JsonSchema {
            name: OutputName::new(reader.read_str().map_err(codec)?.to_owned())?,
            schema: schema(reader, limits)?,
            strict: reader.read_bool().map_err(codec)?,
        }),
        _ => Err(unknown_tag("structured_output")),
    }
}

fn reasoning_policy(reader: &mut CanonicalReader<'_>) -> Result<ReasoningPolicy, ProtocolError> {
    match reader.read_u8().map_err(codec)? {
        1 => Ok(ReasoningPolicy::Disabled),
        2 => Ok(ReasoningPolicy::Adaptive { summary: summary_policy(reader)? }),
        3 => Ok(ReasoningPolicy::Effort {
            effort: reasoning_effort(reader)?,
            summary: summary_policy(reader)?,
        }),
        _ => Err(unknown_tag("reasoning_policy")),
    }
}

fn reasoning_effort(reader: &mut CanonicalReader<'_>) -> Result<ReasoningEffort, ProtocolError> {
    match reader.read_u8().map_err(codec)? {
        1 => Ok(ReasoningEffort::Minimal),
        2 => Ok(ReasoningEffort::Low),
        3 => Ok(ReasoningEffort::Medium),
        4 => Ok(ReasoningEffort::High),
        _ => Err(unknown_tag("reasoning_effort")),
    }
}

fn summary_policy(reader: &mut CanonicalReader<'_>) -> Result<SummaryPolicy, ProtocolError> {
    match reader.read_u8().map_err(codec)? {
        1 => Ok(SummaryPolicy::None),
        2 => Ok(SummaryPolicy::Auto),
        3 => Ok(SummaryPolicy::Concise),
        4 => Ok(SummaryPolicy::Detailed),
        _ => Err(unknown_tag("summary_policy")),
    }
}

fn generation(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<GenerationConfig, ProtocolError> {
    let max_output_tokens = reader.read_u64().map_err(codec)?;
    let count = read_collection_len(reader, 64, "generation.stop_sequences")?;
    let mut stop_sequences = Vec::with_capacity(count);
    for _ in 0..count {
        stop_sequences.push(bounded_text(reader, limits)?);
    }
    GenerationConfig::new(
        max_output_tokens,
        stop_sequences,
        optional_i64(reader)?,
        optional_u32(reader)?,
        optional_u32(reader)?,
    )
}

fn cache_policy(reader: &mut CanonicalReader<'_>) -> Result<CachePolicy, ProtocolError> {
    match reader.read_u8().map_err(codec)? {
        1 => Ok(CachePolicy::Disabled),
        2 => Ok(CachePolicy::Automatic),
        3 => Ok(CachePolicy::Ephemeral { ttl_seconds: reader.read_u32().map_err(codec)? }),
        4 => CacheKey::new(reader.read_str().map_err(codec)?.to_owned()).map(CachePolicy::Explicit),
        _ => Err(unknown_tag("cache_policy")),
    }
}

fn continuation(reader: &mut CanonicalReader<'_>) -> Result<Option<Continuation>, ProtocolError> {
    if !reader.read_option_tag().map_err(codec)? {
        return Ok(None);
    }
    let response_id = ResponseId::new(reader.read_str().map_err(codec)?.to_owned())?;
    let event_id = if reader.read_option_tag().map_err(codec)? {
        Some(EventId::new(reader.read_str().map_err(codec)?.to_owned())?)
    } else {
        None
    };
    Continuation::new(response_id, event_id, optional_u64(reader)?).map(Some)
}
