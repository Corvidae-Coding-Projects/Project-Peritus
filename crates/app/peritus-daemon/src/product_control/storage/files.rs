//! Atomic file-version bytes, source metadata and exact preview/admission proof retention.

use super::{ControlError, ControlOperation, ControlReceipt, ControlStore, Error};
use peritus_journal::StateInstall;
use peritus_product_runner::{
    attachment::ValidatedFileText,
    control::{ControlIntent, FileVersion},
};

const FILE_NAMESPACE: u16 = 3409;
const CONSENT_NAMESPACE: u16 = 3410;

impl ControlStore {
    /// Publishes authorized exact selected bytes and their preview/admission proof together.
    /// The caller must verify selected workspace, path policy and consent before this operation.
    /// This function never opens a path, starts inference, or grants future read authority.
    pub fn accept_file(
        &mut self,
        operation: &ControlOperation,
        text: &ValidatedFileText,
        consent: Vec<u8>,
    ) -> Result<ControlReceipt, Error> {
        let version = version(operation)?;
        if version.operation() != operation.id() || !version.observation().matches(text) {
            return Err(ControlError::InvalidInput.into());
        }
        verify_consent(version, &consent)?;
        if let Some(receipt) = self.resolve(operation)? {
            return Ok(receipt);
        }
        let artifacts =
            [(FILE_NAMESPACE, text.text().as_bytes().to_vec()), (CONSENT_NAMESPACE, consent)]
                .into_iter()
                .map(|(namespace, bytes)| {
                    StateInstall::new(namespace, operation.id().as_bytes().to_vec(), None, 1, bytes)
                        .map_err(Error::from)
                })
                .collect::<Result<_, _>>()?;
        self.accept_installs(operation, artifacts)
    }

    pub(super) fn verify_file_archive(
        &self,
        operation: &ControlOperation,
        position: u64,
    ) -> Result<(), Error> {
        let version = version(operation)?;
        let artifact = self
            .journal
            .state_record(FILE_NAMESPACE, version.operation().as_bytes())?
            .ok_or(Error::Corrupt("file version artifact missing"))?;
        let consent = self
            .journal
            .state_record(CONSENT_NAMESPACE, version.operation().as_bytes())?
            .ok_or(Error::Corrupt("file version consent archive missing"))?;
        if artifact.revision() != 1
            || consent.revision() != 1
            || artifact.producing_position() != position
            || consent.producing_position() != position
        {
            return Err(Error::Corrupt("file version was not published atomically with consent"));
        }
        verify_text(version, artifact.bytes())?;
        verify_consent(version, consent.bytes())
    }

    // The caller first authenticates the conversation and validates its immutable history.
    pub(in crate::product_control) fn file_text(
        &self,
        version: &FileVersion,
    ) -> Result<ValidatedFileText, Error> {
        let artifact = self
            .journal
            .state_record(FILE_NAMESPACE, version.operation().as_bytes())?
            .ok_or(Error::Corrupt("selected file version artifact missing"))?;
        verify_text(version, artifact.bytes())
    }
}

fn version(operation: &ControlOperation) -> Result<&FileVersion, Error> {
    match operation.intent() {
        ControlIntent::AttachFile { file, .. } => Ok(file.initial()),
        ControlIntent::RefreshFile { version, .. } => Ok(version),
        _ => Err(ControlError::InvalidInput.into()),
    }
}
fn verify_text(version: &FileVersion, bytes: &[u8]) -> Result<ValidatedFileText, Error> {
    let text = ValidatedFileText::new(bytes.to_vec())?;
    if !version.observation().matches(&text) {
        return Err(Error::Corrupt("file artifact differs from its exact selected-byte binding"));
    }
    Ok(text)
}
fn verify_consent(version: &FileVersion, bytes: &[u8]) -> Result<(), Error> {
    if bytes.is_empty()
        || bytes.len() > 16 * 1024
        || peritus_codec::sha256(bytes) != version.consent_digest()
    {
        return Err(Error::Corrupt("file version proof differs from its immutable binding"));
    }
    Ok(())
}
