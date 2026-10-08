//! Durable binding for one independently owned process-supervision operation.

mod request;
mod transport;

pub use request::RetainedOwnerRequest;
pub use transport::{
    RetainedOwnerObservation, RetainedOwnerReservation, RetainedProcessKey,
    RetainedProcessTransport, RetainedStreamPage,
};

use peritus_types::{ProcessId, Sha256Digest};
use sha2::{Digest as _, Sha256};

use crate::{ErrorCode, ProcessError, ProcessOperation, RecoveryClass};

const OPERATION_DOMAIN: &[u8] = b"peritus.retained-process-owner.operation.v1\0";
const SERVICE_OWNER_DOMAIN: &[u8] = b"peritus.retained-process-owner.service.v1\0";
const BACKEND_FACTORY_DOMAIN_V1: &[u8] =
    b"peritus.retained-process-owner.backend-factory.v1\0";
const BACKEND_FACTORY_DOMAIN_V2: &[u8] =
    b"peritus.retained-process-owner.backend-factory.v2\0";

/// Unpredictable identity of one pre-effect owner reservation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RetainedOwnerNonce([u8; 32]);

impl RetainedOwnerNonce {
    /// Allocates a fresh operating-system random reservation identity.
    ///
    /// # Errors
    /// Returns a typed pre-effect failure when the operating system cannot supply randomness.
    pub fn allocate() -> Result<Self, ProcessError> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| {
            owner_error(
                ErrorCode::Supervisor,
                ProcessOperation::Authorize,
                RecoveryClass::RetryPreparation,
                "retained process owner nonce cannot be allocated",
            )
        })?;
        Self::from_bytes(bytes)
    }

    /// Restores a persisted nonce after its containing record has been authenticated.
    ///
    /// # Errors
    /// Rejects the reserved all-zero identity.
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, ProcessError> {
        if bytes == [0; 32] {
            return Err(owner_mismatch("retained process owner nonce is invalid"));
        }
        Ok(Self(bytes))
    }

    /// Returns the exact persisted nonce bytes.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Digest-only identity of the service supervisor that owns retained processes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RetainedServiceOwner(Sha256Digest);

impl RetainedServiceOwner {
    /// Derives a non-secret durable identity from the live supervisor authentication token.
    #[must_use]
    pub fn from_token(token: &[u8]) -> Self {
        let mut hash = Sha256::new();
        hash.update(SERVICE_OWNER_DOMAIN);
        hash.update(u64::try_from(token.len()).unwrap_or(u64::MAX).to_be_bytes());
        hash.update(token);
        Self(Sha256Digest::new(hash.finalize().into()))
    }

    /// Restores the authenticated digest stored in an owner record.
    #[must_use]
    pub const fn from_digest(digest: Sha256Digest) -> Self {
        Self(digest)
    }

    /// Returns the durable identity without exposing the authentication token.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.0
    }
}

/// Complete identity of one authorized owner operation before backend preparation or launch.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RetainedOwnerBinding {
    process_id: ProcessId,
    nonce: RetainedOwnerNonce,
    service_owner: RetainedServiceOwner,
    action_digest: Sha256Digest,
    execution_plan_digest: Sha256Digest,
    sandbox_plan_digest: Sha256Digest,
    backend_descriptor_digest: Sha256Digest,
    backend_support_digest: Sha256Digest,
    backend_preparation_digest: Sha256Digest,
    operation_digest: Sha256Digest,
}

impl RetainedOwnerBinding {
    /// Binds consumed authority and every native preparation axis to one owner reservation.
    #[allow(clippy::too_many_arguments, reason = "each independently authenticated axis is explicit")]
    #[must_use]
    pub fn new(
        process_id: ProcessId,
        nonce: RetainedOwnerNonce,
        service_owner: RetainedServiceOwner,
        action_digest: Sha256Digest,
        execution_plan_digest: Sha256Digest,
        sandbox_plan_digest: Sha256Digest,
        backend_descriptor_digest: Sha256Digest,
        backend_support_digest: Sha256Digest,
        backend_preparation_digest: Sha256Digest,
    ) -> Self {
        let mut hash = Sha256::new();
        hash.update(OPERATION_DOMAIN);
        hash.update(process_id.as_bytes());
        hash.update(nonce.as_bytes());
        hash.update(service_owner.digest().as_bytes());
        hash.update(action_digest.as_bytes());
        hash.update(execution_plan_digest.as_bytes());
        hash.update(sandbox_plan_digest.as_bytes());
        hash.update(backend_descriptor_digest.as_bytes());
        hash.update(backend_support_digest.as_bytes());
        hash.update(backend_preparation_digest.as_bytes());
        let operation_digest = Sha256Digest::new(hash.finalize().into());
        Self {
            process_id,
            nonce,
            service_owner,
            action_digest,
            execution_plan_digest,
            sandbox_plan_digest,
            backend_descriptor_digest,
            backend_support_digest,
            backend_preparation_digest,
            operation_digest,
        }
    }

