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
    super::append(path, 6, payload).map_err(|_| failure("persist local checkpoint trace"))
}

/// Replays only completed, explicitly scoped frames. A torn tail is never treated as an effect.
/// The caller receives its presence and must stop append-only work pending trace-tail recovery.
pub fn observations(
    path: &Path,
    scope: Sha256Digest,
    mut ingest: impl FnMut(
        u64,
        u64,
        CompletedToolCall,
        DeveloperToolObservation,
    ) -> Result<(), DeveloperLoopError>,
) -> Result<bool, DeveloperLoopError> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(failure("open scoped observation trace")),
    };
    let length = file.metadata().map_err(|_| failure("inspect trace length"))?.len();
    if length > 1024 * 1024 * 1024 {
        return Err(failure("observation trace exceeds recovery bound"));
    }
    let mut position = 0_u64;
    while position < length {
        if length - position < 9 {
            return Ok(true);
        }
        let mut header = [0; 9];
        file.read_exact(&mut header).map_err(|_| failure("read trace header"))?;
        let mut size = [0; 8];
        size.copy_from_slice(&header[1..]);
        let size = u64::from_le_bytes(size);
        position += 9;
        if size > length - position {
            return Ok(true);
        }
        if header[0] == 7 {
            if size > 64 * 1024 * 1024 {
                return Err(failure("scoped tool trace exceeds frame bound"));
            }
            let mut bytes =
                vec![0; usize::try_from(size).map_err(|_| failure("trace frame size overflow"))?];
            file.read_exact(&mut bytes).map_err(|_| failure("read scoped trace frame"))?;
            let record: ToolRecord =
                serde_json::from_slice(&bytes).map_err(|_| failure("invalid scoped tool trace"))?;
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
    Ok(false)
}

fn failure(detail: &str) -> DeveloperLoopError {
    DeveloperLoopError::Trace(detail.to_owned())
}
