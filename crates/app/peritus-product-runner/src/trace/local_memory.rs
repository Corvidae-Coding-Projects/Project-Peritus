//! Versioned exact observation/checkpoint records; legacy tags remain unchanged and readable.

use peritus_agent::{DeveloperLoopError, DeveloperToolObservation};
use peritus_model_protocol::{
    CanonicalJson, CompletedToolCall, JsonBounds, ProtocolLimits, ToolCallId, ToolName,
};
use peritus_types::Sha256Digest;
use serde::Deserialize;
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read as _, Seek as _, SeekFrom, Write as _},
    path::Path,
};

const MAX_RECEIPT_BYTES: u64 = 64 * 1024;
const MAX_TOOL_FRAME_BYTES: u64 = 64 * 1024 * 1024;
const RECOVERY_DIRECTORY: &str = "trace-tail-recovery-v1";

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

pub(super) fn decode_tool_payload(
    payload: &[u8],
) -> Result<(Scope, CompletedToolCall, DeveloperToolObservation), DeveloperLoopError> {
    let record: ToolRecord =
        serde_json::from_slice(payload).map_err(|_| failure("invalid scoped tool trace"))?;
    if record.schema_version != 1 || record.invocation == 0 || record.tool_sequence == 0 {
        return Err(failure("unsupported scoped trace version or invocation"));
    }
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
    Ok((
        Scope {
            digest: record.scope,
            invocation: record.invocation,
            observed: record.tool_sequence,
        },
        call,
        observation,
    ))
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

/// Appends one exact checkpoint observation unless its committed receipt is already present.
pub fn checkpoint_once(
    path: &Path,
    receipt: Sha256Digest,
    payload: &[u8],
) -> Result<(), DeveloperLoopError> {
    const MAX_CHECKPOINT_TRACE_BYTES: u64 = 32 * 1024 * 1024;
    if payload.len() as u64 > MAX_CHECKPOINT_TRACE_BYTES {
        return Err(failure("checkpoint trace frame exceeds schema envelope"));
    }
    if let Ok(mut file) = File::open(path) {
        let length = file.metadata().map_err(|_| failure("inspect checkpoint trace"))?.len();
        let mut position = 0_u64;
        while position < length {
            if length - position < 9 {
                return Err(failure("checkpoint trace has a torn tail"));
            }
            let mut header = [0; 9];
            file.read_exact(&mut header).map_err(|_| failure("read checkpoint trace header"))?;
            let mut size = [0; 8];
            size.copy_from_slice(&header[1..]);
            let size = u64::from_le_bytes(size);
            position = position
                .checked_add(9)
                .ok_or_else(|| failure("checkpoint trace position overflow"))?;
            if size > length - position {
                return Err(failure("checkpoint trace has a torn payload"));
            }
            if super::DeveloperTraceFrameKind::from_tag(header[0])
                == Some(super::DeveloperTraceFrameKind::LocalMemoryCheckpoint)
            {
                if size > MAX_CHECKPOINT_TRACE_BYTES {
                    return Err(failure("checkpoint trace frame exceeds schema envelope"));
                }
                let mut bytes = vec![
                    0;
                    usize::try_from(size)
                        .map_err(|_| failure("checkpoint trace frame size overflow"))?
                ];
                file.read_exact(&mut bytes)
                    .map_err(|_| failure("read checkpoint trace payload"))?;
                let value: serde_json::Value = serde_json::from_slice(&bytes)
                    .map_err(|_| failure("decode checkpoint trace payload"))?;
                let matches = value
                    .get("trace_receipt")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|encoded| {
                        encoded.len() == 32
                            && encoded.iter().zip(receipt.as_bytes()).all(|(value, expected)| {
                                value.as_u64() == Some(u64::from(*expected))
                            })
                    });
                if matches {
                    return if bytes == payload {
                        Ok(())
                    } else {
                        Err(failure("checkpoint trace receipt conflicts with durable payload"))
                    };
                }
            } else {
                file.seek(std::io::SeekFrom::Current(
                    i64::try_from(size)
                        .map_err(|_| failure("checkpoint trace seek size overflow"))?,
                ))
                .map_err(|_| failure("skip checkpoint trace frame"))?;
            }
            position = position
                .checked_add(size)
                .ok_or_else(|| failure("checkpoint trace position overflow"))?;
        }
    }
    checkpoint(path, payload)
}