    /// Returns the exact consumed process identity.
    #[must_use]
    pub const fn process_id(self) -> ProcessId { self.process_id }
    /// Returns the unpredictable identity of this owner reservation.
    #[must_use]
    pub const fn nonce(self) -> RetainedOwnerNonce { self.nonce }
    /// Returns the authenticated service-owner generation.
    #[must_use]
    pub const fn service_owner(self) -> RetainedServiceOwner { self.service_owner }
    /// Returns the exact committed action digest.
    #[must_use]
    pub const fn action_digest(self) -> Sha256Digest { self.action_digest }
    /// Returns the exact execution-plan digest.
    #[must_use]
    pub const fn execution_plan_digest(self) -> Sha256Digest { self.execution_plan_digest }
    /// Returns the exact checked sandbox-plan digest.
    #[must_use]
    pub const fn sandbox_plan_digest(self) -> Sha256Digest { self.sandbox_plan_digest }
    /// Returns the exact backend descriptor digest.
    #[must_use]
    pub const fn backend_descriptor_digest(self) -> Sha256Digest { self.backend_descriptor_digest }
    /// Returns the exact backend support digest.
    #[must_use]
    pub const fn backend_support_digest(self) -> Sha256Digest { self.backend_support_digest }
    /// Returns the exact backend preparation digest.
    #[must_use]
    pub const fn backend_preparation_digest(self) -> Sha256Digest { self.backend_preparation_digest }
    /// Returns the canonical digest binding every owner-operation axis.
    #[must_use]
    pub const fn operation_digest(self) -> Sha256Digest { self.operation_digest }
}

/// Nonsensitive instructions for reconstructing one admitted backend from current trusted config.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedBackendFactoryRequest {
    platform: crate::NativePlatform,
    version: u16,
    payload: Vec<u8>,
    digest: Sha256Digest,
}

impl RetainedBackendFactoryRequest {
    /// Freezes one platform-owned canonical factory payload.
    ///
    /// # Errors
    /// Rejects an empty payload or unsupported zero version.
    pub fn new(
        platform: crate::NativePlatform,
        version: u16,
        payload: Vec<u8>,
    ) -> Result<Self, ProcessError> {
        if version == 0 || payload.is_empty() {
            return Err(owner_mismatch("retained backend factory payload is invalid"));
        }
        let mut hash = Sha256::new();
        let narrow_length = u32::try_from(payload.len()).ok();
        hash.update(if narrow_length.is_some() {
            BACKEND_FACTORY_DOMAIN_V1
        } else {
            BACKEND_FACTORY_DOMAIN_V2
        });
        hash.update([platform_tag(platform)]);
        hash.update(version.to_be_bytes());
        if let Some(length) = narrow_length {
            hash.update(length.to_be_bytes());
        } else {
            let length = u64::try_from(payload.len())
                .map_err(|_| owner_mismatch("retained backend factory payload is unaddressable"))?;
            hash.update(length.to_be_bytes());
        }
        hash.update(&payload);
        let digest = Sha256Digest::new(hash.finalize().into());
        Ok(Self { platform, version, payload, digest })
    }

    /// Returns the native platform selected by the trusted factory payload.
    #[must_use]
    pub const fn platform(&self) -> crate::NativePlatform { self.platform }
    /// Returns the platform factory codec version.
    #[must_use]
    pub const fn version(&self) -> u16 { self.version }
    /// Returns the nonsensitive canonical platform payload.
    #[must_use]
    pub fn payload(&self) -> &[u8] { &self.payload }
    /// Returns the digest binding platform, version, and complete payload.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest { self.digest }
}

pub(crate) const fn platform_tag(platform: crate::NativePlatform) -> u8 {
    match platform {
        crate::NativePlatform::Linux => 1,
        crate::NativePlatform::Macos => 2,
        crate::NativePlatform::Windows => 3,
    }
}

pub(crate) const fn owner_mismatch(detail: &'static str) -> ProcessError {
    owner_error(
        ErrorCode::PlanMismatch,
        ProcessOperation::Validate,
        RecoveryClass::Quarantine,
        detail,
    )
}

const fn owner_error(
    code: ErrorCode,
    operation: ProcessOperation,
    recovery: RecoveryClass,
    detail: &'static str,
) -> ProcessError {
    ProcessError::new(code, operation, recovery, detail)
}
