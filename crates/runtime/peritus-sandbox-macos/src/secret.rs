//! Protected secret-handle metadata and exact checked destinations.

use peritus_process::NativeProtectedHandle;
use peritus_sandbox::{
    BrokeredHandleLabel, EnvironmentName, SandboxPath, SecretDelivery, SecretReference,
};
use peritus_types::Sha256Digest;

use crate::{
    MacosError, MacosErrorKind, MacosOperation, RecoveryAction,
    canonical::{Reader, Writer},
};

const MAX_SECRET_HANDLES: usize = 128;
const MAX_HANDLE_LABEL_BYTES: usize = 256;
const MAX_PAYLOAD_BYTES: u32 = 1_024 * 1_024;

/// Nonsensitive destination bound to one protected anonymous payload handle.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SecretHandleDestination {
    /// Helper installs the payload under this exact environment name immediately before exec.
    Environment(EnvironmentName),
    /// Helper materializes the payload at this exact private file destination.
    File(SandboxPath),
    /// Target inherits the payload descriptor under this exact opaque label.
    Brokered(BrokeredHandleLabel),
}

/// Nonsensitive secret binding and inherited-handle metadata encoded in the helper manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecretHandleDescriptor {
    descriptor: u32,
    label: String,
    payload_len: u32,
    reference_digest: Sha256Digest,
    destination: SecretHandleDestination,
}

impl SecretHandleDescriptor {
    pub(crate) fn new(
        descriptor: u32,
        label: String,
        payload_len: u32,
        reference_digest: Sha256Digest,
        destination: SecretHandleDestination,
    ) -> Result<Self, MacosError> {
        if descriptor < 3
            || descriptor > i32::MAX.cast_unsigned()
            || label.is_empty()
            || label.len() > MAX_HANDLE_LABEL_BYTES
            || !label.is_ascii()
            || label.bytes().any(|byte| byte.is_ascii_control())
            || payload_len == 0
            || payload_len > MAX_PAYLOAD_BYTES
            || reference_digest == Sha256Digest::new([0; 32])
        {
            return Err(secret_error("secret handle manifest metadata is invalid"));
        }
        Ok(Self { descriptor, label, payload_len, reference_digest, destination })
    }

    /// Returns the inherited descriptor number.
    #[must_use]
    pub const fn descriptor(&self) -> u32 {
        self.descriptor
    }

    /// Returns the nonsensitive protected-handle label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the exact bounded payload length.
    #[must_use]
    pub const fn payload_len(&self) -> u32 {
        self.payload_len
    }

    /// Returns the exact nonsensitive secret-reference digest.
    #[must_use]
    pub const fn reference_digest(&self) -> Sha256Digest {
        self.reference_digest
    }

    /// Returns the exact declared delivery destination.
    #[must_use]
    pub const fn destination(&self) -> &SecretHandleDestination {
        &self.destination
    }

    pub(crate) fn encode(&self, writer: &mut Writer) -> Result<(), MacosError> {
        writer.u32(self.descriptor)?;
        writer.string(&self.label)?;
        writer.u32(self.payload_len)?;
        writer.fixed(self.reference_digest.as_bytes())?;
        match &self.destination {
            SecretHandleDestination::Environment(name) => {
                writer.u8(0)?;
                writer.string(name.as_str())
            }
            SecretHandleDestination::File(path) => {
                writer.u8(1)?;
                writer.string(path.as_str())
            }
            SecretHandleDestination::Brokered(label) => {
                writer.u8(2)?;
                writer.string(label.as_str())
            }
        }
    }

    pub(crate) fn decode(reader: &mut Reader<'_>) -> Result<Self, MacosError> {
        let descriptor = reader.u32()?;
        let label = reader.string()?;
        let payload_len = reader.u32()?;
        let reference_digest = Sha256Digest::new(reader.fixed()?);
        let destination = match reader.u8()? {
            0 => SecretHandleDestination::Environment(
                EnvironmentName::new(reader.string()?).map_err(|_| {
                    secret_error("manifest secret environment destination is invalid")
                })?,
            ),
            1 => SecretHandleDestination::File(
                SandboxPath::new(reader.string()?)
                    .map_err(|_| secret_error("manifest secret file destination is invalid"))?,
            ),
            2 => SecretHandleDestination::Brokered(
                BrokeredHandleLabel::new(reader.string()?)
                    .map_err(|_| secret_error("manifest brokered secret destination is invalid"))?,
            ),
            _ => return Err(secret_error("manifest secret destination tag is invalid")),
        };
        Self::new(descriptor, label, payload_len, reference_digest, destination)
    }
}

