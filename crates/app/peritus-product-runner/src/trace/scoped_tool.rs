//! Additive scope bindings for successful host tool observations already stored in this trace.

use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read as _, Seek as _},
    path::Path,
};

use peritus_agent::{DeveloperLoopError, DeveloperToolObservation};
use peritus_model_protocol::{
    CanonicalJson, CompletedToolCall, JsonBounds, ProtocolLimits, ToolCallId, ToolName,
};
use peritus_types::Sha256Digest;
use serde::{Deserialize, Serialize};

use super::DeveloperTraceFrameKind;

const RECEIPT_SCHEMA_VERSION: u16 = 1;
const MAX_RECEIPT_BYTES: u64 = 64 * 1024;
const MAX_SOURCE_FRAME_BYTES: u64 = 64 * 1024 * 1024;

/// Exact logical host scope attached to tool observations from one reviewer lineage.
#[derive(Clone)]
pub(crate) struct ScopedToolReceiptPolicy {
    scope: Sha256Digest,
    logical_prefix: String,
    protected_legacy_handle: String,
    protected_legacy_tools: Vec<String>,
}

pub(super) struct ScopedToolReceiptBinding {
    scope: Sha256Digest,
    request_prefix: String,
    invocation: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema_version: u16,
    scope: [u8; 32],
    request_prefix: String,
    invocation: u64,
    source_tag: u8,
    source_bytes: u64,
    source_sha256: [u8; 32],
}

#[derive(Clone, Copy)]
pub(super) struct ReceiptSource {
    pub(super) kind: DeveloperTraceFrameKind,
    pub(super) bytes: u64,
    pub(super) sha256: [u8; 32],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyToolObservation {
    call_id: String,
    name: String,
    arguments: String,
    output: String,
    is_error: bool,
}

impl ScopedToolReceiptPolicy {
    /// Creates one exact run/workspace/reviewer-source scope and its admitted invocation prefix.
    ///
    /// # Errors
    /// Rejects an incomplete logical prefix or legacy ambiguity policy.
    pub(crate) fn new(
        scope: Sha256Digest,
        logical_prefix: String,
        protected_legacy_handle: String,
        protected_legacy_tools: &[&str],
    ) -> Result<Self, DeveloperLoopError> {
        if logical_prefix.is_empty() {
            return Err(failure("scoped tool receipt has an empty logical request prefix"));
        }
        if protected_legacy_handle.is_empty()
            || protected_legacy_tools.is_empty()
            || protected_legacy_tools.iter().any(|name| name.is_empty())
        {
            return Err(failure(
                "scoped tool receipt has an incomplete legacy ambiguity policy",
            ));
        }
        Ok(Self {
            scope,
            logical_prefix,
            protected_legacy_handle,
            protected_legacy_tools: protected_legacy_tools
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
        })
    }

