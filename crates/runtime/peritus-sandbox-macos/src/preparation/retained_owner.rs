//! Lossless retained-owner factory identity for the macOS backend.

use std::path::{Path, PathBuf};

use peritus_process::{
    ErrorCode, ExecutionPlan, NativePlatform, ProcessError, ProcessOperation, RecoveryClass,
    RetainedBackendFactoryRequest,
};
use peritus_sandbox::{BackendAdmission, CheckedSandboxPlan};
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use super::{MacosBackend, PreparationConfig, PreparationProgress};
use crate::{ProbeRequest, SystemProbe};

const FACTORY_VERSION_V1: u16 = 1;
const FACTORY_VERSION_V2: u16 = 2;
const PAYLOAD_TAG_V1: &[u8; 8] = b"MACFACT1";
const PAYLOAD_TAG_V2: &[u8; 8] = b"MACFACT2";
const PAYLOAD_PREFIX_BYTES: usize = PAYLOAD_TAG_V2.len() + 5 * 32;
const CONFIG_DOMAIN_V1: &[u8] = b"peritus.macos.retained-backend-config.v1\0";
const CONFIG_DOMAIN_V2: &[u8] = b"peritus.macos.retained-backend-config.v2\0";

#[derive(Clone, Debug, Eq, PartialEq)]
struct RetainedManagedNetwork {
    canonical: Vec<u8>,
    digest: Sha256Digest,
    command_state_root: PathBuf,
}

/// Authenticated macOS factory identity decoded before any host probe or target effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedMacosBackendFactory {
    version: u16,
    sandbox_digest: Sha256Digest,
    descriptor_digest: Sha256Digest,
    support_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
    config_digest: Sha256Digest,
    managed_network: Option<RetainedManagedNetwork>,
}

