//! Strict schema-validated filesystem argument projection.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use peritus_patch::{FileMode, LineEndingPolicy, Preimage};
use peritus_tool_protocol::{
    ArtifactCompleteness, ArtifactProvenance, ArtifactReference, BoundedJson, BoundedText,
    JsonLimits,
};
use peritus_types::{ActionId, Sha256Digest};

use crate::{
    CreateInput, DiscoverInput, FsToolError, FsToolErrorKind, FsToolOperation, MetadataInput,
    MutationContent, PatchEdit, PatchInput, ReadInput, RecoveryClass, RemoveInput, ReplaceInput,
    SearchInput, SearchMatchField, WriteInput,
};

/// Projects an existing immutable artifact reference into the exact `artifact` input shape used
/// by filesystem mutation descriptors. Hosts can pass the result alongside normal tool arguments.
///
/// # Errors
/// Rejects references whose exact numeric size cannot be represented by the JSON protocol.
pub fn artifact_reference_json(reference: &ArtifactReference) -> Result<BoundedJson, FsToolError> {
    let size = i64::try_from(reference.size()).map_err(|_| {
        FsToolError::new(
            FsToolErrorKind::Protocol,
            FsToolOperation::Catalog,
            RecoveryClass::CorrectInput,
            "artifact size cannot be represented by the filesystem input protocol",
        )
    })?;
    let provenance = reference.provenance();
    let provenance = BoundedJson::object(
        vec![
            (
                "action_id".to_owned(),
                BoundedJson::string(
                    hex_bytes(provenance.action_id().as_bytes()),
                    JsonLimits::PRODUCTION,
                )
                .map_err(|_| schema_error())?,
            ),
            (
                "prepared_digest".to_owned(),
                BoundedJson::string(
                    hex_bytes(provenance.prepared_digest().as_bytes()),
                    JsonLimits::PRODUCTION,
                )
                .map_err(|_| schema_error())?,
            ),
        ],
        JsonLimits::PRODUCTION,
    )
    .map_err(|_| schema_error())?;
    let completeness = match reference.completeness() {
        ArtifactCompleteness::Complete => "complete",
        ArtifactCompleteness::Truncated => "truncated",
        ArtifactCompleteness::Indeterminate => "indeterminate",
    };
    BoundedJson::object(
        vec![
            ("completeness".to_owned(), json_string(completeness)?),
            ("digest".to_owned(), json_string(&hex_bytes(reference.digest().as_bytes()))?),
            ("label".to_owned(), json_string(reference.label().as_str())?),
            ("media_type".to_owned(), json_string(reference.media_type().as_str())?),
            ("provenance".to_owned(), provenance),
            ("size".to_owned(), BoundedJson::integer(size)),
        ],
        JsonLimits::PRODUCTION,
    )
    .map_err(|_| schema_error())
}

