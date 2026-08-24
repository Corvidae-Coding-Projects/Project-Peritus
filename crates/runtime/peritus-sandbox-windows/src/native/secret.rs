//! Bounded protected-handle secret staging before target creation.

use core::ptr;

use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    Storage::FileSystem::ReadFile,
};
use zeroize::Zeroize;

use crate::{
    EnvironmentEntry, HelperManifest, SecretHandleDestination, WindowsError, WindowsErrorKind,
    WindowsOperation, WindowsRecovery,
};

const MAX_SECRET_BYTES: usize = 1_048_576;
const READ_CHUNK_BYTES: usize = 8_192;

pub(super) struct StagedSecrets {
    environment: Vec<EnvironmentEntry>,
}

impl StagedSecrets {
    pub(super) fn environment(&self) -> &[EnvironmentEntry] {
        &self.environment
    }
}

pub(super) fn stage(manifest: &HelperManifest) -> Result<StagedSecrets, WindowsError> {
    let mut environment = Vec::new();
    if let Some(route) = manifest.network().proxy() {
        let token = read_bounded(route.routing_handle() as HANDLE);
        // SAFETY: the route handle was verified and this helper owns its inherited copy.
        unsafe { CloseHandle(route.routing_handle() as HANDLE) };
        let mut token = token?;
        if token.len() != 32 {
            token.zeroize();
            return Err(network_error("managed proxy token has an invalid length"));
        }
        let password = hex(&token);
        token.zeroize();
        let proxy = format!("http://peritus:{password}@{}", route.endpoint());
        let http = EnvironmentEntry::new("HTTP_PROXY", &proxy)?;
        let https = EnvironmentEntry::new("HTTPS_PROXY", proxy)?;
        if manifest.environment().iter().any(|value| {
            value.name().eq_ignore_ascii_case(http.name())
                || value.name().eq_ignore_ascii_case(https.name())
        }) {
            return Err(network_error(
                "ordinary environment collides with managed proxy variables",
            ));
        }
        environment.extend([http, https]);
    }
    for descriptor in manifest.secret_handles() {
        if matches!(descriptor.destination(), SecretHandleDestination::Brokered(_)) {
            continue;
        }
        let bytes = read_bounded(descriptor.handle() as HANDLE);
        // This helper owns its inherited copy; environment/file sources must not reach the target.
        // SAFETY: the numeric value was verified as a valid inherited handle immediately before.
        unsafe { CloseHandle(descriptor.handle() as HANDLE) };
        let mut bytes = bytes?;
        match descriptor.destination() {
            SecretHandleDestination::Environment(name) => {
                let text = core::str::from_utf8(&bytes)
                    .map_err(|_| secret_error("environment secret is not valid UTF-8"))?;
                environment.push(EnvironmentEntry::new(name.as_str(), text)?);
            }
            SecretHandleDestination::File(path) => {
                let native = crate::WindowsPath::from_sandbox(manifest.working_directory(), path)?;
                std::fs::write(native.to_path_buf(), &bytes)
                    .map_err(|_| secret_error("private secret file cannot be written"))?;
            }
            SecretHandleDestination::Brokered(_) => {}
        }
        bytes.zeroize();
    }
    environment.sort();
    Ok(StagedSecrets { environment })
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    result
}

fn read_bounded(handle: HANDLE) -> Result<Vec<u8>, WindowsError> {
    let mut result = Vec::new();
    loop {
        if result.len() == MAX_SECRET_BYTES {
            return Err(secret_error("secret exceeds its protected delivery bound"));
        }
        let capacity = (MAX_SECRET_BYTES - result.len()).min(READ_CHUNK_BYTES);
        let start = result.len();
        result.resize(start + capacity, 0);
        let mut read = 0_u32;
        // SAFETY: destination covers exactly `capacity`; the synchronous handle needs no OVERLAPPED.
        let succeeded = unsafe {
            ReadFile(
                handle,
                result[start..].as_mut_ptr(),
                u32::try_from(capacity).unwrap_or(u32::MAX),
                &raw mut read,
                ptr::null_mut(),
            )
        };
        if succeeded == 0 {
            result.zeroize();
            return Err(secret_error("protected secret handle cannot be read to completion"));
        }
        let read =
            usize::try_from(read).map_err(|_| secret_error("secret read size overflowed"))?;
        result.truncate(start + read);
        if read == 0 {
            return Ok(result);
        }
    }
}

fn secret_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::Secret,
        WindowsOperation::Activate,
        WindowsRecovery::CancelAndReap,
        detail,
    )
}

fn network_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::Network,
        WindowsOperation::Activate,
        WindowsRecovery::CancelAndReap,
        detail,
    )
}
