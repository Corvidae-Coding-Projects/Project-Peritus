//! Canonical manifest codec with complete-field checksum coverage.

use std::borrow::Cow;

use peritus_codec::{
    CanonicalReader, CanonicalWriter, CodecLimits, decode_frame, decode_frame_header, encode_frame,
};
use peritus_sandbox::{BrokeredHandleLabel, EnvironmentName, SandboxPath, SandboxResourceKind};
use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    AppContainerProfile, EnforcementLevel, EnvironmentEntry, HelperManifest, InheritedHandlePolicy,
    JobPlan, NetworkIsolation, ProcessPolicy, ProtectedSecretHandle, ProxyRoute, ResourceControl,
    ResourceControlPlan, SecretHandleDestination, TerminalMapping, TokenProfile, WindowsError,
    WindowsPath,
    manifest::expected_preparation,
    resource::{RESOURCE_KINDS, resource_from_ordinal, resource_ordinal},
};

mod scalars;

use scalars::{
    boolean, codec_error, collection, decode_socket, digest, encode_socket, fixed, protocol,
    encode_error, os_string, os_strings, read_digest, read_os_string, read_os_strings, read_strings,
    string, strings, u8_value, u16_value, u32_value, u64_value,
};

const FAMILY: u16 = 0xC307;
const LEGACY_SCHEMA: u16 = 1;
const NATIVE_SCHEMA: u16 = 2;
const PREVIOUS_SCHEMA: u16 = 3;
const PAGED_SCHEMA: u16 = 4;
pub(super) const SCHEMA: u16 = 5;
const CHECKSUM_BYTES: usize = Sha256Digest::LENGTH;
const COMPLETE_FRAME_BYTES: usize = peritus_process::NATIVE_MANIFEST_FRAME_BYTES;
const CODEC_FRAME_BYTES: usize = COMPLETE_FRAME_BYTES - CHECKSUM_BYTES;
const FRAME_PAYLOAD_BYTES: usize = CODEC_FRAME_BYTES - peritus_codec::HEADER_LEN;
const FRAME_LIMITS: CodecLimits = CodecLimits::new(
    CODEC_FRAME_BYTES,
    FRAME_PAYLOAD_BYTES,
    usize::MAX,
    usize::MAX,
    usize::MAX,
    CodecLimits::UNLIMITED_NESTING,
);
const LOGICAL_LIMITS: CodecLimits = CodecLimits::PRODUCTION;
const PAGE_METADATA_BYTES: usize = 8 + 1 + 8 + Sha256Digest::LENGTH;
const PAGE_CHUNK_BYTES: usize = FRAME_PAYLOAD_BYTES - PAGE_METADATA_BYTES;

pub(super) fn encode(manifest: &HelperManifest) -> Result<Vec<u8>, WindowsError> {
    match manifest.encoding_version {
        LEGACY_SCHEMA => {
            let payload = encode_legacy_payload(manifest)?;
            encode_single(LEGACY_SCHEMA, &payload)
        }
        NATIVE_SCHEMA | PREVIOUS_SCHEMA => {
            let payload = encode_native_payload(manifest)?;
            encode_single(manifest.encoding_version, &payload)
        }
        PAGED_SCHEMA | SCHEMA => {
            let payload = encode_native_payload(manifest)?;
            encode_paged(&payload, manifest.encoding_version)
        }
        _ => Err(protocol("manifest schema is unsupported")),
    }
}

