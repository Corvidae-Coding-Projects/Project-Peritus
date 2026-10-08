//! Lossless retained-owner factory identity for the Windows backend.

use std::path::{Path, PathBuf};

use peritus_process::{
    ErrorCode, ExecutionPlan, NativePlatform, ProcessError, ProcessOperation, RecoveryClass,
    RetainedBackendFactoryRequest,
};
use peritus_sandbox::{BackendAdmission, CheckedSandboxPlan};
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use super::WindowsBackend;
use crate::{TokenProfile, WindowsBackendConfig};

const FACTORY_VERSION_V1: u16 = 1;
const FACTORY_VERSION_V2: u16 = 2;
const PAYLOAD_TAG_V1: &[u8; 8] = b"WINFACT1";
const PAYLOAD_TAG_V2: &[u8; 8] = b"WINFACT2";
const PAYLOAD_PREFIX_BYTES: usize = PAYLOAD_TAG_V2.len() + 5 * 32;
const CONFIG_DOMAIN_V1: &[u8] = b"peritus.windows.retained-backend-config.v1\0";
const CONFIG_DOMAIN_V2: &[u8] = b"peritus.windows.retained-backend-config.v2\0";

#[derive(Clone, Debug, Eq, PartialEq)]
struct RetainedManagedNetwork {
    canonical: Vec<u8>,
    digest: Sha256Digest,
    command_state_root: PathBuf,
}

/// Authenticated Windows factory identity decoded before any host probe or target effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedWindowsBackendFactory {
    version: u16,
    sandbox_digest: Sha256Digest,
    descriptor_digest: Sha256Digest,
    support_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
    config_digest: Sha256Digest,
    acl_backup_root: PathBuf,
    managed_network: Option<RetainedManagedNetwork>,
}

impl RetainedWindowsBackendFactory {
    /// Decodes an exact platform-owned version-one or version-two payload.
    ///
    /// # Errors
    /// Rejects another platform, version, framing, or payload tag before probing the host.
    pub fn decode(request: &RetainedBackendFactoryRequest) -> Result<Self, ProcessError> {
        if request.platform() != NativePlatform::Windows {
            return Err(mismatch("retained Windows backend factory payload is not canonical"));
        }
        let payload = request.payload();
        let (acl_backup_root, managed_network) = match request.version() {
            FACTORY_VERSION_V1
                if payload.len() >= PAYLOAD_PREFIX_BYTES + 4
                    && payload.get(..PAYLOAD_TAG_V1.len())
                        == Some(PAYLOAD_TAG_V1.as_slice()) => {
                    (decode_path(&payload[PAYLOAD_PREFIX_BYTES..])?, None)
                }
            FACTORY_VERSION_V2
                if payload.len() > PAYLOAD_PREFIX_BYTES + 4
                    && payload.get(..PAYLOAD_TAG_V2.len())
                        == Some(PAYLOAD_TAG_V2.as_slice()) => {
                    let tail = &payload[PAYLOAD_PREFIX_BYTES..];
                    let (acl_backup_root, consumed) = decode_path_prefix(tail)?;
                    let managed_network = decode_managed_network(
                        tail.get(consumed..).ok_or_else(|| {
                            mismatch("retained Windows managed-network framing is missing")
                        })?,
                    )?;
                    (acl_backup_root, managed_network)
                }
            _ => {
                return Err(mismatch(
                    "retained Windows backend factory payload is not canonical",
                ));
            }
        };
        Ok(Self {
            version: request.version(),
            sandbox_digest: digest_at(payload, 8),
            descriptor_digest: digest_at(payload, 40),
            support_digest: digest_at(payload, 72),
            preparation_digest: digest_at(payload, 104),
            config_digest: digest_at(payload, 136),
            acl_backup_root,
            managed_network,
        })
    }

