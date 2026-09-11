//! Host-only request binding; clients cannot manufacture an incorporation grant.

use super::{ControlStore, ControlStoreError as Error};
use peritus_product_runner::control::{
    ControlError, ControlIntent, ControlOperation, ControlReceipt, ConversationId, InputCapture,
    InvocationId, OperationId, QueueIntent,
};
use peritus_types::{ActorId, WorkspaceId};

#[cfg(test)]
pub(super) mod tests;

mod archive;
mod files;
mod manifest;
pub(super) use archive::inspect::inspect_manifest;
pub(super) use archive::{RequestArchive, validate_archive, verify_manifest};

/// Authenticated read-only input capture. Private fields prevent constructing a false view.
#[derive(Clone, Debug)]
pub struct CapturedConversation {
    conversation: ConversationId,
    actor: ActorId,
    workspace: WorkspaceId,
    revision: u64,
    inputs: InputCapture,
    replies: Vec<peritus_product_runner::control::PublicReplyReference>,
    sources: Vec<manifest::InputSource>,
    brief: Vec<peritus_product_runner::control::BriefBinding>,
    image_sources: Vec<peritus_product_runner::control::ImageAttachment>,
    images: Vec<peritus_model_protocol::MediaInput>,
    file_sources: Vec<manifest::FileSource>,
    file_context: String,
    guidance: peritus_app_protocol::WorkbenchGuidanceRender,
}
impl CapturedConversation {
    /// Returns the aggregate revision governing this request candidate.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Borrows the exact input candidate, without granting permission to execute it.
    #[must_use]
    pub const fn inputs(&self) -> &InputCapture {
        &self.inputs
    }
    /// Borrows the complete explicitly selected immutable image input, in source order.
    #[must_use]
    pub fn images(&self) -> &[peritus_model_protocol::MediaInput] {
        &self.images
    }

    /// Renders current exact inputs plus separately delimited user-approved project guidance.
    pub fn conversation_with_guidance(&self) -> Result<String, Error> {
        let mut conversation = self.inputs.conversation().to_owned();
        if !self.guidance.identities().is_empty() {
            if !conversation.is_empty() {
                conversation.push_str("\n\n");
            }
            conversation.push_str(self.guidance.text());
        }
        if conversation.len() > 1024 * 1024 {
            return Err(ControlError::Capacity.into());
        }
        Ok(conversation)
    }
}

/// Durable request-boundary outcome, distinct from a provider-send or completion receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputAdmission {
    /// Exact request/input binding is committed. This does not assert the provider received it.
    Accepted(ControlReceipt),
    /// Rebuild the request from a fresh capture; no incorporation was committed.
    Stale,
}

impl ControlStore {
    /// Verifies that a run's staged binding was accepted before exposing execution input.
    pub fn capture_execution(
        &self,
        start: &ControlOperation,
    ) -> Result<CapturedConversation, Error> {
        self.execution_record(start)?;
        self.capture_inputs(
            start.conversation(),
            ActorId::new(*start.actor_bytes()).map_err(|_| ControlError::InvalidInput)?,
            WorkspaceId::new(*start.workspace_bytes()).map_err(|_| ControlError::InvalidInput)?,
        )
    }

    pub(crate) fn execution_record(
        &self,
        start: &ControlOperation,
    ) -> Result<peritus_product_runner::control::ConversationRecord, Error> {
        let (run, settings_digest) = match start.intent() {
            ControlIntent::StartExecution { run, settings_digest }
            | ControlIntent::StartGoal { run, settings_digest, .. } => (run, settings_digest),
            _ => return Err(ControlError::InvalidInput.into()),
        };
        if self.resolve(start)?.is_none() {
            return Err(ControlError::NotFound.into());
        }
        let record = self.load(start.conversation())?.ok_or(ControlError::NotFound)?;
        let bound = record.execution().ok_or(ControlError::NotFound)?;
        if bound.run_bytes() != run
            || bound.start_operation() != start.id()
            || bound.settings_digest().as_bytes() != settings_digest
        {
            return Err(ControlError::ScopeMismatch.into());
        }
        Ok(record)
    }

