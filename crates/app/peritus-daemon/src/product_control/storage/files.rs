//! Atomic file-version bytes, source metadata and exact preview/admission proof retention.

use super::{ControlError, ControlOperation, ControlReceipt, ControlStore, Error};
use peritus_journal::StateInstall;
use peritus_product_runner::{
    attachment::ValidatedFileText,
    control::{ControlIntent, FileVersion},
};
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

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
        self.accept_files(operation, &[(text, consent.as_slice())])
    }

    /// Publishes an entire message or refresh snapshot in one event and one receipt.
    pub(crate) fn accept_files(
        &mut self,
        operation: &ControlOperation,
        sources: &[(&ValidatedFileText, &[u8])],
    ) -> Result<ControlReceipt, Error> {
        let versions = versions(operation)?;
        if versions.len() != sources.len() {
            return Err(ControlError::InvalidInput.into());
        }
        for (version, (text, consent)) in versions.iter().zip(sources) {
            if !version.observation().matches(text) {
                return Err(ControlError::InvalidInput.into());
            }
            verify_consent(version, consent)?;
        }
        if let Some(receipt) = self.resolve(operation)? {
            return Ok(receipt);
        }
        let mut artifacts = Vec::new();
        for (version, (text, consent)) in versions.iter().zip(sources) {
            for (namespace, bytes) in
                [(FILE_NAMESPACE, text.text().as_bytes()), (CONSENT_NAMESPACE, *consent)]
            {
                artifacts.push(StateInstall::new(
                    namespace,
                    version.operation().as_bytes().to_vec(),
                    None,
                    1,
                    bytes.to_vec(),
                )?);
            }
        }
        artifacts.sort_by(|left, right| {
            (left.namespace(), left.key()).cmp(&(right.namespace(), right.key()))
        });
        self.accept_installs(operation, artifacts)
    }

    pub(super) fn verify_file_archive(
        &self,
        operation: &ControlOperation,
        position: u64,
    ) -> Result<(), Error> {
        for version in versions(operation)? {
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
                return Err(Error::Corrupt(
                    "file version was not published atomically with consent",
                ));
            }
            verify_text(version, artifact.bytes())?;
            verify_consent(version, consent.bytes())?;
        }
        Ok(())
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
        verify_text(version, artifact.bytes())?;
        Ok(ValidatedFileText::new(artifact.bytes().to_vec())?)
    }
}

fn versions(operation: &ControlOperation) -> Result<Vec<&FileVersion>, Error> {
    let versions = match operation.intent() {
        ControlIntent::AttachFile { file, .. } if file.operation() == operation.id() => {
            vec![file.initial()]
        }
        ControlIntent::RefreshFile { version, .. }
        | ControlIntent::AcceptBriefProposal { version, .. }
            if version.operation() == operation.id() =>
        {
            vec![version]
        }
        ControlIntent::SubmitMessage { files, .. } => {
            files.iter().map(peritus_product_runner::control::FileAttachment::initial).collect()
        }
        ControlIntent::RefreshFiles { versions } => {
            versions.iter().map(|(_, _, version)| version).collect()
        }
        _ => return Err(ControlError::InvalidInput.into()),
    };
    let mut seen = std::collections::BTreeSet::new();
    if versions.iter().any(|version| !seen.insert(version.operation())) {
        return Err(ControlError::InvalidInput.into());
    }
    Ok(versions)
}

fn verify_text(version: &FileVersion, bytes: &[u8]) -> Result<(), Error> {
    let digest = ValidatedFileText::validate_bytes(bytes)?;
    if bytes.len() as u64 != version.observation().bytes()
        || digest != version.observation().digest()
    {
        return Err(Error::Corrupt("file artifact differs from its exact selected-byte binding"));
    }
    Ok(())
}
fn verify_consent(version: &FileVersion, bytes: &[u8]) -> Result<(), Error> {
    if bytes.is_empty() || digest_content(bytes) != version.consent_digest() {
        return Err(Error::Corrupt("file version proof differs from its immutable binding"));
    }
    Ok(())
}

fn digest_content(bytes: &[u8]) -> Sha256Digest {
    let mut digest = Sha256::new();
    for chunk in bytes.chunks(64 * 1024) {
        digest.update(chunk);
    }
    Sha256Digest::new(digest.finalize().into())
}