    /// Reconstructs and reprobes the exact backend configuration selected before owner transfer.
    ///
    /// # Errors
    /// Rejects plan, trusted configuration, installation, or probe drift without preparing or
    /// launching the target.
    pub fn reconstruct(
        self,
        config: WindowsBackendConfig,
        execution: &ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
        should_continue: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Result<WindowsBackend, ProcessError> {
        self.validate_plan(execution, sandbox)?;
        if self.config_digest(&config)? != self.config_digest {
            return Err(unavailable(
                "current trusted Windows backend configuration differs from retained identity",
            ));
        }
        let backend = WindowsBackend::new_cancellable(config, should_continue).map_err(|_| {
            unavailable("retained Windows backend installation cannot be reconstructed")
        })?;
        if backend.descriptor().digest() != self.descriptor_digest
            || backend.descriptor().support_digest() != self.support_digest
        {
            return Err(unavailable(
                "reprobed Windows backend descriptor differs from retained admission",
            ));
        }
        Ok(backend)
    }

    /// Returns the exact nonsensitive ACL-backup configuration reference retained for restart.
    #[must_use]
    pub fn acl_backup_root(&self) -> &Path {
        &self.acl_backup_root
    }

    /// Returns the exact nonsensitive semantic grant and command-state root retained for restart.
    #[must_use]
    pub fn managed_network(&self) -> Option<(&[u8], Sha256Digest, &Path)> {
        self.managed_network.as_ref().map(|network| {
            (
                network.canonical.as_slice(),
                network.digest,
                network.command_state_root.as_path(),
            )
        })
    }

    /// Reports whether one freshly derived trusted configuration is the retained configuration.
    ///
    /// # Errors
    /// Returns unavailable when the candidate's protected resources are incomplete or cannot be
    /// represented by this retained factory version.
    pub fn matches_config(&self, config: &WindowsBackendConfig) -> Result<bool, ProcessError> {
        Ok(self.config_digest(config)? == self.config_digest)
    }

    fn config_digest(
        &self,
        config: &WindowsBackendConfig,
    ) -> Result<Sha256Digest, ProcessError> {
        match self.version {
            FACTORY_VERSION_V1 => config_digest_v1(config),
            FACTORY_VERSION_V2 => config_digest_v2(config, self.managed_network.is_some()),
            _ => Err(mismatch("retained Windows backend factory version is invalid")),
        }
    }

    fn validate_plan(
        &self,
        execution: &ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
    ) -> Result<(), ProcessError> {
        let selected = execution.backend();
        if execution.sandbox_digest() != sandbox.digest()
            || sandbox.digest() != self.sandbox_digest
            || selected.descriptor_digest() != self.descriptor_digest
            || selected.support_digest() != self.support_digest
            || selected.preparation_digest() != self.preparation_digest
        {
            return Err(mismatch(
                "retained Windows backend identity differs from decoded execution plans",
            ));
        }
        Ok(())
    }
}

pub(super) fn request(
    backend: &WindowsBackend,
    sandbox: &CheckedSandboxPlan,
    admission: &BackendAdmission,
) -> Result<RetainedBackendFactoryRequest, ProcessError> {
    if !sandbox.requirements().secrets().is_empty() {
        return Err(unavailable(
            "retained Windows secret grants cannot be reconstructed",
        ));
    }
    let descriptor = backend.descriptor();
    if admission.plan_digest() != sandbox.digest()
        || admission.descriptor() != descriptor
        || admission.descriptor_digest() != descriptor.digest()
        || admission.support_digest() != descriptor.support_digest()
    {
        return Err(mismatch(
            "Windows backend, sandbox, and admission differ before owner transfer",
        ));
    }
    let managed_network_selected = !sandbox.requirements().network().is_empty();
    let managed_network =
        managed_network_identity(&backend.config, managed_network_selected)?;
    let mut payload = Vec::with_capacity(PAYLOAD_PREFIX_BYTES + 5);
    payload.extend_from_slice(PAYLOAD_TAG_V2);
    for digest in [
        sandbox.digest(),
        admission.descriptor_digest(),
        admission.support_digest(),
        admission.preparation_digest(),
        config_digest_v2(&backend.config, managed_network_selected)?,
    ] {
        payload.extend_from_slice(digest.as_bytes());
    }
    encode_path(&mut payload, &backend.config.acl_backup_root)?;
    encode_managed_network(&mut payload, managed_network)?;
    RetainedBackendFactoryRequest::new(NativePlatform::Windows, FACTORY_VERSION_V2, payload)
}

fn config_digest_v1(config: &WindowsBackendConfig) -> Result<Sha256Digest, ProcessError> {
    let mut hash = Sha256::new();
    hash.update(CONFIG_DOMAIN_V1);
    hash_config_base(&mut hash, config)?;
    Ok(Sha256Digest::new(hash.finalize().into()))
}

fn config_digest_v2(
    config: &WindowsBackendConfig,
    managed_network_selected: bool,
) -> Result<Sha256Digest, ProcessError> {
    let managed_network = managed_network_identity(config, managed_network_selected)?;
    let mut hash = Sha256::new();
    hash.update(CONFIG_DOMAIN_V2);
    hash_config_base(&mut hash, config)?;
    hash_length(&mut hash, config.writable_inputs.len())?;
    for path in &config.writable_inputs {
        hash.update(path.digest().as_bytes());
    }
    match config.managed_filter_digest.filter(|_| managed_network_selected) {
        Some(digest) => {
            hash.update([1]);
            hash.update(digest.as_bytes());
        }
        None => hash.update([0]),
    }
    hash_managed_network(&mut hash, managed_network)?;
    Ok(Sha256Digest::new(hash.finalize().into()))
}

fn hash_config_base(
    hash: &mut Sha256,
    config: &WindowsBackendConfig,
) -> Result<(), ProcessError> {
    hash_path(hash, &config.helper_path)?;
    hash.update(config.workspace.digest().as_bytes());
    hash_length(hash, config.protected_roots.len())?;
    for path in &config.protected_roots {
        hash.update(path.digest().as_bytes());
    }
    hash_length(hash, config.read_only_inputs.len())?;
    for path in &config.read_only_inputs {
        hash.update(path.digest().as_bytes());
    }
    hash_path(hash, &config.acl_backup_root)?;
    match &config.token {
        TokenProfile::RestrictedLowIntegrity { principal_sid } => {
            hash.update([1]);
            hash_text(hash, principal_sid)?;
        }
        TokenProfile::AppContainer(profile) => {
            hash.update([2]);
            hash_text(hash, profile.name())?;
            hash_text(hash, profile.sid())?;
        }
    }
    Ok(())
}

type ManagedNetworkRef<'a> = (&'a [u8], Sha256Digest, &'a Path);

fn managed_network_identity(
    config: &WindowsBackendConfig,
    selected: bool,
) -> Result<Option<ManagedNetworkRef<'_>>, ProcessError> {
    if !selected {
        return Ok(None);
    }
    match (
        config.managed_network_grant(),
        config.managed_network_cache_root(),
        config.proxy.is_some(),
        config.managed_filter_digest,
    ) {
        (None, None, false, None) => Ok(None),
        (
            Some((canonical, grant_digest)),
            Some((root, cache_digest)),
            true,
            Some(_),
        ) if grant_digest == cache_digest
            && peritus_codec::sha256(canonical) == grant_digest
            && root.is_absolute()
            && root.is_dir()
            && std::fs::canonicalize(root).ok().as_deref() == Some(root) =>
        {
            Ok(Some((canonical, grant_digest, root)))
        }
        _ => Err(unavailable(
            "retained Windows managed-network identity is incomplete or inconsistent",
        )),
    }
}