fn json_string(value: &str) -> Result<BoundedJson, FsToolError> {
    BoundedJson::string(value.to_owned(), JsonLimits::PRODUCTION).map_err(|_| schema_error())
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

const fn schema_error() -> FsToolError {
    FsToolError::new(
        FsToolErrorKind::Protocol,
        FsToolOperation::Catalog,
        RecoveryClass::CorrectInput,
        "artifact reference exceeds the bounded filesystem protocol",
    )
}

pub fn discover(value: &BoundedJson) -> Result<DiscoverInput, FsToolError> {
    Ok(DiscoverInput::page(
        optional_str(value, "root")?,
        number(value, "maximum_depth")?,
        number(value, "maximum_entries")?,
        optional_number(value, "continuation_offset")?.unwrap_or(0),
    )?
    .with_omission_offset(
        optional_number_for(value, "omission_offset", FsToolOperation::Discover)?.unwrap_or(0),
    )
    .with_path_offset(optional_number_for(value, "path_offset", FsToolOperation::Discover)?))
}

pub fn metadata(value: &BoundedJson) -> Result<MetadataInput, FsToolError> {
    MetadataInput::new(required_str(value, "path")?)
}

pub fn read(value: &BoundedJson) -> Result<ReadInput, FsToolError> {
    ReadInput::range(
        required_str(value, "path")?,
        optional_number(value, "offset")?.unwrap_or(0),
        number(value, "maximum_bytes")?,
    )
}

pub fn search(value: &BoundedJson) -> Result<SearchInput, FsToolError> {
    let root = optional_str(value, "root")?;
    let literal = required_str(value, "literal")?;
    let case_sensitive = required(value, "case_sensitive", FsToolOperation::Search)?
        .as_bool()
        .ok_or_else(|| invalid(FsToolOperation::Search))?;
    let maximum_depth = number(value, "maximum_depth")?;
    let maximum_file_bytes = number(value, "maximum_file_bytes")?;
    let maximum_matches = number(value, "maximum_matches")?;
    let continuation_offset = optional_number(value, "continuation_offset")?.unwrap_or(0);
    let omission_offset =
        optional_number_for(value, "omission_offset", FsToolOperation::Search)?.unwrap_or(0);
    let input = SearchInput::page(
        root,
        literal,
        case_sensitive,
        maximum_depth,
        maximum_file_bytes,
        maximum_matches,
    )?
    .with_continuation_offsets(continuation_offset, omission_offset);
    let field = match optional_str_for(value, "match_field", FsToolOperation::Search)?.as_deref() {
        None => None,
        Some("path") => Some(SearchMatchField::Path),
        Some("preview") => Some(SearchMatchField::Preview),
        Some(_) => return Err(invalid(FsToolOperation::Search)),
    };
    input
        .with_field_ranges(
            field,
            optional_number_for(value, "match_field_offset", FsToolOperation::Search)?.unwrap_or(0),
            optional_number_for(value, "omission_path_offset", FsToolOperation::Search)?,
        )
        .validate_field_ranges()
}

pub fn create(value: &BoundedJson) -> Result<CreateInput, FsToolError> {
    let (content, mode, endings) = final_fields(value, FsToolOperation::Create)?;
    let path = required_str(value, "path")?;
    match content {
        MutationContent::Inline(bytes) => CreateInput::new(path, bytes, mode, endings),
        MutationContent::Artifact(reference) => {
            CreateInput::from_artifact(path, reference, mode, endings)
        }
    }
}

pub fn write(value: &BoundedJson) -> Result<WriteInput, FsToolError> {
    let (content, mode, endings) = final_fields(value, FsToolOperation::Write)?;
    let path = required_str(value, "path")?;
    let preimage = preimage(&required(value, "preimage", FsToolOperation::Write)?, true)?;
    match content {
        MutationContent::Inline(bytes) => WriteInput::new(path, preimage, bytes, mode, endings),
        MutationContent::Artifact(reference) => {
            WriteInput::from_artifact(path, preimage, reference, mode, endings)
        }
    }
}

pub fn remove(value: &BoundedJson) -> Result<RemoveInput, FsToolError> {
    RemoveInput::new(
        required_str(value, "path")?,
        preimage(&required(value, "preimage", FsToolOperation::Remove)?, false)?,
    )
}

pub fn replace(value: &BoundedJson) -> Result<ReplaceInput, FsToolError> {
    let (content, mode, endings) = final_fields(value, FsToolOperation::Replace)?;
    let path = required_str(value, "path")?;
    let preimage = preimage(&required(value, "preimage", FsToolOperation::Replace)?, false)?;
    match content {
        MutationContent::Inline(bytes) => ReplaceInput::new(path, preimage, bytes, mode, endings),
        MutationContent::Artifact(reference) => {
            ReplaceInput::from_artifact(path, preimage, reference, mode, endings)
        }
    }
}

pub fn patch(value: &BoundedJson) -> Result<PatchInput, FsToolError> {
    let values = required(value, "edits", FsToolOperation::Patch)?
        .elements()
        .ok_or_else(|| invalid(FsToolOperation::Patch))?;
    let mut edits = Vec::with_capacity(values.len());
    for value in values {
        let operation = required_str_for(&value, "operation", FsToolOperation::Patch)?;
        edits.push(match operation.as_str() {
            "create" => PatchEdit::Create(create(&value)?),
            "replace" => PatchEdit::Replace(replace(&value)?),
            "remove" => PatchEdit::Remove(remove(&value)?),
            _ => return Err(invalid(FsToolOperation::Patch)),
        });
    }
    PatchInput::new(edits)
}

fn final_fields(
    value: &BoundedJson,
    operation: FsToolOperation,
) -> Result<(MutationContent, FileMode, LineEndingPolicy), FsToolError> {
    let content = match (value.property("content"), value.property("artifact")) {
        (Some(content), None) => {
            let content = content.as_str().ok_or_else(|| invalid(operation))?.to_owned();
            let bytes = match required_str_for(value, "content_encoding", operation)?.as_str() {
                "utf8" => content.into_bytes(),
                "base64" => STANDARD.decode(content).map_err(|_| invalid(operation))?,
                _ => return Err(invalid(operation)),
            };
            MutationContent::Inline(bytes)
        }
        (None, Some(reference)) if value.property("content_encoding").is_none() => {
            MutationContent::Artifact(artifact_reference(&reference)?)
        }
        _ => return Err(invalid(operation)),
    };
    let mode = match required_str_for(value, "mode", operation)?.as_str() {
        "regular" => FileMode::Regular,
        "executable" => FileMode::Executable,
        _ => return Err(invalid(operation)),
    };
    let endings = match required_str_for(value, "line_endings", operation)?.as_str() {
        "preserve" => LineEndingPolicy::Preserve,
        "lf" => LineEndingPolicy::Lf,
        "crlf" => LineEndingPolicy::Crlf,
        _ => return Err(invalid(operation)),
    };
    Ok((content, mode, endings))
}

fn artifact_reference(value: &BoundedJson) -> Result<ArtifactReference, FsToolError> {
    let digest = parse_digest(&required_str_for(value, "digest", FsToolOperation::Patch)?)?;
    let size = number_for(value, "size", FsToolOperation::Patch)?;
    let media_type =
        BoundedText::new(required_str_for(value, "media_type", FsToolOperation::Patch)?)
            .map_err(|_| invalid(FsToolOperation::Patch))?;
    let label = BoundedText::new(required_str_for(value, "label", FsToolOperation::Patch)?)
        .map_err(|_| invalid(FsToolOperation::Patch))?;
    let completeness =
        match required_str_for(value, "completeness", FsToolOperation::Patch)?.as_str() {
            "complete" => ArtifactCompleteness::Complete,
            "truncated" => ArtifactCompleteness::Truncated,
            "indeterminate" => ArtifactCompleteness::Indeterminate,
            _ => return Err(invalid(FsToolOperation::Patch)),
        };
    let provenance = required(value, "provenance", FsToolOperation::Patch)?;
    let action =
        parse_action_id(&required_str_for(&provenance, "action_id", FsToolOperation::Patch)?)?;
    let prepared =
        parse_digest(&required_str_for(&provenance, "prepared_digest", FsToolOperation::Patch)?)?;
    ArtifactReference::new(
        digest,
        size,
        media_type,
        label,
        completeness,
        ArtifactProvenance::new(action, prepared),
    )
    .map_err(|_| invalid(FsToolOperation::Patch))
}

fn parse_action_id(value: &str) -> Result<ActionId, FsToolError> {
    if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(FsToolOperation::Patch));
    }
    let mut bytes = [0_u8; 16];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = hex(pair[0])
            .and_then(|high| hex(pair[1]).map(|low| high * 16 + low))
            .ok_or_else(|| invalid(FsToolOperation::Patch))?;
    }
    ActionId::new(bytes).map_err(|_| invalid(FsToolOperation::Patch))
}