/// Replays every completed, explicitly scoped frame under exclusive trace ownership.
///
/// Physical frames remain allocation-bounded while the complete framed lineage has no cumulative
/// byte or event allowance. Complete scoped-tool receipts are reconciled with their immediately
/// following source before that source is admitted. A torn tool/receipt frame remains in place as
/// uncertain effect evidence. A torn known non-effect frame is first retained as an explicitly
/// uncommitted recovery artifact, then removed from the same open trace file so its native identity
/// and permissions survive recovery.
pub fn observations(
    path: &Path,
    recovery_root: &Path,
    scope: Sha256Digest,
    mut ingest: impl FnMut(
        u64,
        u64,
        CompletedToolCall,
        DeveloperToolObservation,
    ) -> Result<(), DeveloperLoopError>,
) -> Result<(), DeveloperLoopError> {
    let mut file = match OpenOptions::new().read(true).write(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(failure("open scoped observation trace")),
    };
    file.lock().map_err(|_| failure("lock scoped observation trace"))?;
    file.sync_data()
        .map_err(|_| failure("sync scoped observation recovery frontier"))?;
    let source_metadata = file.metadata().map_err(|_| failure("inspect trace length"))?;
    if !source_metadata.is_file() {
        return Err(failure("scoped observation trace is not a regular file"));
    }
    let length = source_metadata.len();
    let mut position = 0_u64;
    let mut receipt = None;
    while position < length {
        if length - position < 9 {
            if receipt.is_some() {
                return Err(failure(
                    "scoped tool receipt source has a torn header; uncertain effect evidence retained",
                ));
            }
            file.seek(SeekFrom::Start(position))
                .map_err(|_| failure("seek torn trace header"))?;
            let mut tag = [0_u8; 1];
            file.read_exact(&mut tag).map_err(|_| failure("read torn trace tag"))?;
            let classification = repairable_torn_kind(tag[0])?;
            preserve_and_remove_uncommitted_tail(
                &mut file,
                path,
                recovery_root,
                scope,
                &source_metadata,
                position,
                length,
                tag[0],
                classification,
            )?;
            return Ok(());
        }
        let mut header = [0; 9];
        file.read_exact(&mut header).map_err(|_| failure("read trace header"))?;
        let mut size = [0; 8];
        size.copy_from_slice(&header[1..]);
        let size = u64::from_le_bytes(size);
        let payload_position = position
            .checked_add(9)
            .ok_or_else(|| failure("trace frame position overflow"))?;
        if size > length - payload_position {
            if receipt.is_some() {
                return Err(failure(
                    "scoped tool receipt source has a torn payload; uncertain effect evidence retained",
                ));
            }
            let classification = repairable_torn_kind(header[0])?;
            preserve_and_remove_uncommitted_tail(
                &mut file,
                path,
                recovery_root,
                scope,
                &source_metadata,
                position,
                length,
                header[0],
                classification,
            )?;
            return Ok(());
        }
        let end = payload_position
            .checked_add(size)
            .ok_or_else(|| failure("trace frame position overflow"))?;
        let kind = super::DeveloperTraceFrameKind::from_tag(header[0]);

        if let Some(expected) = receipt.take() {
            if kind != Some(expected.kind) || size != expected.bytes {
                return Err(failure("scoped tool receipt source header mismatch"));
            }
            let payload = read_payload(&mut file, size, MAX_TOOL_FRAME_BYTES)?;
            if peritus_codec::sha256(&payload).into_bytes() != expected.sha256 {
                return Err(failure("scoped tool receipt source digest mismatch"));
            }
            if expected.kind == super::DeveloperTraceFrameKind::LocalMemoryObservation {
                ingest_memory_payload(&payload, scope, &mut ingest)?;
            }
            position = end;
            continue;
        }

        match kind {
            Some(super::DeveloperTraceFrameKind::ScopedToolObservation) => {
                let payload = read_payload(&mut file, size, MAX_RECEIPT_BYTES)?;
                receipt = Some(super::scoped_tool::receipt_source(&payload)?);
            }
            Some(super::DeveloperTraceFrameKind::LocalMemoryObservation) => {
                let payload = read_payload(&mut file, size, MAX_TOOL_FRAME_BYTES)?;
                ingest_memory_payload(&payload, scope, &mut ingest)?;
            }
            _ => {
                file.seek(SeekFrom::Start(end))
                    .map_err(|_| failure("skip legacy trace frame"))?;
            }
        }
        position = end;
    }
    if receipt.is_some() {
        return Err(failure(
            "scoped tool receipt is missing its source observation; uncertain effect evidence retained",
        ));
    }
    Ok(())
}