fn encode_managed_network(
    payload: &mut Vec<u8>,
    managed_network: Option<ManagedNetworkRef<'_>>,
) -> Result<(), ProcessError> {
    let Some((canonical, digest, command_state_root)) = managed_network else {
        payload.push(0);
        return Ok(());
    };
    let length = u32::try_from(canonical.len())
        .map_err(|_| mismatch("retained Windows managed-network grant exceeds framing"))?;
    payload.push(1);
    payload.extend_from_slice(digest.as_bytes());
    payload.extend_from_slice(&length.to_be_bytes());
    payload.extend_from_slice(canonical);
    encode_path(payload, command_state_root)
}

fn decode_managed_network(bytes: &[u8]) -> Result<Option<RetainedManagedNetwork>, ProcessError> {
    match bytes.first().copied() {
        Some(0) if bytes.len() == 1 => Ok(None),
        Some(1) => {
            let digest_end = 33_usize;
            let length_end = digest_end
                .checked_add(4)
                .ok_or_else(|| mismatch("retained Windows managed-network framing overflows"))?;
            let digest_bytes: [u8; 32] = bytes
                .get(1..digest_end)
                .and_then(|value| value.try_into().ok())
                .ok_or_else(|| mismatch("retained Windows managed-network digest is truncated"))?;
            let length_bytes: [u8; 4] = bytes
                .get(digest_end..length_end)
                .and_then(|value| value.try_into().ok())
                .ok_or_else(|| mismatch("retained Windows managed-network length is truncated"))?;
            let length = usize::try_from(u32::from_be_bytes(length_bytes)).map_err(|_| {
                mismatch("retained Windows managed-network length is not representable")
            })?;
            let canonical_end = length_end
                .checked_add(length)
                .ok_or_else(|| mismatch("retained Windows managed-network length overflows"))?;
            let canonical = bytes
                .get(length_end..canonical_end)
                .ok_or_else(|| mismatch("retained Windows managed-network grant is truncated"))?
                .to_vec();
            let digest = Sha256Digest::new(digest_bytes);
            if canonical.is_empty() || peritus_codec::sha256(&canonical) != digest {
                return Err(mismatch(
                    "retained Windows managed-network grant identity is invalid",
                ));
            }
            let command_state_root = decode_path(
                bytes
                    .get(canonical_end..)
                    .ok_or_else(|| mismatch("retained Windows command-state root is missing"))?,
            )?;
            validate_factory_path(&command_state_root)?;
            Ok(Some(RetainedManagedNetwork {
                canonical,
                digest,
                command_state_root,
            }))
        }
        _ => Err(mismatch(
            "retained Windows managed-network framing is not canonical",
        )),
    }
}

