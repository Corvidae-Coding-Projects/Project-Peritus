//! Complete retained-owner request framing with protected resources kept out of band.

use peritus_sandbox::CheckedSandboxPlan;
use peritus_types::{ProcessId, Sha256Digest};

use crate::{ExecutionPlan, NativePlatform, ProcessError};

use super::{
    RetainedBackendFactoryRequest, RetainedOwnerBinding, RetainedOwnerNonce, RetainedServiceOwner,
    owner_mismatch, platform_tag,
};

const DOMAIN_V1: &[u8] = b"PERITUS-RETAINED-OWNER-REQUEST-V1\0";
const DOMAIN_V2: &[u8] = b"PERITUS-RETAINED-OWNER-REQUEST-V2\0";

/// Exact restart-safe owner input without live handles or prepared protected resources.
///
/// The embedded execution plan intentionally preserves the caller-authorized command arguments
/// and resolved environment values byte for byte. Secret and managed-network preparations remain
/// backend-owned capabilities; they are never represented by this frame's factory payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedOwnerRequest {
    binding: RetainedOwnerBinding,
    execution: ExecutionPlan,
    sandbox: CheckedSandboxPlan,
    backend_factory: RetainedBackendFactoryRequest,
    canonical: Vec<u8>,
    digest: Sha256Digest,
}

impl RetainedOwnerRequest {
    /// Freezes the exact plans, expected backend identities, and opaque trusted-config request.
    ///
    /// # Errors
    /// Rejects any process, sandbox, backend, operation, or platform axis that differs from the
    /// retained owner binding.
    pub fn new(
        binding: RetainedOwnerBinding,
        execution: &ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
        backend_factory: RetainedBackendFactoryRequest,
    ) -> Result<Self, ProcessError> {
        Self::from_owned(binding, execution.clone(), sandbox.clone(), backend_factory)
    }

    fn from_owned(
        binding: RetainedOwnerBinding,
        execution: ExecutionPlan,
        sandbox: CheckedSandboxPlan,
        backend_factory: RetainedBackendFactoryRequest,
    ) -> Result<Self, ProcessError> {
        execution.validate_restored_sandbox(&sandbox)?;
        if binding.process_id() != execution.identity().process_id()
            || binding.process_id() != sandbox.binding().process_id()
            || binding.execution_plan_digest() != execution.digest()
            || binding.sandbox_plan_digest() != sandbox.digest()
            || binding.backend_descriptor_digest() != execution.backend().descriptor_digest()
            || binding.backend_support_digest() != execution.backend().support_digest()
            || binding.backend_preparation_digest() != execution.backend().preparation_digest()
            || backend_factory.platform() != NativePlatform::current()
        {
            return Err(owner_mismatch(
                "retained owner request differs from its exact plan or backend binding",
            ));
        }
        let recomputed = RetainedOwnerBinding::new(
            binding.process_id(),
            binding.nonce(),
            binding.service_owner(),
            binding.action_digest(),
            execution.digest(),
            sandbox.digest(),
            execution.backend().descriptor_digest(),
            execution.backend().support_digest(),
            execution.backend().preparation_digest(),
        );
        if recomputed.operation_digest() != binding.operation_digest() {
            return Err(owner_mismatch("retained owner operation digest is invalid"));
        }
        let mut request = Self {
            binding,
            execution,
            sandbox,
            backend_factory,
            canonical: Vec::new(),
            digest: Sha256Digest::new([0; 32]),
        };
        request.canonical = encode_request(&request)?;
        request.digest = peritus_codec::sha256(&request.canonical);
        Ok(request)
    }