fn ingest_memory_payload(
    payload: &[u8],
    scope: Sha256Digest,
    ingest: &mut impl FnMut(
        u64,
        u64,
        CompletedToolCall,
        DeveloperToolObservation,
    ) -> Result<(), DeveloperLoopError>,
) -> Result<(), DeveloperLoopError> {
    let (record_scope, call, observation) = decode_tool_payload(payload)?;
    if record_scope.digest == scope.into_bytes() {
        ingest(record_scope.invocation, record_scope.observed, call, observation)?;
    }
    Ok(())
}

fn read_payload(
    file: &mut File,
    size: u64,
    maximum: u64,
) -> Result<Vec<u8>, DeveloperLoopError> {
    if size > maximum {
        return Err(failure("scoped tool trace exceeds its physical frame bound"));
    }
    let mut payload = vec![
        0_u8;
        usize::try_from(size).map_err(|_| failure("trace frame size overflow"))?
    ];
    file.read_exact(&mut payload).map_err(|_| failure("read scoped trace frame"))?;
    Ok(payload)
}

fn repairable_torn_kind(tag: u8) -> Result<&'static str, DeveloperLoopError> {
    match super::DeveloperTraceFrameKind::from_tag(tag) {
        Some(super::DeveloperTraceFrameKind::ProviderEnvelope) => Ok("provider-envelope"),
        Some(super::DeveloperTraceFrameKind::ContextCompaction) => Ok("context-compaction"),
        Some(super::DeveloperTraceFrameKind::RetryScheduled) => Ok("retry-scheduled"),
        Some(super::DeveloperTraceFrameKind::ProviderSwitch) => Ok("provider-switch"),
        Some(super::DeveloperTraceFrameKind::LocalMemoryCheckpoint) => {
            Ok("local-memory-checkpoint")
        }
        Some(super::DeveloperTraceFrameKind::RetryAttempt) => Ok("retry-attempt"),
        Some(super::DeveloperTraceFrameKind::RetryDisposition) => Ok("retry-disposition"),
        Some(
            super::DeveloperTraceFrameKind::ToolObservation
            | super::DeveloperTraceFrameKind::LocalMemoryObservation
            | super::DeveloperTraceFrameKind::ScopedToolObservation,
        ) => Err(failure(
            "torn tool-effect trace frame is uncertain effect evidence and was retained in place",
        )),
        None => Err(failure(
            "torn unknown trace frame may contain effect evidence and was retained in place",
        )),
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct UncommittedTailManifest {
    schema_version: u16,
    classification: &'static str,
    scope: [u8; 32],
    source_path_encoding: &'static str,
    source_path_units: Vec<u32>,
    source_identity: String,
    source_permissions: String,
    source_bytes: u64,
    committed_bytes: u64,
    tail_bytes: u64,
    tail_sha256: [u8; 32],
    tail_file: String,
    torn_frame_tag: u8,
    torn_frame_kind: &'static str,
}

#[allow(clippy::too_many_arguments)]
fn preserve_and_remove_uncommitted_tail(
    file: &mut File,
    source_path: &Path,
    recovery_root: &Path,
    scope: Sha256Digest,
    source_metadata: &fs::Metadata,
    committed: u64,
    source_bytes: u64,
    torn_tag: u8,
    torn_kind: &'static str,
) -> Result<(), DeveloperLoopError> {
    let tail_bytes = source_bytes
        .checked_sub(committed)
        .ok_or_else(|| failure("trace recovery frontier exceeds source length"))?;
    if tail_bytes == 0 {
        return Err(failure("trace recovery tail is empty"));
    }
    validate_source_identity(source_path, file, source_metadata, source_bytes)?;

    let directory = recovery_root.join(RECOVERY_DIRECTORY);
    fs::create_dir_all(&directory).map_err(|_| failure("create trace-tail recovery directory"))?;
    let directory_metadata = fs::symlink_metadata(&directory)
        .map_err(|_| failure("inspect trace-tail recovery directory"))?;
    if !directory_metadata.is_dir() || directory_metadata.file_type().is_symlink() {
        return Err(failure("trace-tail recovery path is not an owned directory"));
    }
    sync_directory(recovery_root)?;

    file.seek(SeekFrom::Start(committed))
        .map_err(|_| failure("seek uncommitted trace tail"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(&directory)
        .map_err(|_| failure("create uncommitted trace-tail artifact"))?;
    let mut hasher = Sha256::new();
    let mut remaining = tail_bytes;
    let mut buffer = vec![0_u8; 64 * 1024];
    while remaining != 0 {
        let limit = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| failure("uncommitted trace-tail length overflow"))?;
        let count = file
            .read(&mut buffer[..limit])
            .map_err(|_| failure("read uncommitted trace tail"))?;
        if count == 0 {
            return Err(failure("uncommitted trace tail changed while it was preserved"));
        }
        temporary
            .write_all(&buffer[..count])
            .map_err(|_| failure("write uncommitted trace-tail artifact"))?;
        hasher.update(&buffer[..count]);
        remaining = remaining
            .checked_sub(u64::try_from(count).map_err(|_| failure("trace-tail count overflow"))?)
            .ok_or_else(|| failure("trace-tail count underflow"))?;
    }
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| failure("sync uncommitted trace-tail artifact"))?;
    let tail_sha256: [u8; 32] = hasher.finalize().into();
    let tail_file = format!("{}.UNCOMMITTED.trace-tail", super::digest_hex(&tail_sha256));
    persist_temporary_exact(
        temporary,
        &directory.join(&tail_file),
        tail_sha256,
        tail_bytes,
    )?;
    sync_directory(&directory)?;

    let (source_path_encoding, source_path_units) = native_path(source_path);
    let manifest = UncommittedTailManifest {
        schema_version: 1,
        classification: "uncommitted-non-effect-trace-tail",
        scope: scope.into_bytes(),
        source_path_encoding,
        source_path_units,
        source_identity: file_identity(source_metadata)?,
        source_permissions: file_permissions(source_metadata),
        source_bytes,
        committed_bytes: committed,
        tail_bytes,
        tail_sha256,
        tail_file,
        torn_frame_tag: torn_tag,
        torn_frame_kind: torn_kind,
    };
    let manifest = serde_json::to_vec(&manifest)
        .map_err(|_| failure("encode uncommitted trace-tail manifest"))?;
    let manifest_sha256 = peritus_codec::sha256(&manifest).into_bytes();
    let manifest_path = directory.join(format!(
        "{}.UNCOMMITTED.json",
        super::digest_hex(&manifest_sha256)
    ));
    persist_bytes_exact(&directory, &manifest_path, &manifest, manifest_sha256)?;
    sync_directory(&directory)?;

    validate_source_identity(source_path, file, source_metadata, source_bytes)?;
    file.set_len(committed).map_err(|_| failure("remove preserved uncommitted trace tail"))?;
    file.sync_all().map_err(|_| failure("sync repaired trace frontier"))?;
    validate_source_identity(source_path, file, source_metadata, committed)?;
    Ok(())
}

fn persist_bytes_exact(
    directory: &Path,
    path: &Path,
    bytes: &[u8],
    digest: [u8; 32],
) -> Result<(), DeveloperLoopError> {
    let mut temporary = tempfile::NamedTempFile::new_in(directory)
        .map_err(|_| failure("create trace-tail recovery manifest"))?;
    temporary
        .write_all(bytes)
        .map_err(|_| failure("write trace-tail recovery manifest"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| failure("sync trace-tail recovery manifest"))?;
    persist_temporary_exact(
        temporary,
        path,
        digest,
        u64::try_from(bytes.len()).map_err(|_| failure("trace-tail manifest length overflow"))?,
    )
}

fn persist_temporary_exact(
    temporary: tempfile::NamedTempFile,
    path: &Path,
    digest: [u8; 32],
    bytes: u64,
) -> Result<(), DeveloperLoopError> {
    match temporary.persist_noclobber(path) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            verify_preserved_file(path, digest, bytes)
        }
        Err(_) => Err(failure("publish trace-tail recovery artifact")),
    }
}