    /// Binds one new provider invocation against an accepted run and the inspected input generation.
    pub fn prepare_execution(
        &mut self,
        start: &ControlOperation,
        generation: u64,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<InputAdmission, Error> {
        let captured = self.capture_execution(start)?;
        if captured.inputs().generation() != generation {
            return Ok(InputAdmission::Stale);
        }
        let mut identity = b"peritus-workbench/invocation/v1".to_vec();
        identity.extend_from_slice(start.id().as_bytes());
        identity.extend_from_slice(&captured.revision().to_be_bytes());
        identity.extend_from_slice(request.request_id().expose_for_wire().as_bytes());
        let digest = peritus_codec::sha256(&identity);
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        bytes[0] |= 1;
        self.prepare_inputs(&captured, InvocationId::new(bytes)?, request)
    }
    /// Captures current inputs after checking the authenticated owner and exact workspace.
    /// Does not create absent state, start work, or consume any queue entries.
    pub fn capture_inputs(
        &self,
        conversation: ConversationId,
        actor: ActorId,
        workspace: WorkspaceId,
    ) -> Result<CapturedConversation, Error> {
        let record = self.load(conversation)?.ok_or(ControlError::NotFound)?;
        if record.owner_bytes() != actor.as_bytes()
            || record.workspace_bytes() != workspace.as_bytes()
        {
            return Err(ControlError::ScopeMismatch.into());
        }
        let metadata = record
            .replies()
            .iter()
            .map(|reply| (reply.after_invocation(), String::new()))
            .collect();
        let eligible = record.capture_with_replies(&metadata, true)?;
        let selected: Vec<_> = record
            .replies()
            .iter()
            .filter(|reply| eligible.public_replies().contains(&reply.after_invocation()))
            .collect();
        let bytes: u64 = selected.iter().map(|reply| reply.bytes().saturating_add(32)).sum();
        if bytes.saturating_add(eligible.conversation().len() as u64) > 1024 * 1024 {
            return Err(ControlError::Capacity.into());
        }
        let replies = selected
            .into_iter()
            .map(|reference| {
                self.reply_text(reference).map(|text| (reference.after_invocation(), text))
            })
            .collect::<Result<std::collections::BTreeMap<_, _>, _>>()?;
        let inputs = record.capture_with_replies(&replies, true)?;
        let replies = record
            .replies()
            .iter()
            .filter(|reply| inputs.public_replies().contains(&reply.after_invocation()))
            .cloned()
            .collect();
        let sources = inputs
            .included()
            .iter()
            .map(|selection| {
                record
                    .inputs()
                    .revisions()
                    .iter()
                    .find(|item| item.selection() == *selection)
                    .map(manifest::InputSource::from_revision)
                    .ok_or(Error::Corrupt("captured source revision missing"))
            })
            .collect::<Result<_, _>>()?;
        let brief = record
            .brief()
            .bindings()
            .iter()
            .filter(|binding| inputs.included().contains(&binding.selected()))
            .copied()
            .collect();
        let image_sources: Vec<_> =
            record.eligible_images(inputs.included()).into_iter().cloned().collect();
        if image_sources.len() > peritus_product_runner::attachment::MAX_IMAGE_COUNT
            || image_sources
                .iter()
                .try_fold(0_u64, |total, image| total.checked_add(image.bytes()))
                .is_none_or(|total| {
                    total > peritus_product_runner::attachment::MAX_IMAGE_SELECTION_BYTES
                })
        {
            return Err(ControlError::Capacity.into());
        }
        let images =
            image_sources.iter().map(|image| self.image_media(image)).collect::<Result<_, _>>()?;
        let file_sources = record
            .eligible_files(inputs.included())
            .into_iter()
            .map(manifest::FileSource::selected)
            .collect::<Vec<_>>();
        if file_sources.len() > peritus_product_runner::attachment::MAX_FILE_COUNT
            || file_sources
                .iter()
                .try_fold(0_u64, |total, file| {
                    total.checked_add(file.version.observation().bytes())
                })
                .is_none_or(|total| {
                    total > peritus_product_runner::attachment::MAX_FILE_SELECTION_BYTES
                })
        {
            return Err(ControlError::Capacity.into());
        }
        let file_context = self.file_context(&file_sources)?;
        let inputs = inputs.with_file_context(&file_context)?;
        let app_conversation = peritus_app_protocol::ConversationId::new(*conversation.as_bytes())
            .map_err(|_| ControlError::InvalidInput)?;
        let guidance = self.guidance_for_request(workspace, app_conversation)?;
        if inputs.conversation().len().saturating_add(guidance.text().len()) > 1024 * 1024 {
            return Err(ControlError::Capacity.into());
        }
        Ok(CapturedConversation {
            conversation,
            actor,
            workspace,
            revision: record.revision(),
            inputs,
            replies,
            sources,
            brief,
            image_sources,
            images,
            file_sources,
            file_context,
            guidance,
        })
    }

    /// Atomically binds the fully constructed request fingerprint and exact selected revisions.
    /// The caller holds the serialized control-store owner through this call and must await
    /// this result before invoking a provider. A retry uses the same capture, invocation and
    /// exact request, and receives its original receipt without incorporating newer input.
    pub fn prepare_inputs(
        &mut self,
        captured: &CapturedConversation,
        invocation: InvocationId,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<InputAdmission, Error> {
        let archive = RequestArchive::new(captured, invocation, request)?;
        let mut identity = b"peritus-workbench/request-incorporation/v1".to_vec();
        identity.extend_from_slice(invocation.as_bytes());
        identity.extend_from_slice(captured.conversation.as_bytes());
        let digest = peritus_codec::sha256(&identity);
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        bytes[0] |= 1;
        let operation = ControlOperation::new(
            OperationId::new(bytes)?,
            captured.conversation,
            captured.actor,
            captured.workspace,
            captured.revision,
            ControlIntent::Queue(QueueIntent::Incorporate {
                invocation,
                request_digest: archive.request_digest(),
                manifest_digest: archive.manifest_digest()?,
                items: captured.inputs.pending().to_vec(),
            }),
        );
        match self.accept_archived(&operation, Some(archive)) {
            Ok(receipt) => Ok(InputAdmission::Accepted(receipt)),
            Err(Error::Control(ControlError::StaleRevision)) => Ok(InputAdmission::Stale),
            Err(error) => Err(error),
        }
    }
}

impl InputAdmission {
    /// Maps durability to D0's decision without treating request binding as provider completion.
    pub const fn developer_admission(&self) -> peritus_agent::DeveloperRequestAdmission {
        match self {
            Self::Accepted(_) => peritus_agent::DeveloperRequestAdmission::Accepted,
            Self::Stale => peritus_agent::DeveloperRequestAdmission::Stale,
        }
    }
}
