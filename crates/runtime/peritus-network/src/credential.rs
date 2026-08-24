//! Opaque routing tokens and exact upstream credential leases.

use core::fmt;
use std::sync::{Arc, Mutex};

use base64::Engine;
use peritus_sandbox::{
    HostMatcher, NetworkContract, NetworkDecision, NetworkRule, NetworkTarget, PortRange,
    RuleEffect, SecretReference, Transport,
};
use peritus_types::{ProcessId, Sha256Digest};
use zeroize::Zeroizing;

use crate::{DestinationRequest, NetworkError, NetworkErrorKind, NetworkOperation, RecoveryClass};

/// Per-launch proxy routing token supplied to the sandboxed child.
#[derive(Eq, PartialEq)]
pub struct RoutingToken([u8; 32]);

impl RoutingToken {
    /// Stores caller-generated random token bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    /// Verifies a lowercase hexadecimal header in constant work.
    #[must_use]
    pub fn verifies_hex(&self, candidate: &str) -> bool {
        if candidate.len() != 64 {
            return false;
        }
        let mut difference = 0_u8;
        for (index, pair) in candidate.as_bytes().chunks_exact(2).enumerate() {
            let decoded =
                decode_hex(pair[0]).zip(decode_hex(pair[1])).map(|(high, low)| high << 4 | low);
            difference |= decoded.map_or(0xff, |byte| byte ^ self.0[index]);
        }
        difference == 0
    }

    /// Verifies the proxy authorization form emitted by managed clients.
    ///
    /// The native helper may expose the route as `http://peritus:<hex>@loopback:port`; ordinary
    /// HTTP clients encode that user-info as Basic proxy authorization. The direct `Peritus <hex>`
    /// form remains available to protocol-aware clients. Decoded Basic bytes are zeroized.
    #[must_use]
    pub fn verifies_authorization(&self, authorization: &str) -> bool {
        if let Some(candidate) = authorization.strip_prefix("Peritus ") {
            return self.verifies_hex(candidate);
        }
        let Some(encoded) = authorization.strip_prefix("Basic ") else {
            return false;
        };
        let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
            return false;
        };
        let decoded = Zeroizing::new(decoded);
        let Some(candidate) = decoded.strip_prefix(b"peritus:") else {
            return false;
        };
        let Ok(candidate) = std::str::from_utf8(candidate) else {
            return false;
        };
        self.verifies_hex(candidate)
    }
    /// Exposes the routing header only for scoped child configuration.
    pub fn expose_header<R>(&self, operation: impl FnOnce(&str) -> R) -> R {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut value = Zeroizing::new(String::with_capacity(64));
        for byte in self.0 {
            value.push(char::from(HEX[usize::from(byte >> 4)]));
            value.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        operation(&value)
    }

    /// Exposes token bytes only to an exact protected-handle staging operation.
    pub fn expose_bytes<R>(&self, operation: impl FnOnce(&[u8; 32]) -> R) -> R {
        operation(&self.0)
    }
}

impl fmt::Debug for RoutingToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RoutingToken([REDACTED])")
    }
}

impl Drop for RoutingToken {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

const fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// Exact scoped lease for one upstream credential.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CredentialLease {
    reference: SecretReference,
    matcher: HostMatcher,
    transport: Transport,
    port: u16,
    header_name: String,
    remaining_uses: u32,
    expires_epoch_millis: u64,
    plan_digest: Sha256Digest,
    owner: ProcessId,
    revoked: bool,
}