    /// Decodes and revalidates one complete version-one request.
    ///
    /// # Errors
    /// Rejects malformed, cross-platform, noncanonical, digest-mismatched, or trailing input.
    pub fn decode(bytes: Vec<u8>) -> Result<Self, ProcessError> {
        let (offset, wide) = if bytes.starts_with(DOMAIN_V1) {
            (DOMAIN_V1.len(), false)
        } else if bytes.starts_with(DOMAIN_V2) {
            (DOMAIN_V2.len(), true)
        } else {
            return Err(owner_mismatch("retained owner request domain is unsupported"));
        };
        let mut reader = Reader::new(&bytes, offset, wide);
        let process_id = ProcessId::new(reader.array()?)
            .map_err(|_| owner_mismatch("retained owner process id is invalid"))?;
        let nonce = RetainedOwnerNonce::from_bytes(reader.array()?)?;
        let service_owner =
            RetainedServiceOwner::from_digest(Sha256Digest::new(reader.array()?));
        let operation_digest = Sha256Digest::new(reader.array()?);
        let action_digest = Sha256Digest::new(reader.array()?);
        let execution_bytes = reader.frame()?;
        let execution_digest = Sha256Digest::new(reader.array()?);
        let sandbox_bytes = reader.frame()?;
        let sandbox_digest = Sha256Digest::new(reader.array()?);
        let descriptor_digest = Sha256Digest::new(reader.array()?);
        let support_digest = Sha256Digest::new(reader.array()?);
        let preparation_digest = Sha256Digest::new(reader.array()?);
        let platform = decode_platform(reader.u8()?)?;
        if platform != NativePlatform::current() {
            return Err(owner_mismatch("retained owner request targets another platform"));
        }
        let factory_version = reader.u16()?;
        let factory_payload = reader.frame()?;
        let factory_digest = Sha256Digest::new(reader.array()?);
        let checksum_offset = reader.position();
        let checksum = Sha256Digest::new(reader.array()?);
        reader.finish()?;
        if peritus_codec::sha256(&bytes[..checksum_offset]) != checksum {
            return Err(owner_mismatch("retained owner request checksum is invalid"));
        }

        let execution = ExecutionPlan::restore_canonical(copy_frame(
            execution_bytes,
            "allocate retained execution plan",
        )?)?;
        if execution.digest() != execution_digest {
            return Err(owner_mismatch("retained execution plan digest is invalid"));
        }
        let sandbox = CheckedSandboxPlan::restore_canonical(copy_frame(
            sandbox_bytes,
            "allocate retained sandbox plan",
        )?)
            .map_err(|_| owner_mismatch("retained sandbox plan is invalid"))?;
        if sandbox.digest() != sandbox_digest {
            return Err(owner_mismatch("retained sandbox plan digest is invalid"));
        }
        let backend_factory = RetainedBackendFactoryRequest::new(
            platform,
            factory_version,
            copy_frame(factory_payload, "allocate retained backend factory payload")?,
        )?;
        if backend_factory.digest() != factory_digest {
            return Err(owner_mismatch("retained backend factory digest is invalid"));
        }
        let binding = RetainedOwnerBinding::new(
            process_id,
            nonce,
            service_owner,
            action_digest,
            execution_digest,
            sandbox_digest,
            descriptor_digest,
            support_digest,
            preparation_digest,
        );
        if binding.operation_digest() != operation_digest {
            return Err(owner_mismatch("retained owner operation digest is invalid"));
        }
        let request = Self::from_owned(binding, execution, sandbox, backend_factory)?;
        if request.canonical != bytes {
            return Err(owner_mismatch("retained owner request is not canonical"));
        }
        Ok(request)
    }

    /// Returns the exact owner operation binding.
    #[must_use]
    pub const fn binding(&self) -> RetainedOwnerBinding {
        self.binding
    }

    /// Returns the restored execution plan.
    #[must_use]
    pub const fn execution_plan(&self) -> &ExecutionPlan {
        &self.execution
    }

    /// Returns the restored checked sandbox plan.
    #[must_use]
    pub const fn sandbox_plan(&self) -> &CheckedSandboxPlan {
        &self.sandbox
    }

    /// Returns the opaque platform-owned trusted-config reconstruction request.
    #[must_use]
    pub const fn backend_factory_request(&self) -> &RetainedBackendFactoryRequest {
        &self.backend_factory
    }

    /// Returns the expected current backend descriptor digest.
    #[must_use]
    pub const fn backend_descriptor_digest(&self) -> Sha256Digest {
        self.binding.backend_descriptor_digest()
    }

    /// Returns the expected current backend support digest.
    #[must_use]
    pub const fn backend_support_digest(&self) -> Sha256Digest {
        self.binding.backend_support_digest()
    }

    /// Returns the preparation digest that production admission must reproduce.
    #[must_use]
    pub const fn backend_preparation_digest(&self) -> Sha256Digest {
        self.binding.backend_preparation_digest()
    }

    /// Borrows the complete checksummed canonical request bytes.
    #[must_use]
    pub fn encode(&self) -> &[u8] {
        &self.canonical
    }