impl From<&SecretDelivery> for SecretHandleDestination {
    fn from(value: &SecretDelivery) -> Self {
        match value {
            SecretDelivery::Environment(name) => Self::Environment(name.clone()),
            SecretDelivery::File(path) => Self::File(path.clone()),
            SecretDelivery::BrokeredHandle(label) => Self::Brokered(label.clone()),
        }
    }
}

/// One process-owned protected payload plus its exact reference/destination binding.
#[derive(Clone, Debug)]
pub struct ProtectedSecretHandle {
    handle: NativeProtectedHandle,
    reference_digest: Sha256Digest,
    destination: SecretHandleDestination,
}

impl ProtectedSecretHandle {
    /// Binds an already-created protected payload to one checked secret reference and destination.
    ///
    /// # Errors
    /// Rejects a handle whose native value cannot be represented as a macOS descriptor.
    pub fn new(
        handle: NativeProtectedHandle,
        reference: SecretReference,
        destination: SecretHandleDestination,
    ) -> Result<Self, MacosError> {
        descriptor_number(&handle)?;
        if handle.payload_len().is_none() {
            return Err(secret_error(
                "protected secret handle must contain one finite staged payload",
            ));
        }
        Ok(Self { handle, reference_digest: secret_reference_digest(reference), destination })
    }

    /// Returns the process-owned protected payload handle.
    #[must_use]
    pub const fn handle(&self) -> &NativeProtectedHandle {
        &self.handle
    }

    /// Returns the exact nonsensitive secret-reference digest.
    #[must_use]
    pub const fn reference_digest(&self) -> Sha256Digest {
        self.reference_digest
    }

    /// Returns the exact checked delivery destination.
    #[must_use]
    pub const fn destination(&self) -> &SecretHandleDestination {
        &self.destination
    }

    pub(crate) fn descriptor(&self) -> u32 {
        // Construction and canonicalization both prove representability.
        u32::try_from(self.handle.raw_handle()).unwrap_or(u32::MAX)
    }

    pub(crate) fn payload_len(&self) -> Result<usize, MacosError> {
        self.handle
            .payload_len()
            .ok_or_else(|| secret_error("protected secret handle has no finite payload length"))
    }

    pub(crate) fn manifest_descriptor(&self) -> Result<SecretHandleDescriptor, MacosError> {
        SecretHandleDescriptor::new(
            self.descriptor(),
            self.handle.label().to_owned(),
            u32::try_from(self.payload_len()?)
                .map_err(|_| secret_error("protected secret payload is too large"))?,
            self.reference_digest,
            self.destination.clone(),
        )
    }
}

/// Canonicalizes protected secret handles and rejects handle, label, or destination collisions.
///
/// # Errors
/// Rejects an excessive set, duplicate identity, or a native value outside macOS descriptor range.
pub fn canonical_secret_handles(
    mut handles: Vec<ProtectedSecretHandle>,
) -> Result<Vec<ProtectedSecretHandle>, MacosError> {
    if handles.len() > MAX_SECRET_HANDLES {
        return Err(secret_error("protected secret handle count exceeds its bound"));
    }
    handles.sort_by_key(ProtectedSecretHandle::descriptor);
    for handle in &handles {
        descriptor_number(handle.handle())?;
    }
    if handles.windows(2).any(|pair| {
        pair[0].descriptor() == pair[1].descriptor()
            || pair[0].handle.label() == pair[1].handle.label()
    }) {
        return Err(secret_error("protected secret handle identities collide"));
    }
    for (index, handle) in handles.iter().enumerate() {
        if handles[..index].iter().any(|prior| prior.destination == handle.destination) {
            return Err(secret_error("protected secret destinations collide"));
        }
    }
    Ok(handles)
}

