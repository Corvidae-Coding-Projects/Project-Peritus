//! Exact opaque B3 execution-intent payload.

use peritus_types::{ProcessId, Sha256Digest};

use crate::{ProcessError, error::mismatch};

/// Stable media type for version-one execution intent payloads.
pub const EXECUTION_INTENT_MEDIA_TYPE: &str = "application/vnd.peritus.execution-plan.v1";
const DOMAIN: &[u8] = b"peritus.execution-intent.v1\0";
const ENCODED_BYTES: usize = DOMAIN.len() + 16 + 32 + 32 + 32;

/// Exact action payload binding one process to one execution/sandbox/backend selection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExecutionIntentPayload {
    process_id: ProcessId,
    execution_plan_digest: Sha256Digest,
    sandbox_plan_digest: Sha256Digest,
    backend_descriptor_digest: Sha256Digest,
}

impl ExecutionIntentPayload {
    /// Creates one exact process action payload.
    #[must_use]
    pub const fn new(
        process_id: ProcessId,
        execution_plan_digest: Sha256Digest,
        sandbox_plan_digest: Sha256Digest,
        backend_descriptor_digest: Sha256Digest,
    ) -> Self {
        Self { process_id, execution_plan_digest, sandbox_plan_digest, backend_descriptor_digest }
    }

    /// Returns the stable process identity.
    #[must_use]
    pub const fn process_id(self) -> ProcessId {
        self.process_id
    }
    /// Returns the complete execution-plan digest.
    #[must_use]
    pub const fn execution_plan_digest(self) -> Sha256Digest {
        self.execution_plan_digest
    }
    /// Returns the checked sandbox-plan digest.
    #[must_use]
    pub const fn sandbox_plan_digest(self) -> Sha256Digest {
        self.sandbox_plan_digest
    }
    /// Returns the selected backend descriptor digest.
    #[must_use]
    pub const fn backend_descriptor_digest(self) -> Sha256Digest {
        self.backend_descriptor_digest
    }

    /// Encodes the complete fixed-width version-one payload.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(ENCODED_BYTES);
        bytes.extend_from_slice(DOMAIN);
        bytes.extend_from_slice(self.process_id.as_bytes());
        bytes.extend_from_slice(self.execution_plan_digest.as_bytes());
        bytes.extend_from_slice(self.sandbox_plan_digest.as_bytes());
        bytes.extend_from_slice(self.backend_descriptor_digest.as_bytes());
        bytes
    }

    /// Decodes and checks one exact version-one payload.
    ///
    /// # Errors
    ///
    /// Returns an authorization mismatch for malformed, noncanonical, or zero-identity bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProcessError> {
        if bytes.len() != ENCODED_BYTES || !bytes.starts_with(DOMAIN) {
            return Err(mismatch("execution intent payload is not canonical version one"));
        }
        let mut offset = DOMAIN.len();
        let process_id = ProcessId::new(take::<16>(bytes, &mut offset))
            .map_err(|_| mismatch("execution intent contains an invalid process identity"))?;
        Ok(Self {
            process_id,
            execution_plan_digest: Sha256Digest::new(take::<32>(bytes, &mut offset)),
            sandbox_plan_digest: Sha256Digest::new(take::<32>(bytes, &mut offset)),
            backend_descriptor_digest: Sha256Digest::new(take::<32>(bytes, &mut offset)),
        })
    }
}

fn take<const N: usize>(bytes: &[u8], offset: &mut usize) -> [u8; N] {
    let mut value = [0_u8; N];
    value.copy_from_slice(&bytes[*offset..*offset + N]);
    *offset += N;
    value
}