fn hash_managed_network(
    hash: &mut Sha256,
    managed_network: Option<ManagedNetworkRef<'_>>,
) -> Result<(), ProcessError> {
    let Some((canonical, digest, command_state_root)) = managed_network else {
        hash.update([0]);
        return Ok(());
    };
    hash.update([1]);
    hash.update(digest.as_bytes());
    hash_length(hash, canonical.len())?;
    hash.update(canonical);
    hash_path(hash, command_state_root)
}

fn hash_text(hash: &mut Sha256, value: &str) -> Result<(), ProcessError> {
    hash_length(hash, value.len())?;
    hash.update(value.as_bytes());
    Ok(())
}

fn hash_length(hash: &mut Sha256, length: usize) -> Result<(), ProcessError> {
    let length = u64::try_from(length)
        .map_err(|_| mismatch("retained Windows backend configuration length is invalid"))?;
    hash.update(length.to_be_bytes());
    Ok(())
}

#[cfg(windows)]
fn encode_path(payload: &mut Vec<u8>, path: &Path) -> Result<(), ProcessError> {
    use std::os::windows::ffi::OsStrExt as _;

    let units = path.as_os_str().encode_wide().collect::<Vec<_>>();
    let count = u32::try_from(units.len())
        .map_err(|_| mismatch("Windows factory path exceeds canonical framing"))?;
    payload.extend_from_slice(&count.to_be_bytes());
    for unit in units {
        payload.extend_from_slice(&unit.to_be_bytes());
    }
    Ok(())
}