    pub(super) fn bind(
        self,
        request_prefix: &str,
    ) -> Result<ScopedToolReceiptBinding, DeveloperLoopError> {
        let invocation = invocation(request_prefix, &self.logical_prefix)?;
        Ok(ScopedToolReceiptBinding {
            scope: self.scope,
            request_prefix: request_prefix.to_owned(),
            invocation,
        })
    }
}

pub(super) fn receipt_payload(
    binding: &ScopedToolReceiptBinding,
    source_tag: DeveloperTraceFrameKind,
    source_payload: &[u8],
) -> Result<Vec<u8>, DeveloperLoopError> {
    if !matches!(
        source_tag,
        DeveloperTraceFrameKind::ToolObservation
            | DeveloperTraceFrameKind::LocalMemoryObservation
    )
    {
        return Err(failure("scoped tool receipt source is not a tool observation"));
    }
    let source_bytes = u64::try_from(source_payload.len())
        .map_err(|_| failure("scoped tool receipt source size overflow"))?;
    if source_bytes > MAX_SOURCE_FRAME_BYTES {
        return Err(failure(
            "scoped tool receipt source exceeds its physical frame bound",
        ));
    }
    let payload = serde_json::to_vec(&Receipt {
        schema_version: RECEIPT_SCHEMA_VERSION,
        scope: binding.scope.into_bytes(),
        request_prefix: binding.request_prefix.clone(),
        invocation: binding.invocation,
        source_tag: source_tag.tag(),
        source_bytes,
        source_sha256: peritus_codec::sha256(source_payload).into_bytes(),
    })
    .map_err(|_| failure("encode scoped tool receipt"))?;
    if u64::try_from(payload.len())
        .map_err(|_| failure("scoped tool receipt size overflow"))?
        > MAX_RECEIPT_BYTES
    {
        return Err(failure("scoped tool receipt exceeds its physical frame bound"));
    }
    Ok(payload)
}

/// Replays exact successful host observations in their durable trace order.
///
/// The receipt is metadata only. Its immediately following legacy tag 2 or local-memory tag 7
/// frame remains the sole call/output payload. The receipt is synced before that source is
/// appended, so a crash cannot retain an apparently unscoped successful observation. Conflicting
/// call identity reuse, an incomplete pair, or a matching scope under another request fails closed.
///
/// # Errors
/// Rejects malformed, torn, conflicting, wrongly scoped, or non-successful bound observations.
pub(crate) fn replay(
    path: &Path,
    policy: &ScopedToolReceiptPolicy,
    observe: &mut dyn FnMut(&CompletedToolCall, &CanonicalJson) -> Result<(), DeveloperLoopError>,
) -> Result<(), DeveloperLoopError> {
    let mut file = match OpenOptions::new().read(true).write(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(failure("open scoped tool receipt trace")),
    };
    file.lock()
        .map_err(|_| failure("lock scoped tool receipt trace"))?;
    let length = file.metadata().map_err(|_| failure("inspect scoped tool trace"))?.len();
    let mut identities = BTreeMap::<(String, String), Sha256Digest>::new();
    let mut position = 0_u64;
    while position < length {
        if length - position < 9 {
            return Err(failure("scoped tool receipt trace has a torn header"));
        }
        let (tag, size) = read_header(&mut file)?;
        position = position
            .checked_add(9)
            .ok_or_else(|| failure("scoped tool trace position overflow"))?;
        if size > length - position {
            return Err(failure("scoped tool receipt trace has a torn payload"));
        }
        let next = position
            .checked_add(size)
            .ok_or_else(|| failure("scoped tool trace position overflow"))?;
        let kind = DeveloperTraceFrameKind::from_tag(tag);
        if kind != Some(DeveloperTraceFrameKind::ScopedToolObservation) {
            if matches!(
                kind,
                Some(
                    DeveloperTraceFrameKind::ToolObservation
                        | DeveloperTraceFrameKind::LocalMemoryObservation
                )
            ) {
                let source_kind = kind
                    .ok_or_else(|| failure("tool observation kind disappeared"))?;
                let (call, observation) =
                    read_observation_payload(&mut file, size, source_kind, None)?;
                reject_ambiguous_legacy_success(policy, &call, &observation)?;
            }
            file.seek(std::io::SeekFrom::Start(next))
                .map_err(|_| failure("skip unscoped trace frame"))?;
            position = next;
            continue;
        }
        if size > MAX_RECEIPT_BYTES {
            return Err(failure("scoped tool receipt exceeds its physical frame bound"));
        }
        let mut bytes = vec![
            0;
            usize::try_from(size)
                .map_err(|_| failure("scoped tool receipt size overflow"))?
        ];
        file.read_exact(&mut bytes)
            .map_err(|_| failure("read scoped tool receipt"))?;
        let receipt = decode_receipt(&bytes)?;
        let source_kind = receipt_source_for(&receipt)?.kind;
        let (call, observation, source_end) =
            read_source(&mut file, length, next, &receipt, source_kind)?;
        position = source_end;
        if observation.is_error {
            return Err(failure("scoped tool receipt references an unsuccessful observation"));
        }
        if receipt.scope != policy.scope.into_bytes() {
            continue;
        }
        if receipt.invocation != invocation(&receipt.request_prefix, &policy.logical_prefix)? {
            return Err(failure("scoped tool receipt request binding mismatch"));
        }
        let identity = receipt_identity(&call, &observation.output)?;
        let key = (
            receipt.request_prefix,
            call.id().expose_for_wire().to_owned(),
        );
        if let Some(previous) = identities.insert(key, identity) {
            if previous != identity {
                return Err(failure("scoped tool call identity was reused with conflicting content"));
            }
            continue;
        }
        observe(&call, &observation.output)?;
    }
    // A prior writer may have returned an error from the final source sync after writing a
    // complete receipt/source pair. Do not let readable cache state authorize recovery until the
    // same locked complete trace has successfully crossed a fresh durability boundary.
    file.sync_data()
        .map_err(|_| failure("sync recovered scoped tool receipt trace"))?;
    Ok(())
}

pub(super) fn receipt_source(payload: &[u8]) -> Result<ReceiptSource, DeveloperLoopError> {
    if u64::try_from(payload.len())
        .map_err(|_| failure("scoped tool receipt size overflow"))?
        > MAX_RECEIPT_BYTES
    {
        return Err(failure("scoped tool receipt exceeds its physical frame bound"));
    }
    receipt_source_for(&decode_receipt(payload)?)
}

fn decode_receipt(payload: &[u8]) -> Result<Receipt, DeveloperLoopError> {
    let receipt: Receipt = serde_json::from_slice(payload)
        .map_err(|_| failure("decode scoped tool receipt"))?;
    if receipt.schema_version != RECEIPT_SCHEMA_VERSION {
        return Err(failure("unsupported scoped tool receipt version"));
    }
    Ok(receipt)
}

fn receipt_source_for(receipt: &Receipt) -> Result<ReceiptSource, DeveloperLoopError> {
    let kind = DeveloperTraceFrameKind::from_tag(receipt.source_tag)
        .ok_or_else(|| failure("scoped tool receipt names an unknown source kind"))?;
    if !matches!(
        kind,
        DeveloperTraceFrameKind::ToolObservation
            | DeveloperTraceFrameKind::LocalMemoryObservation
    ) {
        return Err(failure("scoped tool receipt source is not a tool observation"));
    }
    if receipt.source_bytes > MAX_SOURCE_FRAME_BYTES {
        return Err(failure(
            "scoped tool receipt source exceeds its physical frame bound",
        ));
    }
    Ok(ReceiptSource {
        kind,
        bytes: receipt.source_bytes,
        sha256: receipt.source_sha256,
    })
}

fn read_source(
    file: &mut File,
    trace_length: u64,
    source_offset: u64,
    receipt: &Receipt,
    source_kind: DeveloperTraceFrameKind,
) -> Result<(CompletedToolCall, DeveloperToolObservation, u64), DeveloperLoopError> {
    if source_offset == trace_length {
        return Err(failure("scoped tool receipt is missing its source observation"));
    }
    if trace_length - source_offset < 9 {
        return Err(failure("scoped tool receipt source has a torn header"));
    }
    file.seek(std::io::SeekFrom::Start(source_offset))
        .map_err(|_| failure("seek scoped tool receipt source"))?;
    let (tag, size) = read_header(file)?;
    if tag != receipt.source_tag || size != receipt.source_bytes {
        return Err(failure("scoped tool receipt source header mismatch"));
    }
    let end = source_offset
        .checked_add(9)
        .and_then(|position| position.checked_add(size))
        .ok_or_else(|| failure("scoped tool receipt source position overflow"))?;
    if end > trace_length {
        return Err(failure("scoped tool receipt source has a torn payload"));
    }
    let (call, observation) = read_observation_payload(
        file,
        size,
        source_kind,
        Some(receipt.source_sha256),
    )?;
    Ok((call, observation, end))
}

fn read_observation_payload(
    file: &mut File,
    size: u64,
    source_kind: DeveloperTraceFrameKind,
    expected_digest: Option<[u8; 32]>,
) -> Result<(CompletedToolCall, DeveloperToolObservation), DeveloperLoopError> {
    if size > MAX_SOURCE_FRAME_BYTES {
        return Err(failure(
            "scoped tool receipt source exceeds its physical frame bound",
        ));
    }
    let mut payload = vec![
        0;
        usize::try_from(size)
            .map_err(|_| failure("scoped tool receipt source size overflow"))?
    ];
    file.read_exact(&mut payload)
        .map_err(|_| failure("read scoped tool receipt source"))?;
    if expected_digest.is_some_and(|digest| {
        peritus_codec::sha256(&payload).into_bytes() != digest
    }) {
        return Err(failure("scoped tool receipt source digest mismatch"));
    }
    match source_kind {
        DeveloperTraceFrameKind::ToolObservation => decode_legacy(&payload),
        DeveloperTraceFrameKind::LocalMemoryObservation => {
            let (_, call, observation) = super::local_memory::decode_tool_payload(&payload)?;
            Ok((call, observation))
        }
        _ => Err(failure("scoped tool receipt source kind changed during decode")),
    }
}

fn reject_ambiguous_legacy_success(
    policy: &ScopedToolReceiptPolicy,
    call: &CompletedToolCall,
    observation: &DeveloperToolObservation,
) -> Result<(), DeveloperLoopError> {
    if observation.is_error
        || !policy
            .protected_legacy_tools
            .iter()
            .any(|name| name.as_str() == call.name().as_str())
    {
        return Ok(());
    }
    #[derive(Deserialize)]
    struct HandleArgument {
        handle: String,
    }
    let arguments: HandleArgument = serde_json::from_slice(call.arguments().canonical_bytes())
        .map_err(|_| failure("decode legacy protected tool identity"))?;
    if arguments.handle == policy.protected_legacy_handle {
        return Err(failure(
            "successful protected tool observation lacks an exact admitted request receipt",
        ));
    }
    Ok(())
}

fn decode_legacy(
    payload: &[u8],
) -> Result<(CompletedToolCall, DeveloperToolObservation), DeveloperLoopError> {
    let record: LegacyToolObservation = serde_json::from_slice(payload)
        .map_err(|_| failure("decode legacy scoped tool observation"))?;
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
    Ok((call, observation))
}

fn read_header(file: &mut File) -> Result<(u8, u64), DeveloperLoopError> {
    let mut header = [0; 9];
    file.read_exact(&mut header)
        .map_err(|_| failure("read scoped tool trace header"))?;
    let mut size = [0; 8];
    size.copy_from_slice(&header[1..]);
    Ok((header[0], u64::from_le_bytes(size)))
}

fn invocation(request_prefix: &str, logical_prefix: &str) -> Result<u64, DeveloperLoopError> {
    let suffix = request_prefix
        .strip_prefix(logical_prefix)
        .ok_or_else(|| failure("scoped tool receipt is outside its logical request prefix"))?;
    let (sequence, nonce) = suffix
        .split_once('-')
        .ok_or_else(|| failure("scoped tool receipt invocation identity is incomplete"))?;
    if sequence.is_empty()
        || (sequence != "0" && sequence.starts_with('0'))
        || !sequence.bytes().all(|byte| byte.is_ascii_digit())
        || nonce.len() != 32
        || !nonce.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(failure("scoped tool receipt invocation identity is invalid"));
    }
    let invocation = sequence
        .parse()
        .map_err(|_| failure("scoped tool receipt invocation overflow"))?;
    if invocation == 0 {
        return Err(failure("scoped tool receipt invocation is zero"));
    }
    Ok(invocation)
}

fn receipt_identity(
    call: &CompletedToolCall,
    output: &CanonicalJson,
) -> Result<Sha256Digest, DeveloperLoopError> {
    let mut bytes = b"peritus/scoped-tool-receipt-identity/v1\0".to_vec();
    append_bytes(&mut bytes, call.name().as_str().as_bytes())?;
    append_bytes(&mut bytes, call.arguments().canonical_bytes())?;
    append_bytes(&mut bytes, output.canonical_bytes())?;
    Ok(peritus_codec::sha256(&bytes))
}

fn append_bytes(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), DeveloperLoopError> {
    let length = u64::try_from(bytes.len())
        .map_err(|_| failure("scoped tool receipt identity exceeds its representation"))?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

fn failure(detail: &str) -> DeveloperLoopError {
    DeveloperLoopError::Trace(detail.to_owned())
}