fn encode_native_payload(manifest: &HelperManifest) -> Result<Vec<u8>, WindowsError> {
    let mut writer = CanonicalWriter::new(LOGICAL_LIMITS);
    fixed(&mut writer, manifest.process_id.as_bytes())?;
    digest(&mut writer, manifest.plan_digest)?;
    digest(&mut writer, manifest.descriptor_digest)?;
    digest(&mut writer, manifest.support_digest)?;
    digest(&mut writer, manifest.preparation_digest)?;
    digest(&mut writer, manifest.helper_digest)?;
    digest(&mut writer, manifest.acl_digest)?;
    encode_token(&mut writer, &manifest.token)?;
    os_string(&mut writer, &manifest.executable)?;
    os_strings(&mut writer, &manifest.arguments)?;
    os_string(&mut writer, manifest.working_directory.as_os_str())?;
    collection(&mut writer, manifest.environment.len())?;
    for entry in &manifest.environment {
        os_string(&mut writer, entry.name())?;
        os_string(&mut writer, entry.value())?;
    }
    encode_job(&mut writer, manifest.job, manifest.encoding_version)?;
    encode_process(&mut writer, manifest.process, manifest.encoding_version)?;
    encode_terminal(&mut writer, manifest.terminal)?;
    encode_resources(&mut writer, manifest.resources)?;
    encode_network(&mut writer, manifest.network)?;
    encode_secrets(&mut writer, &manifest.secret_handles, manifest.encoding_version)?;
    collection(&mut writer, manifest.inherited_handles.handles().len())?;
    for handle in manifest.inherited_handles.handles() {
        u64_value(&mut writer, *handle)?;
    }
    digest(&mut writer, manifest.inherited_handles.digest())?;
    Ok(writer.into_bytes())
}