#[cfg(not(windows))]
fn encode_path(payload: &mut Vec<u8>, path: &Path) -> Result<(), ProcessError> {
    let text = path
        .to_str()
        .ok_or_else(|| mismatch("Windows factory path is not losslessly representable"))?;
    let units = text.encode_utf16().collect::<Vec<_>>();
    let count = u32::try_from(units.len())
        .map_err(|_| mismatch("Windows factory path exceeds canonical framing"))?;
    payload.extend_from_slice(&count.to_be_bytes());
    for unit in units {
        payload.extend_from_slice(&unit.to_be_bytes());
    }
    Ok(())
}

fn decode_path(bytes: &[u8]) -> Result<PathBuf, ProcessError> {
    let (path, consumed) = decode_path_prefix(bytes)?;
    if consumed != bytes.len() {
        return Err(mismatch("Windows factory path framing is not canonical"));
    }
    Ok(path)
}

fn decode_path_prefix(bytes: &[u8]) -> Result<(PathBuf, usize), ProcessError> {
    let count = bytes
        .get(..4)
        .and_then(|value| <[u8; 4]>::try_from(value).ok())
        .map(u32::from_be_bytes)
        .ok_or_else(|| mismatch("Windows factory path framing is truncated"))?;
    let count = usize::try_from(count)
        .map_err(|_| mismatch("Windows factory path length is not representable"))?;
    let expected = count
        .checked_mul(2)
        .and_then(|value| value.checked_add(4))
        .ok_or_else(|| mismatch("Windows factory path length overflows"))?;
    if bytes.len() < expected || count == 0 {
        return Err(mismatch("Windows factory path framing is not canonical"));
    }
    let units = bytes[4..expected]
        .chunks_exact(2)
        .map(|unit| u16::from_be_bytes([unit[0], unit[1]]))
        .collect::<Vec<_>>();
    decode_path_units(&units).map(|path| (path, expected))
}

fn validate_factory_path(path: &Path) -> Result<(), ProcessError> {
    if !path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
    {
        return Err(mismatch(
            "retained Windows command-state root is not an exact absolute path",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn decode_path_units(units: &[u16]) -> Result<PathBuf, ProcessError> {
    use std::os::windows::ffi::OsStringExt as _;

    Ok(PathBuf::from(std::ffi::OsString::from_wide(units)))
}

#[cfg(not(windows))]
fn decode_path_units(units: &[u16]) -> Result<PathBuf, ProcessError> {
    String::from_utf16(units)
        .map(PathBuf::from)
        .map_err(|_| mismatch("Windows factory path is invalid UTF-16 on this build target"))
}

#[cfg(windows)]
fn hash_path(hash: &mut Sha256, path: &Path) -> Result<(), ProcessError> {
    use std::os::windows::ffi::OsStrExt as _;

    let units = path.as_os_str().encode_wide().collect::<Vec<_>>();
    hash_length(hash, units.len())?;
    for unit in units {
        hash.update(unit.to_be_bytes());
    }
    Ok(())
}

#[cfg(not(windows))]
fn hash_path(hash: &mut Sha256, path: &Path) -> Result<(), ProcessError> {
    let bytes = path
        .to_str()
        .ok_or_else(|| mismatch("Windows configuration path is not losslessly representable"))?
        .as_bytes();
    hash_length(hash, bytes.len())?;
    hash.update(bytes);
    Ok(())
}

fn digest_at(payload: &[u8], offset: usize) -> Sha256Digest {
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(&payload[offset..offset + 32]);
    Sha256Digest::new(bytes)
}

const fn mismatch(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::PlanMismatch,
        ProcessOperation::Validate,
        RecoveryClass::Quarantine,
        detail,
    )
}

const fn unavailable(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Unsupported,
        ProcessOperation::Validate,
        RecoveryClass::RetryPreparation,
        detail,
    )
}