/// Computes the nonsensitive canonical identity of one checked secret reference.
#[must_use]
pub fn secret_reference_digest(reference: SecretReference) -> Sha256Digest {
    let mut bytes = Vec::from(b"PERITUS-MACOS-SECRET-REFERENCE-V1\0".as_slice());
    bytes.extend_from_slice(reference.resource_id().as_bytes());
    bytes.extend_from_slice(reference.version().as_bytes());
    peritus_codec::sha256(&bytes)
}

/// Computes one nonsensitive recovery identity over exact secret references and destinations.
#[must_use]
pub fn secret_binding_digest(secrets: &[SecretHandleDescriptor]) -> Option<Sha256Digest> {
    if secrets.is_empty() {
        return None;
    }
    let mut bytes = Vec::from(b"PERITUS-MACOS-SECRET-BINDINGS-V1\0".as_slice());
    for secret in secrets {
        bytes.extend_from_slice(secret.reference_digest().as_bytes());
        match secret.destination() {
            SecretHandleDestination::Environment(name) => {
                bytes.push(0);
                bytes.extend_from_slice(name.as_str().as_bytes());
            }
            SecretHandleDestination::File(path) => {
                bytes.push(1);
                bytes.extend_from_slice(path.as_str().as_bytes());
            }
            SecretHandleDestination::Brokered(label) => {
                bytes.push(2);
                bytes.extend_from_slice(label.as_str().as_bytes());
            }
        }
        bytes.push(0);
    }
    Some(peritus_codec::sha256(&bytes))
}

fn descriptor_number(handle: &NativeProtectedHandle) -> Result<u32, MacosError> {
    let descriptor = u32::try_from(handle.raw_handle())
        .map_err(|_| secret_error("protected secret descriptor is outside macOS range"))?;
    if descriptor < 3 || descriptor > i32::MAX.cast_unsigned() {
        return Err(secret_error("protected secret descriptor overlaps or exceeds native range"));
    }
    Ok(descriptor)
}

fn secret_error(detail: &'static str) -> MacosError {
    MacosError::new(
        MacosErrorKind::PreparationMismatch,
        MacosOperation::Validate,
        RecoveryAction::Reauthorize,
        detail,
    )
}

#[cfg(test)]
mod tests {
    use peritus_process::NativeProtectedHandle;
    use peritus_sandbox::{EnvironmentName, SecretReference};
    use peritus_types::{ResourceId, Sha256Digest};

    use super::{
        ProtectedSecretHandle, SecretHandleDescriptor, SecretHandleDestination,
        canonical_secret_handles,
    };

    fn protected(label: &str, descriptor_byte: u8) -> ProtectedSecretHandle {
        ProtectedSecretHandle::new(
            NativeProtectedHandle::from_bytes(label, vec![descriptor_byte; 8]).unwrap(),
            SecretReference::new(
                ResourceId::new([descriptor_byte; 16]).unwrap(),
                Sha256Digest::new([descriptor_byte; 32]),
            ),
            SecretHandleDestination::Environment(EnvironmentName::new("TOKEN").unwrap()),
        )
        .unwrap()
    }

    #[test]
    fn protected_secret_metadata_round_trips_without_payload_bytes() {
        let protected = protected("secret-one", 7);
        let descriptor = protected.manifest_descriptor().unwrap();
        let mut writer = crate::canonical::Writer::new();
        descriptor.encode(&mut writer).unwrap();
        let bytes = writer.finish();
        assert!(!bytes.windows(8).any(|window| window == [7; 8]));
        let mut reader = crate::canonical::Reader::new(&bytes).unwrap();
        let decoded = SecretHandleDescriptor::decode(&mut reader).unwrap();
        reader.finish().unwrap();
        assert_eq!(decoded, descriptor);
        assert_eq!(decoded.payload_len(), 8);
    }

    #[test]
    fn canonical_secret_handles_reject_duplicate_destinations() {
        let left = protected("secret-left", 7);
        let right = protected("secret-right", 8);
        assert!(canonical_secret_handles(vec![left, right]).is_err());
    }
}