fn encode_legacy_payload(manifest: &HelperManifest) -> Result<Vec<u8>, WindowsError> {
    let mut writer = CanonicalWriter::new(LOGICAL_LIMITS);
    fixed(&mut writer, manifest.process_id.as_bytes())?;
    digest(&mut writer, manifest.plan_digest)?;
    digest(&mut writer, manifest.descriptor_digest)?;
    digest(&mut writer, manifest.support_digest)?;
    digest(&mut writer, manifest.preparation_digest)?;
    digest(&mut writer, manifest.helper_digest)?;
    digest(&mut writer, manifest.acl_digest)?;
    encode_token(&mut writer, &manifest.token)?;
    string(
        &mut writer,
        manifest.executable.to_str().ok_or_else(|| protocol("legacy executable is not UTF-8"))?,
    )?;
    let arguments = manifest
        .arguments
        .iter()
        .map(|value| {
            value
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| protocol("legacy argument is not UTF-8"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    strings(&mut writer, &arguments)?;
    string(
        &mut writer,
        manifest
            .working_directory
            .as_os_str()
            .to_str()
            .ok_or_else(|| protocol("legacy working directory is not UTF-8"))?,
    )?;
    collection(&mut writer, manifest.environment.len())?;
    for entry in &manifest.environment {
        string(
            &mut writer,
            entry.name().to_str().ok_or_else(|| protocol("legacy environment name is not UTF-8"))?,
        )?;
        string(
            &mut writer,
            entry.value().to_str().ok_or_else(|| protocol("legacy environment value is not UTF-8"))?,
        )?;
    }
    encode_job(&mut writer, manifest.job, LEGACY_SCHEMA)?;
    encode_process(&mut writer, manifest.process, LEGACY_SCHEMA)?;
    encode_terminal(&mut writer, manifest.terminal)?;
    encode_resources(&mut writer, manifest.resources)?;
    encode_network(&mut writer, manifest.network)?;
    encode_secrets(&mut writer, &manifest.secret_handles, LEGACY_SCHEMA)?;
    collection(&mut writer, manifest.inherited_handles.handles().len())?;
    for handle in manifest.inherited_handles.handles() {
        u64_value(&mut writer, *handle)?;
    }
    digest(&mut writer, manifest.inherited_handles.digest())?;
    Ok(writer.into_bytes())
}

fn encode_single(schema: u16, payload: &[u8]) -> Result<Vec<u8>, WindowsError> {
    let mut frame = encode_frame(FAMILY, schema, payload, FRAME_LIMITS).map_err(encode_error)?;
    let checksum = peritus_codec::sha256(&frame);
    frame.extend_from_slice(checksum.as_bytes());
    Ok(frame)
}

fn encode_paged(payload: &[u8], schema: u16) -> Result<Vec<u8>, WindowsError> {
    if payload.is_empty() {
        return Err(protocol("manifest logical payload is empty"));
    }
    let total = u64::try_from(payload.len()).map_err(|_| {
        crate::error::invalid(
            crate::WindowsOperation::Manifest,
            "manifest logical length is not representable",
        )
    })?;
    let payload_digest = peritus_codec::sha256(payload);
    let page_count = payload.len().div_ceil(PAGE_CHUNK_BYTES);
    let mut encoded = Vec::new();
    for (index, chunk) in payload.chunks(PAGE_CHUNK_BYTES).enumerate() {
        let ordinal = u64::try_from(index).map_err(|_| {
            crate::error::invalid(
                crate::WindowsOperation::Manifest,
                "manifest page ordinal is not representable",
            )
        })?;
        let mut page_payload = Vec::new();
        page_payload.try_reserve_exact(PAGE_METADATA_BYTES + chunk.len()).map_err(|_| {
            crate::error::invalid(
                crate::WindowsOperation::Manifest,
                "manifest physical-page allocation is unavailable",
            )
        })?;
        page_payload.extend_from_slice(&ordinal.to_be_bytes());
        page_payload.push(u8::from(index + 1 == page_count));
        page_payload.extend_from_slice(&total.to_be_bytes());
        page_payload.extend_from_slice(payload_digest.as_bytes());
        page_payload.extend_from_slice(chunk);
        let mut frame =
            encode_frame(FAMILY, schema, &page_payload, FRAME_LIMITS).map_err(encode_error)?;
        let checksum = peritus_codec::sha256(&frame);
        frame.extend_from_slice(checksum.as_bytes());
        encoded.try_reserve(frame.len()).map_err(|_| {
            crate::error::invalid(
                crate::WindowsOperation::Manifest,
                "manifest page-stream allocation is unavailable",
            )
        })?;
        encoded.extend_from_slice(&frame);
    }
    Ok(encoded)
}

#[allow(clippy::too_many_lines, reason = "closed schema decode keeps every binding field visible")]
pub(super) fn decode(bytes: &[u8]) -> Result<HelperManifest, WindowsError> {
    if bytes.len() < peritus_codec::HEADER_LEN + CHECKSUM_BYTES {
        return Err(protocol("manifest size is invalid"));
    }
    let first = decode_frame_header(&bytes[..peritus_codec::HEADER_LEN], FRAME_LIMITS)
        .map_err(codec_error)?;
    let schema = first.schema_version();
    let payload = if matches!(schema, PAGED_SCHEMA | SCHEMA) {
        Cow::Owned(decode_paged(bytes, schema)?)
    } else {
        if bytes.len() > COMPLETE_FRAME_BYTES {
            return Err(protocol("manifest complete frame exceeds physical capacity"));
        }
        let checksum_at = bytes.len() - CHECKSUM_BYTES;
        let (frame_bytes, checksum) = bytes.split_at(checksum_at);
        if peritus_codec::sha256(frame_bytes).as_bytes() != checksum {
            return Err(protocol("manifest checksum does not match"));
        }
        let frame = decode_frame(frame_bytes, FRAME_LIMITS).map_err(codec_error)?;
        if frame.header().family() != FAMILY
            || !matches!(schema, LEGACY_SCHEMA | NATIVE_SCHEMA | PREVIOUS_SCHEMA)
        {
            return Err(protocol("manifest family or schema is unsupported"));
        }
        Cow::Borrowed(frame.payload())
    };
    let limits = if matches!(schema, PAGED_SCHEMA | SCHEMA) {
        LOGICAL_LIMITS
    } else {
        FRAME_LIMITS
    };
    let mut reader = CanonicalReader::new(&payload, limits);
    let process_id = ProcessId::new(reader.read_fixed().map_err(codec_error)?)
        .map_err(|_| protocol("manifest process identity is zero"))?;
    let plan_digest = read_digest(&mut reader)?;
    let descriptor_digest = read_digest(&mut reader)?;
    let support_digest = read_digest(&mut reader)?;
    let preparation_digest = read_digest(&mut reader)?;
    let helper_digest = read_digest(&mut reader)?;
    let acl_digest = read_digest(&mut reader)?;
    let token = decode_token(&mut reader)?;
    let executable = if schema == LEGACY_SCHEMA {
        reader.read_str().map_err(codec_error)?.to_owned().into()
    } else {
        read_os_string(&mut reader)?
    };
    let arguments = if schema == LEGACY_SCHEMA {
        read_strings(&mut reader)?.into_iter().map(Into::into).collect()
    } else {
        read_os_strings(&mut reader)?
    };
    let working_directory = if schema == LEGACY_SCHEMA {
        WindowsPath::new(reader.read_str().map_err(codec_error)?)?
    } else {
        WindowsPath::from_os_str(&read_os_string(&mut reader)?)?
    };
    let environment_count = reader.read_collection_len(2 * 4).map_err(codec_error)?;
    let mut environment = reader.reserve_collection(environment_count).map_err(codec_error)?;
    for _ in 0..environment_count {
        environment.push(if schema == LEGACY_SCHEMA {
            EnvironmentEntry::new_legacy(
                reader.read_str().map_err(codec_error)?.to_owned(),
                reader.read_str().map_err(codec_error)?.to_owned(),
            )?
        } else {
            EnvironmentEntry::new(read_os_string(&mut reader)?, read_os_string(&mut reader)?)?
        });
    }
    if environment.iter().enumerate().any(|(index, entry)| {
        environment[index + 1..]
            .iter()
            .any(|other| super::windows_name_cmp(entry.name(), other.name()).is_eq())
    }) {
        return Err(protocol("manifest environment contains a native name collision"));
    }
    if schema != LEGACY_SCHEMA
        && environment
            .windows(2)
            .any(|pair| !super::windows_name_cmp(pair[0].name(), pair[1].name()).is_lt())
    {
        return Err(protocol("manifest environment is duplicated or out of native order"));
    }
    let job = decode_job(&mut reader, schema)?;
    let process = decode_process(&mut reader, schema)?;
    let terminal = decode_terminal(&mut reader)?;
    let resources = decode_resources(&mut reader)?;
    let network = decode_network(&mut reader)?;
    let secret_handles = decode_secrets(&mut reader, schema)?;
    let handle_count = reader.read_collection_len(8).map_err(codec_error)?;
    let mut handles = reader.reserve_collection(handle_count).map_err(codec_error)?;
    for _ in 0..handle_count {
        handles.push(reader.read_u64().map_err(codec_error)?);
    }
    let inherited_handles = InheritedHandlePolicy::new(handles)?;
    if inherited_handles.digest() != read_digest(&mut reader)? {
        return Err(protocol("inherited handle digest differs from handle set"));
    }
    reader.finish().map_err(codec_error)?;
    if expected_preparation(plan_digest, descriptor_digest, support_digest) != preparation_digest {
        return Err(protocol("manifest preparation binding is invalid"));
    }
    let mut manifest = HelperManifest {
        encoding_version: schema,
        process_id,
        plan_digest,
        descriptor_digest,
        support_digest,
        preparation_digest,
        helper_digest,
        acl_digest,
        token,
        executable,
        arguments,
        working_directory,
        environment,
        job,
        process,
        terminal,
        resources,
        network,
        secret_handles,
        inherited_handles,
        canonical: bytes.to_vec(),
        canonical_page_ends: Vec::new(),
        digest: peritus_codec::sha256(bytes),
    };
    let reencoded = encode(&manifest)?;
    if reencoded != bytes {
        return Err(protocol("manifest is not in canonical field order"));
    }
    manifest.canonical = reencoded;
    manifest.canonical_page_ends = page_ends(&manifest.canonical)?;
    Ok(manifest)
}

pub(super) fn page_ends(bytes: &[u8]) -> Result<Vec<usize>, WindowsError> {
    if bytes.len() < peritus_codec::HEADER_LEN + CHECKSUM_BYTES {
        return Err(protocol("manifest size is invalid"));
    }
    let first = decode_frame_header(&bytes[..peritus_codec::HEADER_LEN], FRAME_LIMITS)
        .map_err(codec_error)?;
    if !matches!(first.schema_version(), PAGED_SCHEMA | SCHEMA) {
        if bytes.len() > COMPLETE_FRAME_BYTES {
            return Err(protocol("manifest complete frame exceeds physical capacity"));
        }
        return Ok(vec![bytes.len()]);
    }
    let mut ends = Vec::new();
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let end = physical_page_end(bytes, offset)?;
        ends.try_reserve(1).map_err(|_| {
            crate::error::invalid(
                crate::WindowsOperation::Manifest,
                "manifest page-index allocation is unavailable",
            )
        })?;
        ends.push(end);
        offset = end;
    }
    Ok(ends)
}

fn decode_paged(bytes: &[u8], schema: u16) -> Result<Vec<u8>, WindowsError> {
    let mut offset = 0_usize;
    let mut expected_ordinal = 0_u64;
    let mut declared_length = None;
    let mut declared_digest = None;
    let mut logical = Vec::new();
    let mut saw_final = false;
    while offset < bytes.len() {
        if saw_final {
            return Err(protocol("manifest page follows the final page"));
        }
        let end = physical_page_end(bytes, offset)?;
        let checksum_at = end - CHECKSUM_BYTES;
        let frame_bytes = &bytes[offset..checksum_at];
        let checksum = &bytes[checksum_at..end];
        if peritus_codec::sha256(frame_bytes).as_bytes() != checksum {
            return Err(protocol("manifest page checksum does not match"));
        }
        let frame = decode_frame(frame_bytes, FRAME_LIMITS).map_err(codec_error)?;
        if frame.header().family() != FAMILY || frame.header().schema_version() != schema {
            return Err(protocol("manifest page family or schema is unsupported"));
        }
        let page = frame.payload();
        if page.len() <= PAGE_METADATA_BYTES {
            return Err(protocol("manifest page has no logical payload"));
        }
        let ordinal = u64::from_be_bytes(page[..8].try_into().expect("fixed page ordinal"));
        if ordinal != expected_ordinal {
            return Err(protocol("manifest page ordinal is not contiguous"));
        }
        expected_ordinal = expected_ordinal
            .checked_add(1)
            .ok_or_else(|| protocol("manifest page ordinal overflowed"))?;
        let final_page = match page[8] {
            0 => false,
            1 => true,
            _ => return Err(protocol("manifest page final marker is invalid")),
        };
        let total = u64::from_be_bytes(page[9..17].try_into().expect("fixed logical length"));
        let total = usize::try_from(total)
            .map_err(|_| protocol("manifest logical length is not representable"))?;
        let digest = Sha256Digest::new(page[17..PAGE_METADATA_BYTES].try_into().expect("digest"));
        match (declared_length, declared_digest) {
            (None, None) => {
                if total == 0 {
                    return Err(protocol("manifest logical payload is empty"));
                }
                declared_length = Some(total);
                declared_digest = Some(digest);
            }
            (Some(length), Some(expected)) if length == total && expected == digest => {}
            _ => return Err(protocol("manifest page stream binding changed between pages")),
        }
        let chunk = &page[PAGE_METADATA_BYTES..];
        logical.try_reserve(chunk.len()).map_err(|_| {
            WindowsError::new(
                crate::WindowsErrorKind::HelperProtocol,
                crate::WindowsOperation::Manifest,
                crate::WindowsRecovery::RepairHelper,
                "manifest logical-payload allocation is unavailable",
            )
        })?;
        logical.extend_from_slice(chunk);
        if logical.len() > total {
            return Err(protocol("manifest page stream exceeds its declared logical length"));
        }
        offset = end;
        saw_final = final_page;
        if final_page && offset != bytes.len() {
            return Err(protocol("manifest final page has trailing physical pages"));
        }
    }
    let Some(total) = declared_length else {
        return Err(protocol("manifest page stream is empty"));
    };
    let Some(digest) = declared_digest else {
        return Err(protocol("manifest page stream lacks a digest"));
    };
    if !saw_final || logical.len() != total || peritus_codec::sha256(&logical) != digest {
        return Err(protocol("manifest logical page stream is incomplete or mismatched"));
    }
    Ok(logical)
}

fn physical_page_end(bytes: &[u8], offset: usize) -> Result<usize, WindowsError> {
    let header_end = offset
        .checked_add(peritus_codec::HEADER_LEN)
        .ok_or_else(|| protocol("manifest page header offset overflowed"))?;
    let header_bytes = bytes
        .get(offset..header_end)
        .ok_or_else(|| protocol("manifest page header is truncated"))?;
    let header = decode_frame_header(header_bytes, FRAME_LIMITS).map_err(codec_error)?;
    let frame_len = header.frame_len().map_err(codec_error)?;
    let complete_len = frame_len
        .checked_add(CHECKSUM_BYTES)
        .ok_or_else(|| protocol("manifest complete page length overflowed"))?;
    if complete_len > COMPLETE_FRAME_BYTES {
        return Err(protocol("manifest complete page exceeds physical capacity"));
    }
    let end = offset
        .checked_add(complete_len)
        .ok_or_else(|| protocol("manifest page end offset overflowed"))?;
    bytes
        .get(offset..end)
        .ok_or_else(|| protocol("manifest physical page is truncated"))?;
    Ok(end)
}

fn encode_token(writer: &mut CanonicalWriter, token: &TokenProfile) -> Result<(), WindowsError> {
    match token {
        TokenProfile::RestrictedLowIntegrity { principal_sid } => {
            u8_value(writer, 1)?;
            string(writer, principal_sid)
        }
        TokenProfile::AppContainer(profile) => {
            u8_value(writer, 2)?;
            string(writer, profile.name())?;
            string(writer, profile.sid())
        }
    }
}

fn decode_token(reader: &mut CanonicalReader<'_>) -> Result<TokenProfile, WindowsError> {
    match reader.read_u8().map_err(codec_error)? {
        1 => TokenProfile::restricted(reader.read_str().map_err(codec_error)?),
        2 => Ok(TokenProfile::AppContainer(AppContainerProfile::new(
            reader.read_str().map_err(codec_error)?,
            reader.read_str().map_err(codec_error)?,
        )?)),
        _ => Err(protocol("manifest has unknown token profile")),
    }
}

fn encode_job(
    writer: &mut CanonicalWriter,
    job: JobPlan,
    schema: u16,
) -> Result<(), WindowsError> {
    boolean(writer, job.kill_on_close())?;
    if schema <= NATIVE_SCHEMA {
        u32_value(writer, job.active_process_limit())?;
        u64_value(writer, job.job_memory_bytes())?;
        return u64_value(writer, job.cpu_time_millis());
    }
    encode_optional_u32(writer, job.active_process_limit_option())?;
    encode_optional_u64(writer, job.job_memory_bytes_option())?;
    encode_optional_u64(writer, job.cpu_time_millis_option())
}

fn decode_job(reader: &mut CanonicalReader<'_>, schema: u16) -> Result<JobPlan, WindowsError> {
    let kill_on_close = reader.read_bool().map_err(codec_error)?;
    if schema <= NATIVE_SCHEMA {
        return JobPlan::from_manifest(
            kill_on_close,
            reader.read_u32().map_err(codec_error)?,
            reader.read_u64().map_err(codec_error)?,
            reader.read_u64().map_err(codec_error)?,
        );
    }
    JobPlan::from_native_manifest(
        kill_on_close,
        decode_optional_u32(reader)?,
        decode_optional_u64(reader)?,
        decode_optional_u64(reader)?,
    )
}

fn encode_optional_u32(
    writer: &mut CanonicalWriter,
    value: Option<u32>,
) -> Result<(), WindowsError> {
    boolean(writer, value.is_some())?;
    if let Some(value) = value {
        u32_value(writer, value)?;
    }
    Ok(())
}

fn encode_optional_u64(
    writer: &mut CanonicalWriter,
    value: Option<u64>,
) -> Result<(), WindowsError> {
    boolean(writer, value.is_some())?;
    if let Some(value) = value {
        u64_value(writer, value)?;
    }
    Ok(())
}

fn decode_optional_u32(reader: &mut CanonicalReader<'_>) -> Result<Option<u32>, WindowsError> {
    if reader.read_bool().map_err(codec_error)? {
        Ok(Some(reader.read_u32().map_err(codec_error)?))
    } else {
        Ok(None)
    }
}

fn decode_optional_u64(reader: &mut CanonicalReader<'_>) -> Result<Option<u64>, WindowsError> {
    if reader.read_bool().map_err(codec_error)? {
        Ok(Some(reader.read_u64().map_err(codec_error)?))
    } else {
        Ok(None)
    }
}

fn encode_process(
    writer: &mut CanonicalWriter,
    policy: ProcessPolicy,
    schema: u16,
) -> Result<(), WindowsError> {
    if schema <= NATIVE_SCHEMA {
        if policy.unbounded_descendants() {
            return Err(protocol("legacy manifest cannot encode unbounded descendants"));
        }
        u32_value(writer, policy.descendant_limit())?;
    } else if policy.unbounded_descendants() {
        u8_value(writer, 3)?;
    } else if policy.descendant_limit() == 0 {
        u8_value(writer, 1)?;
    } else {
        u8_value(writer, 2)?;
        u32_value(writer, policy.descendant_limit())?;
    }
    boolean(writer, policy.graceful())?;
    boolean(writer, policy.forced())?;
    boolean(writer, policy.tree_required())
}

fn decode_process(
    reader: &mut CanonicalReader<'_>,
    schema: u16,
) -> Result<ProcessPolicy, WindowsError> {
    let (descendant_limit, unbounded_descendants) = if schema <= NATIVE_SCHEMA {
        (reader.read_u32().map_err(codec_error)?, false)
    } else {
        match reader.read_u8().map_err(codec_error)? {
            1 => (0, false),
            2 => {
                let limit = reader.read_u32().map_err(codec_error)?;
                if limit == 0 {
                    return Err(protocol("bounded descendant policy has a zero ceiling"));
                }
                (limit, false)
            }
            3 => (0, true),
            _ => return Err(protocol("manifest has unknown descendant policy")),
        }
    };
    ProcessPolicy::from_native_manifest(
        descendant_limit,
        unbounded_descendants,
        reader.read_bool().map_err(codec_error)?,
        reader.read_bool().map_err(codec_error)?,
        reader.read_bool().map_err(codec_error)?,
    )
}

fn encode_terminal(
    writer: &mut CanonicalWriter,
    value: TerminalMapping,
) -> Result<(), WindowsError> {
    match value {
        TerminalMapping::Pipes { input } => {
            u8_value(writer, 1)?;
            boolean(writer, input)
        }
        TerminalMapping::ConPty { columns, rows, resize, signals, input } => {
            u8_value(writer, 2)?;
            u16_value(writer, columns)?;
            u16_value(writer, rows)?;
            boolean(writer, resize)?;
            boolean(writer, signals)?;
            boolean(writer, input)
        }
    }
}

fn decode_terminal(reader: &mut CanonicalReader<'_>) -> Result<TerminalMapping, WindowsError> {
    match reader.read_u8().map_err(codec_error)? {
        1 => Ok(TerminalMapping::pipes(reader.read_bool().map_err(codec_error)?)),
        2 => TerminalMapping::conpty(
            reader.read_u16().map_err(codec_error)?,
            reader.read_u16().map_err(codec_error)?,
            reader.read_bool().map_err(codec_error)?,
            reader.read_bool().map_err(codec_error)?,
            reader.read_bool().map_err(codec_error)?,
        ),
        _ => Err(protocol("manifest has unknown terminal mapping")),
    }
}

fn encode_resources(
    writer: &mut CanonicalWriter,
    plan: ResourceControlPlan,
) -> Result<(), WindowsError> {
    collection(writer, RESOURCE_KINDS.len())?;
    for control in plan.controls() {
        u8_value(writer, resource_ordinal(control.kind()))?;
        u64_value(writer, control.ceiling())?;
        u8_value(writer, control.level().ordinal())?;
    }
    Ok(())
}

fn decode_resources(reader: &mut CanonicalReader<'_>) -> Result<ResourceControlPlan, WindowsError> {
    if reader.read_collection_len(1 + 8 + 1).map_err(codec_error)? != RESOURCE_KINDS.len() {
        return Err(protocol("manifest resource mapping is not complete"));
    }
    let mut controls =
        [ResourceControl::new(SandboxResourceKind::WallTime, 1, EnforcementLevel::Unsupported); 8];
    for (index, expected) in RESOURCE_KINDS.into_iter().enumerate() {
        let kind = resource_from_ordinal(reader.read_u8().map_err(codec_error)?)
            .ok_or_else(|| protocol("manifest has unknown resource dimension"))?;
        if kind != expected {
            return Err(protocol("manifest resource dimensions are out of order"));
        }
        let ceiling = reader.read_u64().map_err(codec_error)?;
        let level = EnforcementLevel::from_ordinal(reader.read_u8().map_err(codec_error)?)
            .ok_or_else(|| protocol("manifest has unknown enforcement level"))?;
        controls[index] = ResourceControl::new(kind, ceiling, level);
    }
    let plan = ResourceControlPlan::from_controls(controls);
    if !plan.is_complete() {
        return Err(protocol("manifest contains unsupported resource enforcement"));
    }
    Ok(plan)
}

fn encode_network(
    writer: &mut CanonicalWriter,
    value: NetworkIsolation,
) -> Result<(), WindowsError> {
    match value {
        NetworkIsolation::DenyAll => u8_value(writer, 1),
        NetworkIsolation::ManagedProxy(route) => {
            u8_value(writer, 2)?;
            encode_socket(writer, route.endpoint())?;
            u64_value(writer, route.routing_handle())?;
            digest(writer, route.network_plan_digest())?;
            digest(writer, route.filter_digest())
        }
    }
}

fn decode_network(reader: &mut CanonicalReader<'_>) -> Result<NetworkIsolation, WindowsError> {
    match reader.read_u8().map_err(codec_error)? {
        1 => Ok(NetworkIsolation::DenyAll),
        2 => Ok(NetworkIsolation::ManagedProxy(ProxyRoute::new(
            decode_socket(reader)?,
            reader.read_u64().map_err(codec_error)?,
            read_digest(reader)?,
            read_digest(reader)?,
        )?)),
        _ => Err(protocol("manifest has unknown network isolation")),
    }
}

fn encode_secrets(
    writer: &mut CanonicalWriter,
    values: &[ProtectedSecretHandle],
    schema: u16,
) -> Result<(), WindowsError> {
    collection(writer, values.len())?;
    for value in values {
        u64_value(writer, value.handle())?;
        digest(writer, value.reference_digest())?;
        match value.destination() {
            SecretHandleDestination::Environment(name) => {
                u8_value(writer, 1)?;
                string(writer, name.as_str())?;
            }
            SecretHandleDestination::File(path) => {
                u8_value(writer, 2)?;
                string(writer, path.as_str())?;
            }
            SecretHandleDestination::Brokered(label) => {
                u8_value(writer, 3)?;
                string(writer, label.as_str())?;
            }
        }
        if schema >= SCHEMA {
            boolean(writer, value.payload_len().is_some())?;
            if let Some(payload_len) = value.payload_len() {
                u64_value(writer, payload_len)?;
            }
        }
    }
    Ok(())
}

fn decode_secrets(
    reader: &mut CanonicalReader<'_>,
    schema: u16,
) -> Result<Vec<ProtectedSecretHandle>, WindowsError> {
    let count = reader.read_collection_len(8 + 32 + 1 + 4).map_err(codec_error)?;
    let mut values = reader.reserve_collection(count).map_err(codec_error)?;
    for _ in 0..count {
        let handle = reader.read_u64().map_err(codec_error)?;
        let reference = read_digest(reader)?;
        let destination = match reader.read_u8().map_err(codec_error)? {
            1 => SecretHandleDestination::Environment(
                EnvironmentName::new(reader.read_str().map_err(codec_error)?)
                    .map_err(|_| protocol("manifest secret environment name is invalid"))?,
            ),
            2 => SecretHandleDestination::File(
                SandboxPath::new(reader.read_str().map_err(codec_error)?)
                    .map_err(|_| protocol("manifest secret file path is invalid"))?,
            ),
            3 => SecretHandleDestination::Brokered(
                BrokeredHandleLabel::new(reader.read_str().map_err(codec_error)?)
                    .map_err(|_| protocol("manifest brokered handle label is invalid"))?,
            ),
            _ => return Err(protocol("manifest has unknown secret destination")),
        };
        let payload_len = if schema >= SCHEMA && reader.read_bool().map_err(codec_error)? {
            Some(reader.read_u64().map_err(codec_error)?)
        } else {
            None
        };
        values.push(match payload_len {
            Some(payload_len) => {
                ProtectedSecretHandle::new_bound(handle, reference, destination, payload_len)?
            }
            None => ProtectedSecretHandle::new(handle, reference, destination)?,
        });
    }
    crate::canonical_handles(values)
}
