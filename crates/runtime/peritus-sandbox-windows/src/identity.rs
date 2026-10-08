//! Deterministic nonsensitive helper, job, and token profile identities.

use peritus_types::{ProcessId, Sha256Digest};

use crate::{JobPlan, TokenProfile};

pub(crate) fn helper(digest: Sha256Digest) -> String {
    format!("{}:{}:{}", crate::BACKEND_NAME, crate::BACKEND_VERSION, short_hex(digest.as_bytes()))
}

pub(crate) fn job(
    process_id: ProcessId,
    preparation: Sha256Digest,
    plan: JobPlan,
) -> Sha256Digest {
    let mut bytes = Vec::from(b"PERITUS-WINDOWS-JOB-IDENTITY-V2\0".as_slice());
    bytes.extend_from_slice(process_id.as_bytes());
    bytes.extend_from_slice(preparation.as_bytes());
    bytes.push(u8::from(plan.kill_on_close()));
    bytes.push(u8::from(plan.active_process_limit_option().is_some()));
    bytes.extend_from_slice(&plan.active_process_limit().to_be_bytes());
    bytes.push(u8::from(plan.job_memory_bytes_option().is_some()));
    bytes.extend_from_slice(&plan.job_memory_bytes().to_be_bytes());
    bytes.push(u8::from(plan.cpu_time_millis_option().is_some()));
    bytes.extend_from_slice(&plan.cpu_time_millis().to_be_bytes());
    peritus_codec::sha256(&bytes)
}

pub(crate) fn job_name(process_id: ProcessId, identity: Sha256Digest) -> String {
    let mut name = String::with_capacity(18 + ProcessId::LENGTH * 2 + Sha256Digest::LENGTH * 2);
    name.push_str("Local\\PeritusJob-");
    append_hex(&mut name, process_id.as_bytes());
    name.push('-');
    append_hex(&mut name, identity.as_bytes());
    name
}

pub(crate) fn profile(profile: &TokenProfile) -> Sha256Digest {
    let mut bytes = Vec::from(b"PERITUS-WINDOWS-PROFILE-IDENTITY-V1\0".as_slice());
    bytes.extend_from_slice(profile.principal_sid().as_bytes());
    if let TokenProfile::AppContainer(value) = profile {
        bytes.extend_from_slice(value.name().as_bytes());
    }
    peritus_codec::sha256(&bytes)
}

fn short_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(16);
    for byte in bytes.iter().take(8) {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    result
}

fn append_hex(target: &mut String, bytes: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        target.push(char::from(HEX[usize::from(byte >> 4)]));
        target.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
}
