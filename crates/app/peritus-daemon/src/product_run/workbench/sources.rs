//! Atomic admission of completed source-backed user messages.

use super::{
    AppResponsePayload, ProductRunService, WorkbenchCommand, WorkbenchIntent, WorkbenchReceipt,
    domain_operation, error_value, receipt_projection,
};
use crate::AuthorityHandle;
use peritus_artifact_store::{ArtifactDigest, ArtifactStore, StoreConfig};
use peritus_app_protocol::{AppErrorCode, AppProtocolError, WorkbenchQueueIntent};
use peritus_journal::ApplicationArtifact;
use peritus_types::ActorId;

impl ProductRunService {
    pub(crate) async fn confirm_workbench_request_source(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        if let Err(error) = self.ensure_workspace_available(command.query().workspace()) {
            return error.response();
        }
        let service = self.clone();
        let authority = authority.clone();
        let command = command.clone();
        Self::await_blocking_future("admit complete workbench request source", move || {
            async move { service.confirm_request_source_owned(authority, actor, command).await }
        })
        .await
        .unwrap_or_else(|_| AppResponsePayload::Error(protocol_error(AppErrorCode::Internal)))
    }

    async fn confirm_request_source_owned(
        &self,
        authority: AuthorityHandle,
        actor: ActorId,
        command: WorkbenchCommand,
    ) -> AppResponsePayload {
        self.confirm_request_source(&authority, actor, &command)
            .await
            .map_or_else(AppResponsePayload::Error, AppResponsePayload::WorkbenchReceipt)
    }

    async fn confirm_request_source(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> Result<WorkbenchReceipt, AppProtocolError> {
        let WorkbenchIntent::Queue(queue) = command.intent() else {
            return Err(protocol_error(AppErrorCode::MalformedFrame));
        };
        if !super::inputs::is_source_intent(queue) {
            return Err(protocol_error(AppErrorCode::MalformedFrame));
        }
        self.control_workspace(command.query()).map_err(error_value)?;
        let operation = domain_operation(actor, command).map_err(error_value)?;
        if let Some(receipt) = self
            .with_control_conversation(operation.conversation(), |store| {
                store.resolve(&operation)
            })
            .map_err(error_value)?
        {
            return receipt_projection(command, &receipt).map_err(error_value);
        }
        let source = protocol_source(queue)?;
        if source.artifact().as_bytes() != command.operation().as_bytes() {
            return Err(protocol_error(AppErrorCode::InvalidIdentifier));
        }
        let scope = self.image_scope(actor, command.query(), command.expected_revision())?;
        let catalog = authority
            .authorize_scoped_text(scope, source.artifact())
            .await
            .map_err(super::images::daemon_error)?;
        if catalog.digest() != source.digest() || catalog.byte_size() != source.bytes() {
            return Err(protocol_error(AppErrorCode::MalformedFrame));
        }
        validate_request_source_artifact(&self.inner.request_source_artifacts, &catalog)?;
        if !authority
            .status()
            .await
            .map_err(super::images::daemon_error)?
            .mutation_ready()
        {
            return Err(protocol_error(AppErrorCode::ReadOnly));
        }
        let receipt = self
            .with_control_conversation(operation.conversation(), |store| {
                store.accept_request_source(&operation)
            })
            .map_err(error_value)?;
        receipt_projection(command, &receipt).map_err(error_value)
    }
}

pub(super) fn validate_request_source_artifact(
    config: &StoreConfig,
    catalog: &ApplicationArtifact,
) -> Result<(), AppProtocolError> {
    validate_text_artifact(config, catalog, false, true)
}

pub(super) fn validate_file_source_artifact(
    config: &StoreConfig,
    catalog: &ApplicationArtifact,
) -> Result<(), AppProtocolError> {
    validate_text_artifact(config, catalog, true, false)
}

fn validate_text_artifact(
    config: &StoreConfig,
    catalog: &ApplicationArtifact,
    allow_carriage_return: bool,
    require_non_whitespace: bool,
) -> Result<(), AppProtocolError> {
    let digest = ArtifactDigest::from_sha256(catalog.digest());
    let mut reader = ArtifactStore::open_existing(config, digest)
        .map_err(|_| protocol_error(AppErrorCode::NotReady))?;
    if reader.metadata().size() != catalog.byte_size() || reader.metadata().digest() != digest {
        return Err(protocol_error(AppErrorCode::InvalidIdentifier));
    }
    let mut carry = Vec::with_capacity(4);
    let mut non_whitespace = false;
    while let Some(chunk) = reader
        .read_chunk(64 * 1024)
        .map_err(|_| protocol_error(AppErrorCode::NotReady))?
    {
        validate_text_chunk(
            &mut carry,
            chunk.bytes(),
            &mut non_whitespace,
            allow_carriage_return,
        )?;
    }
    if !carry.is_empty() || (require_non_whitespace && !non_whitespace) {
        return Err(protocol_error(AppErrorCode::InvalidIdentifier));
    }
    Ok(())
}

fn validate_text_chunk(
    carry: &mut Vec<u8>,
    bytes: &[u8],
    non_whitespace: &mut bool,
    allow_carriage_return: bool,
) -> Result<(), AppProtocolError> {
    carry.extend_from_slice(bytes);
    match std::str::from_utf8(carry) {
        Ok(text) => {
            validate_text_chars(text, non_whitespace, allow_carriage_return)?;
            carry.clear();
        }
        Err(error) => {
            if error.error_len().is_some() {
                return Err(protocol_error(AppErrorCode::InvalidIdentifier));
            }
            let valid = error.valid_up_to();
            let text = std::str::from_utf8(&carry[..valid])
                .map_err(|_| protocol_error(AppErrorCode::InvalidIdentifier))?;
            validate_text_chars(text, non_whitespace, allow_carriage_return)?;
            carry.drain(..valid);
            if carry.len() > 3 {
                return Err(protocol_error(AppErrorCode::InvalidIdentifier));
            }
        }
    }
    Ok(())
}

fn validate_text_chars(
    text: &str,
    non_whitespace: &mut bool,
    allow_carriage_return: bool,
) -> Result<(), AppProtocolError> {
    for character in text.chars() {
        if character.is_control()
            && character != '\n'
            && character != '\t'
            && !(allow_carriage_return && character == '\r')
        {
            return Err(protocol_error(AppErrorCode::InvalidIdentifier));
        }
        *non_whitespace |= !character.is_whitespace();
    }
    Ok(())
}

fn protocol_source(
    queue: &WorkbenchQueueIntent,
) -> Result<peritus_app_protocol::WorkbenchInputSource, AppProtocolError> {
    match queue {
        WorkbenchQueueIntent::EnqueueSource { source, .. }
        | WorkbenchQueueIntent::EditSource { source, .. }
        | WorkbenchQueueIntent::CorrectSource { source, .. } => Ok(*source),
        _ => Err(protocol_error(AppErrorCode::MalformedFrame)),
    }
}

const fn protocol_error(code: AppErrorCode) -> AppProtocolError {
    AppProtocolError::new(code, None)
}