    /// Returns SHA-256 over the complete encoded request, including its final checksum.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

fn encode_request(request: &RetainedOwnerRequest) -> Result<Vec<u8>, ProcessError> {
    let wide = [
        request.execution.canonical_bytes().len(),
        request.sandbox.canonical_bytes().len(),
        request.backend_factory.payload().len(),
    ]
    .into_iter()
    .any(|length| u32::try_from(length).is_err());
    let mut writer = Writer::new(wide);
    writer.raw(if wide { DOMAIN_V2 } else { DOMAIN_V1 })?;
    writer.raw(request.binding.process_id().as_bytes())?;
    writer.raw(&request.binding.nonce().as_bytes())?;
    writer.raw(request.binding.service_owner().digest().as_bytes())?;
    writer.raw(request.binding.operation_digest().as_bytes())?;
    writer.raw(request.binding.action_digest().as_bytes())?;
    writer.frame(request.execution.canonical_bytes())?;
    writer.raw(request.execution.digest().as_bytes())?;
    writer.frame(request.sandbox.canonical_bytes())?;
    writer.raw(request.sandbox.digest().as_bytes())?;
    writer.raw(request.binding.backend_descriptor_digest().as_bytes())?;
    writer.raw(request.binding.backend_support_digest().as_bytes())?;
    writer.raw(request.binding.backend_preparation_digest().as_bytes())?;
    writer.u8(platform_tag(request.backend_factory.platform()))?;
    writer.u16(request.backend_factory.version())?;
    writer.frame(request.backend_factory.payload())?;
    writer.raw(request.backend_factory.digest().as_bytes())?;
    let checksum = peritus_codec::sha256(writer.bytes());
    writer.raw(checksum.as_bytes())?;
    Ok(writer.finish())
}

fn copy_frame(value: &[u8], detail: &'static str) -> Result<Vec<u8>, ProcessError> {
    let mut owned = Vec::new();
    owned.try_reserve(value.len()).map_err(|_| owner_mismatch(detail))?;
    owned.extend_from_slice(value);
    Ok(owned)
}

const fn decode_platform(tag: u8) -> Result<NativePlatform, ProcessError> {
    match tag {
        1 => Ok(NativePlatform::Linux),
        2 => Ok(NativePlatform::Macos),
        3 => Ok(NativePlatform::Windows),
        _ => Err(owner_mismatch("retained backend platform tag is invalid")),
    }
}

struct Writer {
    bytes: Vec<u8>,
    wide: bool,
}

impl Writer {
    const fn new(wide: bool) -> Self {
        Self { bytes: Vec::new(), wide }
    }

    fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn raw(&mut self, value: &[u8]) -> Result<(), ProcessError> {
        self.bytes
            .try_reserve(value.len())
            .map_err(|_| owner_mismatch("allocate retained owner request"))?;
        self.bytes.extend_from_slice(value);
        Ok(())
    }

    fn u8(&mut self, value: u8) -> Result<(), ProcessError> {
        self.raw(&[value])
    }

    fn u16(&mut self, value: u16) -> Result<(), ProcessError> {
        self.raw(&value.to_be_bytes())
    }

    fn frame(&mut self, value: &[u8]) -> Result<(), ProcessError> {
        if self.wide {
            let length = u64::try_from(value.len())
                .map_err(|_| owner_mismatch("retained owner frame is unaddressable"))?;
            self.raw(&length.to_be_bytes())?;
        } else {
            let length = u32::try_from(value.len())
                .map_err(|_| owner_mismatch("retained owner frame exceeds its representation"))?;
            self.raw(&length.to_be_bytes())?;
        }
        self.raw(value)
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
    wide: bool,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8], position: usize, wide: bool) -> Self {
        Self { bytes, position, wide }
    }

    const fn position(&self) -> usize {
        self.position
    }

    fn finish(&self) -> Result<(), ProcessError> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(owner_mismatch("retained owner request contains trailing data"))
        }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ProcessError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or_else(|| owner_mismatch("retained owner request position overflow"))?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| owner_mismatch("retained owner request is truncated"))?;
        self.position = end;
        Ok(value)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], ProcessError> {
        self.take(N)?
            .try_into()
            .map_err(|_| owner_mismatch("retained owner fixed field is truncated"))
    }

    fn u8(&mut self) -> Result<u8, ProcessError> {
        Ok(self.array::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, ProcessError> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, ProcessError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, ProcessError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn frame(&mut self) -> Result<&'a [u8], ProcessError> {
        let encoded = if self.wide { self.u64()? } else { u64::from(self.u32()?) };
        let length = usize::try_from(encoded)
            .map_err(|_| owner_mismatch("retained owner frame length is unsupported"))?;
        self.take(length)
    }
}
