//! Strict schema-validated filesystem argument projection.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use peritus_patch::{FileMode, LineEndingPolicy, Preimage};
use peritus_tool_protocol::BoundedJson;
use peritus_types::Sha256Digest;

use crate::{
    CreateInput, DiscoverInput, FsToolError, FsToolOperation, MetadataInput, PatchEdit, PatchInput,
    ReadInput, RemoveInput, ReplaceInput, SearchInput, WriteInput,
};

pub fn discover(value: &BoundedJson) -> Result<DiscoverInput, FsToolError> {
    DiscoverInput::new(
        optional_str(value, "root")?,
        number(value, "maximum_depth")?,
        number(value, "maximum_entries")?,
    )
}

pub fn metadata(value: &BoundedJson) -> Result<MetadataInput, FsToolError> {
    MetadataInput::new(required_str(value, "path")?)
}

pub fn read(value: &BoundedJson) -> Result<ReadInput, FsToolError> {
    ReadInput::new(required_str(value, "path")?, number(value, "maximum_bytes")?)
}

pub fn search(value: &BoundedJson) -> Result<SearchInput, FsToolError> {
    SearchInput::new(
        optional_str(value, "root")?,
        required_str(value, "literal")?,
        required(value, "case_sensitive", FsToolOperation::Search)?
            .as_bool()
            .ok_or_else(|| invalid(FsToolOperation::Search))?,
        number(value, "maximum_depth")?,
        number(value, "maximum_entries")?,
        number(value, "maximum_file_bytes")?,
        number(value, "maximum_total_bytes")?,
        number(value, "maximum_matches")?,
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
) -> Result<(Vec<u8>, FileMode, LineEndingPolicy), FsToolError> {
    let content = required_str_for(value, "content", operation)?;
    let bytes = match required_str_for(value, "content_encoding", operation)?.as_str() {
        "utf8" => content.into_bytes(),
        "base64" => STANDARD.decode(content).map_err(|_| invalid(operation))?,
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
    Ok((bytes, mode, endings))
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
    value
        .property(name)
        .map(|value| {
            value.as_str().map(str::to_owned).ok_or_else(|| invalid(FsToolOperation::Patch))
        })
        .transpose()
}

fn number<T>(value: &BoundedJson, name: &str) -> Result<T, FsToolError>
where
    T: TryFrom<i64>,
{
    number_for(value, name, FsToolOperation::Patch)
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