fn preimage(value: &BoundedJson, allow_absent: bool) -> Result<Preimage, FsToolError> {
    let state = required_str_for(value, "state", FsToolOperation::Patch)?;
    if state == "absent" && allow_absent {
        if value.property("digest").is_some()
            || value.property("size").is_some()
            || value.property("mode").is_some()
        {
            return Err(invalid(FsToolOperation::Patch));
        }
        return Ok(Preimage::Absent);
    }
    if state != "present" {
        return Err(invalid(FsToolOperation::Patch));
    }
    let digest = parse_digest(&required_str_for(value, "digest", FsToolOperation::Patch)?)?;
    let size = number_for(value, "size", FsToolOperation::Patch)?;
    let mode = match required_str_for(value, "mode", FsToolOperation::Patch)?.as_str() {
        "regular" => FileMode::Regular,
        "executable" => FileMode::Executable,
        _ => return Err(invalid(FsToolOperation::Patch)),
    };
    Ok(Preimage::present(digest, size, mode))
}

fn parse_digest(value: &str) -> Result<Sha256Digest, FsToolError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(FsToolOperation::Patch));
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = hex(pair[0])
            .and_then(|high| hex(pair[1]).map(|low| high * 16 + low))
            .ok_or_else(|| invalid(FsToolOperation::Patch))?;
    }
    Ok(Sha256Digest::new(bytes))
}

const fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn required(
    value: &BoundedJson,
    name: &str,
    operation: FsToolOperation,
) -> Result<BoundedJson, FsToolError> {
    value.property(name).ok_or_else(|| invalid(operation))
}

fn required_str(value: &BoundedJson, name: &str) -> Result<String, FsToolError> {
    required_str_for(value, name, FsToolOperation::Patch)
}

fn required_str_for(
    value: &BoundedJson,
    name: &str,
    operation: FsToolOperation,
) -> Result<String, FsToolError> {
    required(value, name, operation)?.as_str().map(str::to_owned).ok_or_else(|| invalid(operation))
}

fn optional_str(value: &BoundedJson, name: &str) -> Result<Option<String>, FsToolError> {
    optional_str_for(value, name, FsToolOperation::Patch)
}

fn optional_str_for(
    value: &BoundedJson,
    name: &str,
    operation: FsToolOperation,
) -> Result<Option<String>, FsToolError> {
    value
        .property(name)
        .map(|value| value.as_str().map(str::to_owned).ok_or_else(|| invalid(operation)))
        .transpose()
}

fn number<T>(value: &BoundedJson, name: &str) -> Result<T, FsToolError>
where
    T: TryFrom<i64>,
{
    number_for(value, name, FsToolOperation::Patch)
}

fn optional_number<T>(value: &BoundedJson, name: &str) -> Result<Option<T>, FsToolError>
where
    T: TryFrom<i64>,
{
    optional_number_for(value, name, FsToolOperation::Read)
}

fn optional_number_for<T>(
    value: &BoundedJson,
    name: &str,
    operation: FsToolOperation,
) -> Result<Option<T>, FsToolError>
where
    T: TryFrom<i64>,
{
    value
        .property(name)
        .map(|number| {
            number
                .as_i64()
                .and_then(|value| T::try_from(value).ok())
                .ok_or_else(|| invalid(operation))
        })
        .transpose()
}

fn number_for<T>(
    value: &BoundedJson,
    name: &str,
    operation: FsToolOperation,
) -> Result<T, FsToolError>
where
    T: TryFrom<i64>,
{
    required(value, name, operation)?
        .as_i64()
        .and_then(|value| T::try_from(value).ok())
        .ok_or_else(|| invalid(operation))
}

const fn invalid(operation: FsToolOperation) -> FsToolError {
    FsToolError::invalid(operation, "prepared filesystem arguments are structurally invalid")
}