impl RetainedMacosBackendFactory {
    /// Decodes an exact platform-owned version-one or version-two payload.
    ///
    /// # Errors
    /// Rejects another platform, version, framing, or payload tag before probing the host.
    pub fn decode(request: &RetainedBackendFactoryRequest) -> Result<Self, ProcessError> {
        if request.platform() != NativePlatform::Macos {
            return Err(mismatch("retained macOS backend factory payload is not canonical"));
        }
        let payload = request.payload();
        let managed_network = match request.version() {
            FACTORY_VERSION_V1
                if payload.len() == PAYLOAD_PREFIX_BYTES
                    && payload.get(..PAYLOAD_TAG_V1.len())
                        == Some(PAYLOAD_TAG_V1.as_slice()) => None,
            FACTORY_VERSION_V2
                if payload.len() > PAYLOAD_PREFIX_BYTES
                    && payload.get(..PAYLOAD_TAG_V2.len())
                        == Some(PAYLOAD_TAG_V2.as_slice()) => {
                    decode_managed_network(&payload[PAYLOAD_PREFIX_BYTES..])?
                }
            _ => {
                return Err(mismatch(
                    "retained macOS backend factory payload is not canonical",
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
        config: PreparationConfig,
        execution: &ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
        should_continue: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Result<MacosBackend, ProcessError> {
        self.reconstruct_with_progress(
            config,
            execution,
            sandbox,
            should_continue,
            |_| {},
        )
    }

    /// Reconstructs the retained backend with explicit authorized-preparation progress.
    ///
    /// # Errors
    /// Rejects plan, trusted configuration, installation, or probe drift without preparing or
    /// launching the target.
    pub fn reconstruct_with_progress(
        self,
        config: PreparationConfig,
        execution: &ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
        should_continue: impl Fn() -> bool + Send + Sync + 'static,
        observe_progress: impl Fn(PreparationProgress) + Send + Sync + 'static,
    ) -> Result<MacosBackend, ProcessError> {
        self.validate_plan(execution, sandbox)?;
        if self.config_digest(&config)? != self.config_digest {
            return Err(unavailable(
                "current trusted macOS backend configuration differs from retained identity",
            ));
        }
        let request = ProbeRequest::without_proxy(
            config.helper_path().to_path_buf(),
            config.seatbelt_path().to_path_buf(),
        )
        .map_err(|_| unavailable("retained macOS probe request cannot be reconstructed"))?;
        let continues = std::sync::Arc::new(should_continue);
        let probe = SystemProbe::run_cancellable(&request, {
            let continues = std::sync::Arc::clone(&continues);
            move || continues()
        })
        .map_err(|_| unavailable("retained macOS backend installation cannot be reprobed"))?;
        let backend = MacosBackend::new_with_progress(
            &probe,
            config,
            move || continues(),
            observe_progress,
        )
        .map_err(|_| unavailable("retained macOS backend cannot be reconstructed"))?;
        if backend.descriptor().digest() != self.descriptor_digest
            || backend.descriptor().support_digest() != self.support_digest
        {
            return Err(unavailable(
                "reprobed macOS backend descriptor differs from retained admission",
            ));
        }
        Ok(backend)
    }

    /// Reports whether one freshly derived trusted configuration is the retained configuration.
    ///
    /// # Errors
    /// Returns unavailable when the candidate's protected resources are incomplete or cannot be
    /// represented by this retained factory version.
    pub fn matches_config(&self, config: &PreparationConfig) -> Result<bool, ProcessError> {
        Ok(self.config_digest(config)? == self.config_digest)
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

    fn config_digest(&self, config: &PreparationConfig) -> Result<Sha256Digest, ProcessError> {
        match self.version {
            FACTORY_VERSION_V1 => config_digest_v1(config),
            FACTORY_VERSION_V2 => config_digest_v2(config),
            _ => Err(mismatch("retained macOS backend factory version is invalid")),
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
                "retained macOS backend identity differs from decoded execution plans",
            ));
        }
        Ok(())
    }
}

pub(super) fn request(
    backend: &MacosBackend,
    sandbox: &CheckedSandboxPlan,
    admission: &BackendAdmission,
) -> Result<RetainedBackendFactoryRequest, ProcessError> {
    if backend.config.secrets.is_some() {
        return Err(unavailable(
            "retained macOS secret grants cannot be reconstructed",
        ));
    }
    let descriptor = backend.descriptor();
    if admission.plan_digest() != sandbox.digest()
        || admission.descriptor() != descriptor
        || admission.descriptor_digest() != descriptor.digest()
        || admission.support_digest() != descriptor.support_digest()
    {
        return Err(mismatch(
            "macOS backend, sandbox, and admission differ before owner transfer",
        ));
    }
    let managed_network = managed_network_identity(&backend.config)?;
    let mut payload = Vec::with_capacity(PAYLOAD_PREFIX_BYTES + 1);
    payload.extend_from_slice(PAYLOAD_TAG_V2);
    for digest in [
        sandbox.digest(),
        admission.descriptor_digest(),
        admission.support_digest(),
        admission.preparation_digest(),
        config_digest_v2(&backend.config)?,
    ] {
        payload.extend_from_slice(digest.as_bytes());
    }
    encode_managed_network(&mut payload, managed_network)?;
    RetainedBackendFactoryRequest::new(NativePlatform::Macos, FACTORY_VERSION_V2, payload)
}

fn config_digest_v1(config: &PreparationConfig) -> Result<Sha256Digest, ProcessError> {
    if config.proxy.is_some() || config.secrets.is_some() {
        return Err(unavailable(
            "retained macOS protected grants are unavailable from trusted configuration",
        ));
    }
    let mut hash = Sha256::new();
    hash.update(CONFIG_DOMAIN_V1);
    hash_path(&mut hash, config.helper_path())?;
    hash_path(&mut hash, config.seatbelt_path())?;
    hash_length(&mut hash, config.additional_protected_roots().len())?;
    for path in config.additional_protected_roots() {
        hash_path(&mut hash, path)?;
    }
    Ok(Sha256Digest::new(hash.finalize().into()))
}

fn config_digest_v2(config: &PreparationConfig) -> Result<Sha256Digest, ProcessError> {
    if config.secrets.is_some() {
        return Err(unavailable(
            "retained macOS secret grants cannot be reconstructed",
        ));
    }
    let managed_network = managed_network_identity(config)?;
    let mut hash = Sha256::new();
    hash.update(CONFIG_DOMAIN_V2);
    hash_path(&mut hash, config.helper_path())?;
    hash_path(&mut hash, config.seatbelt_path())?;
    hash_length(&mut hash, config.additional_protected_roots().len())?;
    for path in config.additional_protected_roots() {
        hash_path(&mut hash, path)?;
    }
    hash_managed_network(&mut hash, managed_network)?;
    Ok(Sha256Digest::new(hash.finalize().into()))
}

type ManagedNetworkRef<'a> = (&'a [u8], Sha256Digest, &'a Path);

fn managed_network_identity(
    config: &PreparationConfig,
) -> Result<Option<ManagedNetworkRef<'_>>, ProcessError> {
    match (
        config.managed_network_grant(),
        config.managed_network_cache_root(),
        config.proxy().is_some(),
    ) {
        (None, None, false) => Ok(None),
        (Some((canonical, grant_digest)), Some((root, cache_digest)), true)
            if grant_digest == cache_digest
                && peritus_codec::sha256(canonical) == grant_digest
                && root.is_absolute()
                && root.is_dir()
                && std::fs::canonicalize(root).ok().as_deref() == Some(root) => {
            Ok(Some((canonical, grant_digest, root)))
        }
        _ => Err(unavailable(
            "retained macOS managed-network identity is incomplete or inconsistent",
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
        .map_err(|_| mismatch("retained macOS managed-network grant exceeds framing"))?;
    payload.push(1);
    payload.extend_from_slice(digest.as_bytes());
    payload.extend_from_slice(&length.to_be_bytes());
    payload.extend_from_slice(canonical);
    encode_factory_path(payload, command_state_root)
}

fn decode_managed_network(bytes: &[u8]) -> Result<Option<RetainedManagedNetwork>, ProcessError> {
    match bytes.first().copied() {
        Some(0) if bytes.len() == 1 => Ok(None),
        Some(1) => {
            let digest_end = 33_usize;
            let length_end = digest_end
                .checked_add(4)
                .ok_or_else(|| mismatch("retained macOS managed-network framing overflows"))?;
            let digest_bytes: [u8; 32] = bytes
                .get(1..digest_end)
                .and_then(|value| value.try_into().ok())
                .ok_or_else(|| mismatch("retained macOS managed-network digest is truncated"))?;
            let length_bytes: [u8; 4] = bytes
                .get(digest_end..length_end)
                .and_then(|value| value.try_into().ok())
                .ok_or_else(|| mismatch("retained macOS managed-network length is truncated"))?;
            let length = usize::try_from(u32::from_be_bytes(length_bytes)).map_err(|_| {
                mismatch("retained macOS managed-network length is not representable")
            })?;
            let canonical_end = length_end
                .checked_add(length)
                .ok_or_else(|| mismatch("retained macOS managed-network length overflows"))?;
            let canonical = bytes
                .get(length_end..canonical_end)
                .ok_or_else(|| mismatch("retained macOS managed-network grant is truncated"))?
                .to_vec();
            let digest = Sha256Digest::new(digest_bytes);
            if canonical.is_empty() || peritus_codec::sha256(&canonical) != digest {
                return Err(mismatch(
                    "retained macOS managed-network grant identity is invalid",
                ));
            }
            let command_state_root = decode_factory_path(
                bytes
                    .get(canonical_end..)
                    .ok_or_else(|| mismatch("retained macOS command-state root is missing"))?,
            )?;
            Ok(Some(RetainedManagedNetwork {
                canonical,
                digest,
                command_state_root,
            }))
        }
        _ => Err(mismatch(
            "retained macOS managed-network framing is not canonical",
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

#[cfg(unix)]
fn encode_factory_path(payload: &mut Vec<u8>, path: &Path) -> Result<(), ProcessError> {
    use std::os::unix::ffi::OsStrExt as _;

    let bytes = path.as_os_str().as_bytes();
    let length = u32::try_from(bytes.len())
        .map_err(|_| mismatch("retained macOS command-state root exceeds framing"))?;
    payload.extend_from_slice(&length.to_be_bytes());
    payload.extend_from_slice(bytes);
    Ok(())
}

#[cfg(not(unix))]
fn encode_factory_path(payload: &mut Vec<u8>, path: &Path) -> Result<(), ProcessError> {
    let bytes = path
        .to_str()
        .ok_or_else(|| mismatch("retained macOS command-state root is not lossless"))?
        .as_bytes();
    let length = u32::try_from(bytes.len())
        .map_err(|_| mismatch("retained macOS command-state root exceeds framing"))?;
    payload.extend_from_slice(&length.to_be_bytes());
    payload.extend_from_slice(bytes);
    Ok(())
}

fn decode_factory_path(bytes: &[u8]) -> Result<PathBuf, ProcessError> {
    let length: [u8; 4] = bytes
        .get(..4)
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| mismatch("retained macOS command-state root framing is truncated"))?;
    let length = usize::try_from(u32::from_be_bytes(length))
        .map_err(|_| mismatch("retained macOS command-state root length is invalid"))?;
    let end = length
        .checked_add(4)
        .ok_or_else(|| mismatch("retained macOS command-state root length overflows"))?;
    if length == 0 || bytes.len() != end {
        return Err(mismatch(
            "retained macOS command-state root framing is not canonical",
        ));
    }
    decode_factory_path_bytes(&bytes[4..])
}

#[cfg(unix)]
fn decode_factory_path_bytes(bytes: &[u8]) -> Result<PathBuf, ProcessError> {
    use std::os::unix::ffi::OsStringExt as _;

    let path = PathBuf::from(std::ffi::OsString::from_vec(bytes.to_vec()));
    validate_factory_path(path)
}

#[cfg(not(unix))]
fn decode_factory_path_bytes(bytes: &[u8]) -> Result<PathBuf, ProcessError> {
    let path = std::str::from_utf8(bytes)
        .map(PathBuf::from)
        .map_err(|_| mismatch("retained macOS command-state root is not valid UTF-8"))?;
    validate_factory_path(path)
}

fn validate_factory_path(path: PathBuf) -> Result<PathBuf, ProcessError> {
    if !path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
    {
        return Err(mismatch(
            "retained macOS command-state root is not an exact absolute path",
        ));
    }
    Ok(path)
}

fn hash_length(hash: &mut Sha256, length: usize) -> Result<(), ProcessError> {
    let length = u64::try_from(length)
        .map_err(|_| mismatch("retained macOS backend configuration length is invalid"))?;
    hash.update(length.to_be_bytes());
    Ok(())
}

#[cfg(unix)]
fn hash_path(hash: &mut Sha256, path: &Path) -> Result<(), ProcessError> {
    use std::os::unix::ffi::OsStrExt as _;

    let bytes = path.as_os_str().as_bytes();
    hash_length(hash, bytes.len())?;
    hash.update(bytes);
    Ok(())
}

#[cfg(not(unix))]
fn hash_path(hash: &mut Sha256, path: &Path) -> Result<(), ProcessError> {
    let bytes = path
        .to_str()
        .ok_or_else(|| mismatch("macOS configuration path is not losslessly representable"))?
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
