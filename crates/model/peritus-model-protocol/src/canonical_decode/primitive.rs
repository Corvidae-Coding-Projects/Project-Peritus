//! Primitive canonical reads and checked nested domain values.

use peritus_codec::{CanonicalReader, CodecLimits};
use peritus_types::Sha256Digest;

use crate::{
    BoundedText, CanonicalJson, JsonBounds, JsonSchema, ProtocolError, ProtocolErrorKind,
    ProtocolLimits, SchemaDialect,
};

const MAX_CANONICAL_REQUEST_BYTES: usize = 512 * 1024 * 1024;

pub(super) const fn reader_limits(limits: ProtocolLimits) -> CodecLimits {
    CodecLimits::new(
        MAX_CANONICAL_REQUEST_BYTES,
        MAX_CANONICAL_REQUEST_BYTES,
        max_usize(
            max_usize(limits.max_messages(), limits.max_content_blocks()),
            max_usize(limits.max_tools(), 128),
        ),
        max_usize(limits.max_text_bytes(), 8 * 1024),
        max_usize(
            max_usize(limits.max_inline_media_bytes(), limits.max_schema_bytes()),
            max_usize(limits.max_tool_argument_bytes(), limits.max_extension_bytes()),
        ),
        128,
    )
}

const fn max_usize(left: usize, right: usize) -> usize {
    if left > right { left } else { right }
}

pub(super) fn read_collection_len(
    reader: &mut CanonicalReader<'_>,
    maximum: usize,
    path: &'static str,
) -> Result<usize, ProtocolError> {
    let count = reader.read_collection_len().map_err(codec)?;
    if count > maximum {
        return Err(invalid(path, "canonical collection count exceeds its request bound"));
    }
    Ok(count)
}

pub(super) fn optional_text(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<Option<BoundedText>, ProtocolError> {
    if reader.read_option_tag().map_err(codec)? {
        bounded_text(reader, limits).map(Some)
    } else {
        Ok(None)
    }
}

pub(super) fn bounded_text(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<BoundedText, ProtocolError> {
    BoundedText::new(reader.read_str().map_err(codec)?.to_owned(), limits)
}

pub(super) fn canonical_json(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<CanonicalJson, ProtocolError> {
    let bytes = reader.read_bytes().map_err(codec)?;
    let text = core::str::from_utf8(bytes)
        .map_err(|_| invalid("canonical_json", "canonical JSON is not valid UTF-8"))?;
    let value = CanonicalJson::parse(text, JsonBounds::value(limits))?;
    if value.canonical_bytes() != bytes {
        return Err(invalid("canonical_json", "JSON bytes are not in canonical form"));
    }
    Ok(value)
}

pub(super) fn schema(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<JsonSchema, ProtocolError> {
    let dialect = schema_dialect(reader.read_u8().map_err(codec)?)?;
    let bytes = reader.read_bytes().map_err(codec)?;
    let text = core::str::from_utf8(bytes)
        .map_err(|_| invalid("json_schema", "canonical JSON Schema is not valid UTF-8"))?;
    let schema = JsonSchema::parse(text, dialect, JsonBounds::schema(limits))?;
    if schema.canonical_bytes() != bytes {
        return Err(invalid("json_schema", "JSON Schema bytes are not in canonical form"));
    }
    Ok(schema)
}

pub(super) fn optional_digest(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<Sha256Digest>, ProtocolError> {
    if reader.read_option_tag().map_err(codec)? {
        Ok(Some(Sha256Digest::new(reader.read_fixed::<32>().map_err(codec)?)))
    } else {
        Ok(None)
    }
}

pub(super) fn optional_u64(reader: &mut CanonicalReader<'_>) -> Result<Option<u64>, ProtocolError> {
    if reader.read_option_tag().map_err(codec)? {
        reader.read_u64().map(Some).map_err(codec)
    } else {
        Ok(None)
    }
}

pub(super) fn optional_i64(reader: &mut CanonicalReader<'_>) -> Result<Option<i64>, ProtocolError> {
    if reader.read_option_tag().map_err(codec)? {
        let bytes = reader.read_fixed::<8>().map_err(codec)?;
        Ok(Some(i64::from_be_bytes(bytes)))
    } else {
        Ok(None)
    }
}

pub(super) fn optional_u32(reader: &mut CanonicalReader<'_>) -> Result<Option<u32>, ProtocolError> {
    if reader.read_option_tag().map_err(codec)? {
        reader.read_u32().map(Some).map_err(codec)
    } else {
        Ok(None)
    }
}

fn schema_dialect(tag: u8) -> Result<SchemaDialect, ProtocolError> {
    match tag {
        1 => Ok(SchemaDialect::Draft202012),
        2 => Ok(SchemaDialect::Draft7),
        3 => Ok(SchemaDialect::GeminiSubset),
        4 => Ok(SchemaDialect::ProfiledSubset),
        _ => Err(unknown_tag("schema_dialect")),
    }
}

pub(super) fn codec(_: peritus_codec::CodecError) -> ProtocolError {
    invalid("canonical_request", "canonical request bytes are malformed or incomplete")
}

pub(super) fn unknown_tag(path: &'static str) -> ProtocolError {
    invalid(path, "canonical request contains an unknown tag")
}

pub(super) fn invalid(path: &'static str, detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::InvalidRequest, path, detail)
}
