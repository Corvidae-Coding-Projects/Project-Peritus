//! Exact checked secret-delivery bindings for process-owned protected handles.

use crate::{InheritedHandle, LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery};
use peritus_process::NativeProtectedHandle;
use peritus_sandbox::{SecretDelivery, SecretRequirement};
use peritus_secrets::{DeliveryArtifact, SecretDeliverySession};

/// One opaque protected payload bound to one exact checked secret requirement.
///
/// Payload bytes remain owned by [`NativeProtectedHandle`] and are omitted from this type's
/// ordinary representation. The binding carries only the checked reference/destination and the
/// nonsensitive handle label, number, and length into the helper manifest.
#[derive(Clone, Debug)]
pub struct LinuxProtectedPayload {
    requirement: SecretRequirement,
    handle: NativeProtectedHandle,
    payload_len: usize,
}

impl LinuxProtectedPayload {
    /// Binds one process-owned anonymous handle to one exact checked secret requirement.
    ///
    /// # Errors
    /// Rejects a handle that cannot be represented as a Linux descriptor or bounded manifest
    /// label.
    pub fn new(
        requirement: SecretRequirement,
        handle: NativeProtectedHandle,
    ) -> Result<Self, LinuxError> {
        let raw = handle.raw_handle();
        let payload_len = handle
            .payload_len()
            .ok_or_else(|| secret_error("secret binding requires a finite protected payload"))?;
        if raw < 3
            || raw > i32::MAX as u64
            || handle.label().len() > 128
            || !handle.label().bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err(secret_error("protected payload handle is not a bounded Linux descriptor"));
        }
        Ok(Self { requirement, handle, payload_len })
    }

    /// Returns the exact checked reference and destination.
    #[must_use]
    pub const fn requirement(&self) -> &SecretRequirement {
        &self.requirement
    }

    /// Returns the opaque process-owned handle.
    #[must_use]
    pub const fn handle(&self) -> &NativeProtectedHandle {
        &self.handle
    }

    /// Returns the exact finite payload length without exposing its bytes.
    #[must_use]
    pub const fn payload_len(&self) -> usize {
        self.payload_len
    }

    pub(crate) fn manifest_handle(&self) -> Result<InheritedHandle, LinuxError> {
        InheritedHandle::new(self.handle.raw_handle(), self.handle.label().to_owned())
    }
}

/// Validates and canonicalizes already authorized protected payload bindings.
///
/// # Errors
/// Rejects duplicate requirements, destinations, labels, or operating-system descriptors.
pub fn canonical_payloads(
    mut payloads: Vec<LinuxProtectedPayload>,
) -> Result<Vec<LinuxProtectedPayload>, LinuxError> {
    payloads.sort_by(|left, right| left.requirement.cmp(&right.requirement));
    for (index, payload) in payloads.iter().enumerate() {
        if payloads[..index].iter().any(|prior| {
            prior.requirement == payload.requirement
                || prior.requirement.delivery() == payload.requirement.delivery()
                || prior.handle.label() == payload.handle.label()
                || prior.handle.raw_handle() == payload.handle.raw_handle()
        }) {
            return Err(secret_error(
                "protected payload requirement, destination, label, or descriptor collides",
            ));
        }
    }
    Ok(payloads)
}

pub fn payloads_from_session(
    session: &SecretDeliverySession,
    requirements: &[SecretRequirement],
) -> Result<Vec<LinuxProtectedPayload>, LinuxError> {
    if session.artifacts().len() != requirements.len() {
        return Err(secret_error("prepared secret artifacts differ from checked requirements"));
    }
    let mut payloads = Vec::with_capacity(requirements.len());
    for (index, (requirement, artifact)) in requirements.iter().zip(session.artifacts()).enumerate()
    {
        let kind = match requirement.delivery() {
            SecretDelivery::Environment(_) => "environment",
            SecretDelivery::File(_) => "file",
            SecretDelivery::BrokeredHandle(_) => "brokered",
        };
        let label = format!("peritus-secret-{kind}-v1-{index}");
        let handle = match (requirement.delivery(), artifact) {
            (SecretDelivery::Environment(expected), DeliveryArtifact::Environment { name, .. })
                if expected == name =>
            {
                artifact
                    .expose_environment(|_, bytes| {
                        NativeProtectedHandle::from_bytes(label, bytes.to_vec())
                    })
                    .transpose()
                    .map_err(|_| secret_error("protected environment handle creation failed"))?
                    .ok_or_else(|| secret_error("prepared environment artifact changed kind"))?
            }
            (
                SecretDelivery::BrokeredHandle(expected),
                DeliveryArtifact::Brokered { label, .. },
            ) if expected == label => artifact
                .expose_brokered(|_, bytes| {
                    NativeProtectedHandle::from_bytes(
                        format!("peritus-secret-brokered-v1-{index}"),
                        bytes.to_vec(),
                    )
                })
                .transpose()
                .map_err(|_| secret_error("protected brokered handle creation failed"))?
                .ok_or_else(|| secret_error("prepared brokered artifact changed kind"))?,
            (SecretDelivery::File(expected), DeliveryArtifact::File { .. }) => {
                let (staging, actual) = artifact
                    .file_paths()
                    .ok_or_else(|| secret_error("prepared file artifact changed kind"))?;
                if expected != actual {
                    return Err(secret_error("prepared file destination differs"));
                }
                let bytes = std::fs::read(staging)
                    .map_err(|_| secret_error("private staged secret file could not be read"))?;
                NativeProtectedHandle::from_bytes(label, bytes)
                    .map_err(|_| secret_error("protected file handle creation failed"))?
            }
            _ => return Err(secret_error("prepared secret artifact destination differs")),
        };
        payloads.push(LinuxProtectedPayload::new(requirement.clone(), handle)?);
    }
    canonical_payloads(payloads)
}

/// Validates and canonicalizes generic already-authorized protected handles.
///
/// # Errors
/// Rejects descriptor or label collisions.
pub fn canonical_handles(
    mut handles: Vec<InheritedHandle>,
) -> Result<Vec<InheritedHandle>, LinuxError> {
    handles.sort();
    if handles.windows(2).any(|pair| {
        pair[0].descriptor() == pair[1].descriptor() || pair[0].label() == pair[1].label()
    }) {
        return Err(secret_error("protected secret handles collide"));
    }
    Ok(handles)
}

fn secret_error(detail: &'static str) -> LinuxError {
    LinuxError::new(
        LinuxErrorKind::PreparationMismatch,
        LinuxOperation::Prepare,
        LinuxRecovery::CorrectRequest,
        detail,
    )
}