fn verify_preserved_file(
    path: &Path,
    expected_digest: [u8; 32],
    expected_bytes: u64,
) -> Result<(), DeveloperLoopError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| failure("inspect existing trace-tail recovery artifact"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() != expected_bytes {
        return Err(failure("trace-tail recovery artifact conflicts with its identity"));
    }
    let mut file = File::open(path).map_err(|_| failure("open trace-tail recovery artifact"))?;
    let opened = file.metadata().map_err(|_| failure("inspect trace-tail recovery artifact"))?;
    if !same_file_identity(&metadata, &opened) {
        return Err(failure("trace-tail recovery artifact changed while it was opened"));
    }
    let mut hasher = Sha256::new();
    let mut count = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| failure("read trace-tail recovery artifact"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        count = count
            .checked_add(u64::try_from(read).map_err(|_| failure("recovery byte count overflow"))?)
            .ok_or_else(|| failure("recovery byte count overflow"))?;
    }
    let actual: [u8; 32] = hasher.finalize().into();
    if count != expected_bytes || actual != expected_digest {
        return Err(failure("trace-tail recovery artifact conflicts with its content"));
    }
    Ok(())
}

fn validate_source_identity(
    path: &Path,
    file: &File,
    expected: &fs::Metadata,
    expected_bytes: u64,
) -> Result<(), DeveloperLoopError> {
    let opened = file.metadata().map_err(|_| failure("inspect open trace source"))?;
    let current = fs::metadata(path).map_err(|_| failure("inspect trace source path"))?;
    if !opened.is_file()
        || !current.is_file()
        || opened.len() != expected_bytes
        || current.len() != expected_bytes
        || !same_file_identity(expected, &opened)
        || !same_file_identity(expected, &current)
        || expected.permissions() != opened.permissions()
        || expected.permissions() != current.permissions()
    {
        return Err(failure(
            "trace source identity or permissions changed during tail recovery",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn native_path(path: &Path) -> (&'static str, Vec<u32>) {
    use std::os::unix::ffi::OsStrExt as _;
    ("unix-bytes", path.as_os_str().as_bytes().iter().map(|byte| u32::from(*byte)).collect())
}

#[cfg(windows)]
fn native_path(path: &Path) -> (&'static str, Vec<u32>) {
    use std::os::windows::ffi::OsStrExt as _;
    ("windows-wide", path.as_os_str().encode_wide().map(u32::from).collect())
}

#[cfg(not(any(unix, windows)))]
fn native_path(path: &Path) -> (&'static str, Vec<u32>) {
    ("unicode-scalar", path.to_string_lossy().chars().map(u32::from).collect())
}

#[cfg(unix)]
fn file_identity(metadata: &fs::Metadata) -> Result<String, DeveloperLoopError> {
    use std::os::unix::fs::MetadataExt as _;
    Ok(format!("unix:{}:{}", metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn file_identity(metadata: &fs::Metadata) -> Result<String, DeveloperLoopError> {
    use std::os::windows::fs::MetadataExt as _;
    let volume = metadata
        .volume_serial_number()
        .ok_or_else(|| failure("trace source has no stable volume identity"))?;
    let index = metadata
        .file_index()
        .ok_or_else(|| failure("trace source has no stable file identity"))?;
    Ok(format!("windows:{volume}:{index}"))
}

#[cfg(not(any(unix, windows)))]
fn file_identity(metadata: &fs::Metadata) -> Result<String, DeveloperLoopError> {
    let created = metadata
        .created()
        .and_then(|created| {
            created.duration_since(std::time::UNIX_EPOCH).map_err(std::io::Error::other)
        })
        .map_err(|_| failure("trace source has no stable native identity"))?;
    Ok(format!("portable:{}", created.as_nanos()))
}

#[cfg(unix)]
fn file_permissions(metadata: &fs::Metadata) -> String {
    use std::os::unix::fs::PermissionsExt as _;
    format!("unix:{:o}", metadata.permissions().mode())
}

#[cfg(not(unix))]
fn file_permissions(metadata: &fs::Metadata) -> String {
    format!("readonly:{}", metadata.permissions().readonly())
}

#[cfg(unix)]
fn same_file_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(windows)]
fn same_file_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    left.volume_serial_number().is_some()
        && left.volume_serial_number() == right.volume_serial_number()
        && left.file_index().is_some()
        && left.file_index() == right.file_index()
}

#[cfg(not(any(unix, windows)))]
fn same_file_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.created().ok() == right.created().ok()
}

fn sync_directory(path: &Path) -> Result<(), DeveloperLoopError> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| failure("sync trace-tail recovery directory"))?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn failure(detail: &str) -> DeveloperLoopError {
    DeveloperLoopError::Trace(detail.to_owned())
}