impl CredentialLease {
    /// Creates one exact bounded credential injection lease.
    ///
    /// # Errors
    /// Rejects zero port/uses/expiry or a nonportable header name.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        reference: SecretReference,
        matcher: HostMatcher,
        transport: Transport,
        port: u16,
        header_name: impl Into<String>,
        uses: u32,
        expires_epoch_millis: u64,
        plan_digest: Sha256Digest,
        owner: ProcessId,
    ) -> Result<Self, NetworkError> {
        let header_name = header_name.into();
        if port == 0
            || uses == 0
            || expires_epoch_millis == 0
            || header_name.is_empty()
            || header_name.len() > 128
            || !header_name.bytes().all(is_header_name_byte)
        {
            return Err(credential_error("credential lease fields are invalid"));
        }
        Ok(Self {
            reference,
            matcher,
            transport,
            port,
            header_name,
            remaining_uses: uses,
            expires_epoch_millis,
            plan_digest,
            owner,
            revoked: false,
        })
    }
    /// Consumes one exact matching use.
    ///
    /// # Errors
    /// Rejects mismatched, expired, exhausted, or revoked leases without decrementing.
    pub fn consume(
        &mut self,
        request: &DestinationRequest,
        plan_digest: Sha256Digest,
        owner: ProcessId,
        now_epoch_millis: u64,
    ) -> Result<(), NetworkError> {
        if self.revoked
            || self.remaining_uses == 0
            || now_epoch_millis >= self.expires_epoch_millis
            || plan_digest != self.plan_digest
            || owner != self.owner
            || request.transport() != self.transport
            || request.port() != self.port
            || !matcher_matches(&self.matcher, request)?
        {
            return Err(credential_error(
                "credential lease is expired, exhausted, revoked, or mismatched",
            ));
        }
        self.remaining_uses -= 1;
        Ok(())
    }
    /// Revokes future use idempotently.
    pub const fn revoke(&mut self) {
        self.revoked = true;
    }
    /// Returns the opaque secret reference.
    #[must_use]
    pub const fn reference(&self) -> SecretReference {
        self.reference
    }
    /// Returns the exact header name.
    #[must_use]
    pub fn header_name(&self) -> &str {
        &self.header_name
    }
    /// Returns remaining uses.
    #[must_use]
    pub const fn remaining_uses(&self) -> u32 {
        self.remaining_uses
    }
    /// Returns whether revoked.
    #[must_use]
    pub const fn is_revoked(&self) -> bool {
        self.revoked
    }
}

fn matcher_matches(
    matcher: &HostMatcher,
    request: &DestinationRequest,
) -> Result<bool, NetworkError> {
    let range = PortRange::new(request.port(), request.port())
        .map_err(|_| credential_error("credential destination port is invalid"))?;
    let contract = NetworkContract::new(vec![NetworkRule::new(
        RuleEffect::Allow,
        matcher.clone(),
        request.transport(),
        range,
    )])
    .map_err(|_| credential_error("credential matcher cannot be represented"))?;
    let target = NetworkTarget::new(request.host().clone(), request.transport(), request.port())
        .map_err(|_| credential_error("credential target cannot be represented"))?;
    Ok(contract.decide(&target) == NetworkDecision::Allowed)
}

const fn is_header_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

/// Non-clone zero-on-drop upstream header material.
pub struct ScopedCredential(Vec<u8>);

impl ScopedCredential {
    /// Validates an HTTP field value without CR/LF or control bytes.
    ///
    /// # Errors
    /// Rejects empty, oversized, or injection-capable material.
    pub fn new(value: Vec<u8>) -> Result<Self, NetworkError> {
        if value.is_empty()
            || value.len() > 16 * 1_024
            || value.iter().any(|byte| {
                *byte == b'\r'
                    || *byte == b'\n'
                    || (*byte < 0x20 && *byte != b'\t')
                    || *byte == 0x7f
            })
        {
            return Err(credential_error(
                "upstream credential is empty, excessive, or not a safe field value",
            ));
        }
        Ok(Self(value))
    }
    /// Exposes bytes only to the exact injection operation.
    pub fn expose<R>(&self, operation: impl FnOnce(&[u8]) -> R) -> R {
        operation(&self.0)
    }
}

impl fmt::Debug for ScopedCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("ScopedCredential").field("bytes", &"[REDACTED]").finish()
    }
}

impl Drop for ScopedCredential {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

/// Provider boundary used only after destination and lease admission.
pub trait CredentialProvider: Send + Sync + 'static {
    /// Resolves exact scoped header material.
    ///
    /// # Errors
    /// Returns a non-content-bearing credential error.
    fn acquire(&self, reference: SecretReference) -> Result<ScopedCredential, NetworkError>;
}

/// Shared exact credential configuration used by bounded proxy workers.
pub struct ProxyCredential {
    pub(crate) lease: Mutex<CredentialLease>,
    pub(crate) provider: Arc<dyn CredentialProvider>,
}

impl ProxyCredential {
    /// Couples an exact lease with its material provider.
    #[must_use]
    pub fn new(lease: CredentialLease, provider: Arc<dyn CredentialProvider>) -> Self {
        Self { lease: Mutex::new(lease), provider }
    }

    /// Revokes future injection uses idempotently.
    pub fn revoke(&self) {
        self.lease.lock().unwrap_or_else(std::sync::PoisonError::into_inner).revoke();
    }
}

impl fmt::Debug for ProxyCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProxyCredential { lease: [SCOPED], provider: [OPAQUE] }")
    }
}

const fn credential_error(detail: &'static str) -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Credential,
        NetworkOperation::Credential,
        RecoveryClass::ReacquireCredential,
        detail,
    )
}
