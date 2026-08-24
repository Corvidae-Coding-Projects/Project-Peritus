//! Versioned nonsensitive managed-proxy recovery records.

use peritus_types::{ProcessId, Sha256Digest};

use crate::{NetworkError, NetworkErrorKind, NetworkOperation, RecoveryClass};

/// Reopened proxy ownership classification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProxyRecoveryState {
    /// Exact owner is live and may be cancelled.
    LiveOwned,
    /// Listener and workers are absent and clean.
    AbsentClean,
    /// Runtime identity differs and must not be touched.
    Mismatched,
    /// Exact state cannot be established.
    Indeterminate,
}

/// Nonsensitive version-one proxy recovery identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProxyRecoveryRecord {
    owner: ProcessId,
    plan_digest: Sha256Digest,
    routing_token_digest: Sha256Digest,
    listener_port: u16,
    active_workers: u16,
    released: bool,
    checksum: Sha256Digest,
}

impl ProxyRecoveryRecord {
    /// Creates and checksums one bounded runtime record.
    #[must_use]
    pub fn new(
        owner: ProcessId,
        plan_digest: Sha256Digest,
        routing_token_digest: Sha256Digest,
        listener_port: u16,
        active_workers: u16,
        released: bool,
    ) -> Self {
        let checksum = checksum(
            owner,
            plan_digest,
            routing_token_digest,
            listener_port,
            active_workers,
            released,
        );
        Self {
            owner,
            plan_digest,
            routing_token_digest,
            listener_port,
            active_workers,
            released,
            checksum,
        }
    }
    /// Validates the checksum and classifies an exact current observation.
    ///
    /// # Errors
    /// Rejects a corrupt record.
    pub fn classify(
        &self,
        observed_owner: ProcessId,
        listener_live: Option<bool>,
        observed_workers: Option<u16>,
    ) -> Result<ProxyRecoveryState, NetworkError> {
        if self.checksum
            != checksum(
                self.owner,
                self.plan_digest,
                self.routing_token_digest,
                self.listener_port,
                self.active_workers,
                self.released,
            )
        {
            return Err(recovery_error("proxy recovery checksum differs"));
        }
        if observed_owner != self.owner {
            return Ok(ProxyRecoveryState::Mismatched);
        }
        Ok(match (listener_live, observed_workers) {
            (Some(false), Some(0)) if self.released => ProxyRecoveryState::AbsentClean,
            (Some(true), Some(workers)) if !self.released && workers <= self.active_workers => {
                ProxyRecoveryState::LiveOwned
            }
            (Some(_), Some(_)) => ProxyRecoveryState::Mismatched,
            _ => ProxyRecoveryState::Indeterminate,
        })
    }
    /// Returns owner.
    #[must_use]
    pub const fn owner(&self) -> ProcessId {
        self.owner
    }
    /// Returns plan digest.
    #[must_use]
    pub const fn plan_digest(&self) -> Sha256Digest {
        self.plan_digest
    }
}

fn checksum(
    owner: ProcessId,
    plan: Sha256Digest,
    token: Sha256Digest,
    port: u16,
    workers: u16,
    released: bool,
) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(101);
    bytes.extend_from_slice(b"PERITUS_PROXY_RECOVERY\0\x01");
    bytes.extend_from_slice(owner.as_bytes());
    bytes.extend_from_slice(plan.as_bytes());
    bytes.extend_from_slice(token.as_bytes());
    bytes.extend_from_slice(&port.to_be_bytes());
    bytes.extend_from_slice(&workers.to_be_bytes());
    bytes.push(u8::from(released));
    peritus_codec::sha256(&bytes)
}

const fn recovery_error(detail: &'static str) -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Recovery,
        NetworkOperation::Recover,
        RecoveryClass::Reconcile,
        detail,
    )
}
