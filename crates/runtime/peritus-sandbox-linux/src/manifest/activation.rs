//! Fixed helper activation record used by backend-local diagnostics.

use super::manifest_error;
use crate::LinuxError;
use peritus_types::Sha256Digest;

const ACTIVATION_MAGIC: [u8; 8] = *b"PRTLNXA1";
const VERSION: u16 = 1;

/// Fixed helper activation fact emitted after all native controls are installed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivationRecord {
    plan_digest: Sha256Digest,
    backend_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
    landlock: bool,
    seccomp: bool,
    no_new_privs: bool,
}

impl ActivationRecord {
    /// Creates a complete activation record.
    #[must_use]
    pub const fn new(
        plan_digest: Sha256Digest,
        backend_digest: Sha256Digest,
        preparation_digest: Sha256Digest,
        landlock: bool,
        seccomp: bool,
        no_new_privs: bool,
    ) -> Self {
        Self { plan_digest, backend_digest, preparation_digest, landlock, seccomp, no_new_privs }
    }
    /// Returns the plan digest.
    #[must_use]
    pub const fn plan_digest(self) -> Sha256Digest {
        self.plan_digest
    }
    /// Reports Landlock installation.
    #[must_use]
    pub const fn landlock(self) -> bool {
        self.landlock
    }
    /// Reports seccomp installation.
    #[must_use]
    pub const fn seccomp(self) -> bool {
        self.seccomp
    }
    /// Reports no-new-privileges installation.
    #[must_use]
    pub const fn no_new_privs(self) -> bool {
        self.no_new_privs
    }
    /// Encodes one fixed-size checksummed record.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(139);
        bytes.extend_from_slice(&ACTIVATION_MAGIC);
        bytes.extend_from_slice(&VERSION.to_be_bytes());
        bytes.extend_from_slice(self.plan_digest.as_bytes());
        bytes.extend_from_slice(self.backend_digest.as_bytes());
        bytes.extend_from_slice(self.preparation_digest.as_bytes());
        let flags = u8::from(self.landlock)
            | (u8::from(self.seccomp) << 1)
            | (u8::from(self.no_new_privs) << 2);
        bytes.push(flags);
        let checksum = peritus_codec::sha256(&bytes);
        bytes.extend_from_slice(checksum.as_bytes());
        bytes
    }
    /// Decodes and verifies one fixed activation record.
    ///
    /// # Errors
    /// Rejects any wrong length, checksum, version, or flag.
    pub fn decode(bytes: &[u8]) -> Result<Self, LinuxError> {
        if bytes.len() != 139 {
            return Err(manifest_error("activation record length is invalid"));
        }
        let (body, checksum) = bytes.split_at(107);
        if peritus_codec::sha256(body).as_bytes() != checksum {
            return Err(manifest_error("activation record checksum mismatch"));
        }
        let mut reader = crate::canonical::Reader::new(body);
        if reader.fixed::<8>()? != ACTIVATION_MAGIC || reader.u16()? != VERSION {
            return Err(manifest_error("activation record magic or version is unsupported"));
        }
        let plan_digest = Sha256Digest::new(reader.fixed()?);
        let backend_digest = Sha256Digest::new(reader.fixed()?);
        let preparation_digest = Sha256Digest::new(reader.fixed()?);
        let flags = reader.u8()?;
        reader.finish()?;
        if flags & !0x07 != 0 {
            return Err(manifest_error("activation record contains unknown flags"));
        }
        Ok(Self::new(
            plan_digest,
            backend_digest,
            preparation_digest,
            flags & 1 != 0,
            flags & 2 != 0,
            flags & 4 != 0,
        ))
    }
}
