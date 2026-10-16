//! Versioned exact observation/checkpoint records; legacy tags remain unchanged and readable.

use peritus_agent::{DeveloperLoopError, DeveloperToolObservation};
use peritus_model_protocol::{
    CanonicalJson, CompletedToolCall, JsonBounds, ProtocolLimits, ToolCallId, ToolName,
};
use peritus_types::Sha256Digest;
use serde::Deserialize;
use serde::Serialize;
use std::{
    fs::File,
    io::{Read as _, Seek as _},
    path::Path,
};

/// Verified prefix cursor. A changed/truncated trace falls back to ordered streaming recovery.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct TraceCursor {
    offset: u64,
    anchor: [u8; 32],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    identity: Option<[u64; 2]>,
}

fn identity(file: &File) -> Result<Option<[u64; 2]>, DeveloperLoopError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        let metadata = file.metadata().map_err(|_| failure("inspect trace identity"))?;
        Ok(Some([metadata.dev(), metadata.ino()]))
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        Ok(None)
    }
}

fn anchor(file: &mut File, offset: u64) -> Result<[u8; 32], DeveloperLoopError> {
    let start = offset.saturating_sub(4096);
    file.seek(std::io::SeekFrom::Start(start)).map_err(|_| failure("seek trace cursor"))?;
    let mut bytes = vec![0; (offset - start) as usize];
    file.read_exact(&mut bytes).map_err(|_| failure("read trace cursor anchor"))?;
    Ok(peritus_codec::sha256(&bytes).into_bytes())
}

#[derive(Clone, Copy)]
pub(super) struct Scope {
    pub(super) digest: [u8; 32],
    pub(super) invocation: u64,
    pub(super) observed: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ToolRecord {
    schema_version: u16,
    scope: [u8; 32],
    invocation: u64,
    tool_sequence: u64,
    call_id: String,
    name: String,
    arguments: String,
    output: String,
    is_error: bool,
}

pub(super) fn tool_payload(
    scope: Scope,
    call: &CompletedToolCall,
    observation: &DeveloperToolObservation,
) -> Result<Vec<u8>, DeveloperLoopError> {
    if scope.invocation == 0 {
        return Err(failure("invalid local trace invocation"));
    }
    serde_json::to_vec(&ToolRecord {
        schema_version: 1,
        scope: scope.digest,
        invocation: scope.invocation,
        tool_sequence: scope.observed,
        call_id: call.id().expose_for_wire().to_owned(),
        name: call.name().as_str().to_owned(),
        arguments: call.arguments().to_wire_string(),
        output: observation.output.to_wire_string(),
        is_error: observation.is_error,
    })
    .map_err(|_| failure("encode scoped tool trace"))
}

pub fn checkpoint(path: &Path, payload: &[u8]) -> Result<(), DeveloperLoopError> {
    super::append(path, super::DeveloperTraceFrameKind::LocalMemoryCheckpoint.tag(), payload)
        .map_err(|_| failure("persist local checkpoint trace"))
}

/// Replays only completed, explicitly scoped frames. A torn tail is never treated as an effect.
/// The caller receives its presence and must stop append-only work pending trace-tail recovery.
pub fn observations(
    path: &Path,
    scope: Sha256Digest,
    cursor: Option<&TraceCursor>,
    mut ingest: impl FnMut(
        u64,
        u64,
        CompletedToolCall,
        DeveloperToolObservation,
    ) -> Result<(), DeveloperLoopError>,
) -> Result<(bool, Option<TraceCursor>), DeveloperLoopError> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((false, None)),
        Err(_) => return Err(failure("open scoped observation trace")),
    };
    let length = file.metadata().map_err(|_| failure("inspect trace length"))?.len();
    let mut position = 0_u64;
    if let Some(cursor) = cursor {
        if cursor.offset <= length
            && cursor.identity == identity(&file)?
            && anchor(&mut file, cursor.offset)? == cursor.anchor
        {
            position = cursor.offset;
        }
    }
    file.seek(std::io::SeekFrom::Start(position))
        .map_err(|_| failure("seek observation suffix"))?;
    while position < length {
        if length - position < 9 {
            return Ok((true, None));
        }
        let mut header = [0; 9];
        file.read_exact(&mut header).map_err(|_| failure("read trace header"))?;
        let mut size = [0; 8];
        size.copy_from_slice(&header[1..]);
        let size = u64::from_le_bytes(size);
        position += 9;
        if size > length - position {
            return Ok((true, None));
        }
        if super::DeveloperTraceFrameKind::from_tag(header[0])
            == Some(super::DeveloperTraceFrameKind::LocalMemoryObservation)
        {
            // The frame length bounds the stream; no allocation is based on its untrusted prefix.
            let record: ToolRecord = serde_json::from_reader((&mut file).take(size))
                .map_err(|_| failure("invalid scoped tool trace"))?;
            if record.schema_version != 1 || record.invocation == 0 || record.tool_sequence == 0 {
                return Err(failure("unsupported scoped trace version or invocation"));
            }
            if record.scope == scope.into_bytes() {
                let bounds = JsonBounds::value(ProtocolLimits::PRODUCTION);
                let call = CompletedToolCall::new(
                    ToolCallId::new(record.call_id)?,
                    ToolName::new(record.name)?,
                    CanonicalJson::parse(&record.arguments, bounds)?,
                )?;
                let observation = DeveloperToolObservation {
                    output: CanonicalJson::parse(&record.output, bounds)?,
                    is_error: record.is_error,
                };
                ingest(record.invocation, record.tool_sequence, call, observation)?;
            }
        } else {
            file.seek(std::io::SeekFrom::Start(position + size))
                .map_err(|_| failure("skip legacy trace frame"))?;
        }
        position += size;
    }
    let anchor = anchor(&mut file, position)?;
    Ok((false, Some(TraceCursor { offset: position, anchor, identity: identity(&file)? })))
}

fn failure(detail: &str) -> DeveloperLoopError {
    DeveloperLoopError::Trace(detail.to_owned())
}
