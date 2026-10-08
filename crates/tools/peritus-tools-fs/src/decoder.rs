//! Strict schema-validated filesystem argument projection.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use peritus_patch::{FileMode, LineEndingPolicy, Preimage};
use peritus_tool_protocol::BoundedJson;
use peritus_types::Sha256Digest;

use crate::{
    CreateInput, DiscoverInput, FsToolError, FsToolOperation, MetadataInput, MutationContent,
    PatchEdit, PatchInput, ReadInput, RemoveInput, ReplaceInput, SearchInput, WriteInput,
};

pub fn discover(value: &BoundedJson) -> Result<DiscoverInput, FsToolError> {
    let root = optional_str_for(value, "root", FsToolOperation::Discover)?;
    let depth = number_for(value, "maximum_depth", FsToolOperation::Discover)?;
    let entries = number_for(value, "maximum_entries", FsToolOperation::Discover)?;
    optional_str_for(value, "cursor", FsToolOperation::Discover)?.map_or_else(
        || DiscoverInput::new(root, depth, entries),
        |cursor| DiscoverInput::resume(root, depth, entries, cursor),
    )
}

pub fn metadata(value: &BoundedJson) -> Result<MetadataInput, FsToolError> {
    MetadataInput::new(required_str(value, "path")?)
}

pub fn read(value: &BoundedJson) -> Result<ReadInput, FsToolError> {
    let path = required_str_for(value, "path", FsToolOperation::Read)?;
    let maximum = number_for(value, "maximum_bytes", FsToolOperation::Read)?;
    match (
        optional_str_for(value, "cursor", FsToolOperation::Read)?,
        optional_number_for(value, "offset", FsToolOperation::Read)?,
    ) {
        (Some(cursor), None) => ReadInput::resume(path, maximum, cursor),
        (None, offset) => ReadInput::at_offset(path, maximum, offset.unwrap_or(0)),
        (Some(_), Some(_)) => Err(invalid(FsToolOperation::Read)),
    }
}

pub fn search(value: &BoundedJson) -> Result<SearchInput, FsToolError> {
    let root = optional_str_for(value, "root", FsToolOperation::Search)?;
    let literal = required_str_for(value, "literal", FsToolOperation::Search)?;
    let sensitive = required(value, "case_sensitive", FsToolOperation::Search)?
        .as_bool()
        .ok_or_else(|| invalid(FsToolOperation::Search))?;
    let depth = number_for(value, "maximum_depth", FsToolOperation::Search)?;
    let entries = number_for(value, "maximum_entries", FsToolOperation::Search)?;
    let file_bytes = number_for(value, "maximum_file_bytes", FsToolOperation::Search)?;
    let total_bytes = number_for(value, "maximum_total_bytes", FsToolOperation::Search)?;
    let matches = number_for(value, "maximum_matches", FsToolOperation::Search)?;
    optional_str_for(value, "cursor", FsToolOperation::Search)?.map_or_else(
        || SearchInput::new(root, literal, sensitive, depth, entries, file_bytes, total_bytes, matches),
        |cursor| SearchInput::resume(root, literal, sensitive, depth, entries, file_bytes, total_bytes, matches, cursor),
    )
}

pub fn create(value: &BoundedJson) -> Result<CreateInput, FsToolError> {
    let (bytes, mode, endings) = final_fields(value, FsToolOperation::Create)?;
    CreateInput::new(required_str(value, "path")?, bytes, mode, endings)
}

pub fn write(value: &BoundedJson) -> Result<WriteInput, FsToolError> {
    let (bytes, mode, endings) = final_fields(value, FsToolOperation::Write)?;
    WriteInput::new(
        required_str(value, "path")?,
        preimage(&required(value, "preimage", FsToolOperation::Write)?, true)?,
        bytes,
        mode,
        endings,
    )
}

pub fn remove(value: &BoundedJson) -> Result<RemoveInput, FsToolError> {
    RemoveInput::new(
        required_str(value, "path")?,
        preimage(&required(value, "preimage", FsToolOperation::Remove)?, false)?,
    )
}

pub fn replace(value: &BoundedJson) -> Result<ReplaceInput, FsToolError> {
    let (bytes, mode, endings) = final_fields(value, FsToolOperation::Replace)?;
    ReplaceInput::new(
        required_str(value, "path")?,
        preimage(&required(value, "preimage", FsToolOperation::Replace)?, false)?,
        bytes,
        mode,
        endings,
    )
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
    let source = optional_str_for(value, "content_source", operation)?;
    let content = optional_str_for(value, "content", operation)?;
    let encoding = optional_str_for(value, "content_encoding", operation)?;
    let artifact_digest = optional_str_for(value, "artifact_digest", operation)?;
    let artifact_bytes = optional_number_for(value, "artifact_bytes", operation)?;
    let content = match (source.as_deref(), content, encoding, artifact_digest, artifact_bytes) {
        (None | Some("inline"), Some(content), Some(encoding), None, None) => {
            let bytes = match encoding.as_str() {
                "utf8" => content.into_bytes(),
                "base64" => STANDARD.decode(content).map_err(|_| invalid(operation))?,
                _ => return Err(invalid(operation)),
            };
            MutationContent::Inline(bytes)
        }
        (Some("artifact"), None, None, Some(digest), Some(bytes)) => {
            MutationContent::artifact(parse_digest(&digest, operation)?, bytes)
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
    let digest = parse_digest(
        &required_str_for(value, "digest", FsToolOperation::Patch)?,
        FsToolOperation::Patch,
    )?;
    let size = number_for(value, "size", FsToolOperation::Patch)?;
    let mode = match required_str_for(value, "mode", FsToolOperation::Patch)?.as_str() {
        "regular" => FileMode::Regular,
        "executable" => FileMode::Executable,
        _ => return Err(invalid(FsToolOperation::Patch)),
    };
    Ok(Preimage::present(digest, size, mode))
}

fn parse_digest(value: &str, operation: FsToolOperation) -> Result<Sha256Digest, FsToolError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(operation));
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = hex(pair[0])
            .and_then(|high| hex(pair[1]).map(|low| high * 16 + low))
            .ok_or_else(|| invalid(operation))?;
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
        .map(|value| {
            value
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
