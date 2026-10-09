//! Canonical manifest codec with complete-field checksum coverage.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits, decode_frame, encode_frame};
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
    read_digest, read_strings, string, strings, u8_value, u16_value, u32_value, u64_value,
};

const FAMILY: u16 = 0xC307;
const SCHEMA: u16 = 1;
const CHECKSUM_BYTES: usize = Sha256Digest::LENGTH;
const LIMITS: CodecLimits = CodecLimits::new(
    4 * 1_024 * 1_024,
    4 * 1_024 * 1_024 - peritus_codec::HEADER_LEN,
    4_096,
    1_048_576,
    1_048_576,
    16,
);

pub(super) fn encode(manifest: &HelperManifest) -> Result<Vec<u8>, WindowsError> {
    let mut writer = CanonicalWriter::new(LIMITS);
    fixed(&mut writer, manifest.process_id.as_bytes())?;
    digest(&mut writer, manifest.plan_digest)?;
    digest(&mut writer, manifest.descriptor_digest)?;
    digest(&mut writer, manifest.support_digest)?;
    digest(&mut writer, manifest.preparation_digest)?;
    digest(&mut writer, manifest.helper_digest)?;
    digest(&mut writer, manifest.acl_digest)?;
    encode_token(&mut writer, &manifest.token)?;
    string(&mut writer, &manifest.executable)?;
    strings(&mut writer, &manifest.arguments)?;
    string(&mut writer, manifest.working_directory.as_str())?;
    collection(&mut writer, manifest.environment.len())?;
    for entry in &manifest.environment {
        string(&mut writer, entry.name())?;
        string(&mut writer, entry.value())?;
    }
    encode_job(&mut writer, manifest.job)?;
    encode_process(&mut writer, manifest.process)?;
    encode_terminal(&mut writer, manifest.terminal)?;
    encode_resources(&mut writer, manifest.resources)?;
    encode_network(&mut writer, manifest.network)?;
    encode_secrets(&mut writer, &manifest.secret_handles)?;
    collection(&mut writer, manifest.inherited_handles.handles().len())?;
    for handle in manifest.inherited_handles.handles() {
        u64_value(&mut writer, *handle)?;
    }
    digest(&mut writer, manifest.inherited_handles.digest())?;
    let mut frame =
        encode_frame(FAMILY, SCHEMA, &writer.into_bytes(), LIMITS).map_err(codec_error)?;
    let checksum = peritus_codec::sha256(&frame);
    frame.extend_from_slice(checksum.as_bytes());
    Ok(frame)
}

#[allow(clippy::too_many_lines, reason = "closed schema decode keeps every binding field visible")]
pub(super) fn decode(bytes: &[u8]) -> Result<HelperManifest, WindowsError> {
    if bytes.len() <= CHECKSUM_BYTES || bytes.len() > LIMITS.max_frame_bytes + CHECKSUM_BYTES {
        return Err(protocol("manifest size is invalid"));
    }
    let checksum_at = bytes.len() - CHECKSUM_BYTES;
    let (frame_bytes, checksum) = bytes.split_at(checksum_at);
    if peritus_codec::sha256(frame_bytes).as_bytes() != checksum {
        return Err(protocol("manifest checksum does not match"));
    }
    let frame = decode_frame(frame_bytes, LIMITS).map_err(codec_error)?;
    if frame.header().family() != FAMILY || frame.header().schema_version() != SCHEMA {
        return Err(protocol("manifest family or schema is unsupported"));
    }
    let mut reader = CanonicalReader::new(frame.payload(), LIMITS);
    let process_id = ProcessId::new(reader.read_fixed().map_err(codec_error)?)
        .map_err(|_| protocol("manifest process identity is zero"))?;
    let plan_digest = read_digest(&mut reader)?;
    let descriptor_digest = read_digest(&mut reader)?;
    let support_digest = read_digest(&mut reader)?;
    let preparation_digest = read_digest(&mut reader)?;
    let helper_digest = read_digest(&mut reader)?;
    let acl_digest = read_digest(&mut reader)?;
    let token = decode_token(&mut reader)?;
    let executable = reader.read_str().map_err(codec_error)?.to_owned();
    let arguments = read_strings(&mut reader)?;
    let working_directory = WindowsPath::new(reader.read_str().map_err(codec_error)?)?;
    let environment_count = reader.read_collection_len().map_err(codec_error)?;
    let mut environment = Vec::with_capacity(environment_count);
    for _ in 0..environment_count {
        environment.push(EnvironmentEntry::new(
            reader.read_str().map_err(codec_error)?,
            reader.read_str().map_err(codec_error)?,
        )?);
    }
    let job = decode_job(&mut reader)?;
    let process = decode_process(&mut reader)?;
    let terminal = decode_terminal(&mut reader)?;
    let resources = decode_resources(&mut reader)?;
    let network = decode_network(&mut reader)?;
    let secret_handles = decode_secrets(&mut reader)?;
    let handle_count = reader.read_collection_len().map_err(codec_error)?;
    let mut handles = Vec::with_capacity(handle_count);
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
        digest: peritus_codec::sha256(bytes),
    };
    let reencoded = encode(&manifest)?;
    if reencoded != bytes {
        return Err(protocol("manifest is not in canonical field order"));
    }
    manifest.canonical = reencoded;
    Ok(manifest)
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

fn encode_job(writer: &mut CanonicalWriter, job: JobPlan) -> Result<(), WindowsError> {
    boolean(writer, job.kill_on_close())?;
    u32_value(writer, job.active_process_limit())?;
    u64_value(writer, job.job_memory_bytes())?;
    u64_value(writer, job.cpu_time_millis())
}

fn decode_job(reader: &mut CanonicalReader<'_>) -> Result<JobPlan, WindowsError> {
    JobPlan::from_manifest(
        reader.read_bool().map_err(codec_error)?,
        reader.read_u32().map_err(codec_error)?,
        reader.read_u64().map_err(codec_error)?,
        reader.read_u64().map_err(codec_error)?,
    )
}

fn encode_process(writer: &mut CanonicalWriter, policy: ProcessPolicy) -> Result<(), WindowsError> {
    u32_value(writer, policy.descendant_limit())?;
    boolean(writer, policy.graceful())?;
    boolean(writer, policy.forced())?;
    boolean(writer, policy.tree_required())
}

fn decode_process(reader: &mut CanonicalReader<'_>) -> Result<ProcessPolicy, WindowsError> {
    ProcessPolicy::from_manifest(
        reader.read_u32().map_err(codec_error)?,
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
    if reader.read_collection_len().map_err(codec_error)? != RESOURCE_KINDS.len() {
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
    }
    Ok(())
}

fn decode_secrets(
    reader: &mut CanonicalReader<'_>,
) -> Result<Vec<ProtectedSecretHandle>, WindowsError> {
    let count = reader.read_collection_len().map_err(codec_error)?;
    let mut values = Vec::with_capacity(count);
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
        values.push(ProtectedSecretHandle::new(handle, reference, destination)?);
    }
    crate::canonical_handles(values)
}
