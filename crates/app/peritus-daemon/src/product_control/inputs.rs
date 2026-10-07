//! Host-only request binding; clients cannot manufacture an incorporation grant.

use super::{ControlStore, ControlStoreError as Error};
use peritus_product_runner::control::{
    ControlError, ControlIntent, ControlOperation, ControlReceipt, ConversationId, InputCapture,
    InvocationId, MAX_REQUEST_CONTEXT_BYTES, OperationId, QueueIntent,
};
use peritus_types::{ActorId, WorkspaceId};

#[cfg(test)]
pub(super) mod tests;

mod archive;
mod files;
mod manifest;
mod requests;
pub(crate) use requests::{RequestSourceBody, RequestSourceSnapshot, read_verified_text};
pub(super) use archive::inspect::inspect_manifest;
pub(super) use archive::{
    RequestArchive, manifest_source_revision, validate_archive, verify_manifest,
};

/// Authenticated read-only input capture. Private fields prevent constructing a false view.
#[derive(Clone, Debug)]
pub struct CapturedConversation {
    conversation: ConversationId,
    actor: ActorId,
    workspace: WorkspaceId,
    source_revision: u64,
    source_incorporated: bool,
    revision: u64,
    inputs: InputCapture,
    user_context: String,
    replies: Vec<peritus_product_runner::control::PublicReplyReference>,
    sources: Vec<manifest::InputSource>,
    brief: Vec<peritus_product_runner::control::BriefBinding>,
    image_sources: Vec<peritus_product_runner::control::ImageAttachment>,
    images: Vec<peritus_model_protocol::MediaInput>,
    file_sources: Vec<manifest::FileSource>,
    guidance: peritus_app_protocol::WorkbenchGuidanceRender,
}

type FileReaderSlot = std::sync::Arc<
    std::sync::Mutex<Option<peritus_artifact_store::ArtifactReadHandle>>,
>;

/// Authenticated artifact readers owned by one exact selected file scope.
///
/// The live conversation retains this separately from `CapturedConversation` so non-file input
/// generations can advance without reopening unchanged immutable file versions.
pub(crate) struct CapturedFileReaders {
    conversation: ConversationId,
    actor: ActorId,
    workspace: WorkspaceId,
    file_sources: Vec<manifest::FileSource>,
    readers: Vec<FileReaderSlot>,
}

impl CapturedConversation {
    /// Returns the authenticated conversation owner of this exact capture.
    #[must_use]
    pub(crate) const fn conversation(&self) -> ConversationId {
        self.conversation
    }
    /// Returns the authenticated actor owner of this exact capture.
    #[must_use]
    pub(crate) const fn actor(&self) -> ActorId {
        self.actor
    }
    /// Returns the authenticated workspace owner of this exact capture.
    #[must_use]
    pub(crate) const fn workspace(&self) -> WorkspaceId {
        self.workspace
    }
    /// Returns the aggregate revision governing this request candidate.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Returns the immutable aggregate revision from which request content was selected.
    #[must_use]
    pub const fn source_revision(&self) -> u64 {
        self.source_revision
    }
    /// Borrows the exact input candidate, without granting permission to execute it.
    #[must_use]
    pub const fn inputs(&self) -> &InputCapture {
        &self.inputs
    }
    /// Borrows only current user-authored input, excluding public replies and host guidance.
    #[must_use]
    pub fn reference_authority_context(&self) -> &str {
        &self.user_context
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
        if conversation.len() > MAX_REQUEST_CONTEXT_BYTES {
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

    /// Captures the exact historical control revision sealed by a continuation receipt while
    /// fencing any incorporation append against the current aggregate revision.
    pub fn capture_execution_revision(
        &self,
        start: &ControlOperation,
        source_revision: u64,
    ) -> Result<CapturedConversation, Error> {
        let current = self.execution_record(start)?;
        let source = self
            .load_revision(start.conversation(), source_revision)?
            .ok_or(ControlError::NotFound)?;
        let expected = current.execution().ok_or(ControlError::NotFound)?;
        let selected = source.execution().ok_or(ControlError::NotFound)?;
        if selected.run_bytes() != expected.run_bytes()
            || selected.start_operation() != expected.start_operation()
            || selected.settings_digest() != expected.settings_digest()
        {
            return Err(ControlError::ScopeMismatch.into());
        }
        self.capture_record(
            source,
            ActorId::new(*start.actor_bytes()).map_err(|_| ControlError::InvalidInput)?,
            WorkspaceId::new(*start.workspace_bytes()).map_err(|_| ControlError::InvalidInput)?,
            source_revision,
            false,
            current.revision(),
        )
    }

    /// Captures the same sealed continuation source after its pending items were incorporated,
    /// without admitting newer queued input into later requests of the same attempt.
    pub fn capture_execution_revision_incorporated(
        &self,
        start: &ControlOperation,
        source_revision: u64,
    ) -> Result<CapturedConversation, Error> {
        let current = self.execution_record(start)?;
        let source = self
            .load_revision(start.conversation(), source_revision)?
            .ok_or(ControlError::NotFound)?;
        let expected = current.execution().ok_or(ControlError::NotFound)?;
        let selected = source.execution().ok_or(ControlError::NotFound)?;
        if selected.run_bytes() != expected.run_bytes()
            || selected.start_operation() != expected.start_operation()
            || selected.settings_digest() != expected.settings_digest()
        {
            return Err(ControlError::ScopeMismatch.into());
        }
        let source = incorporated_source_view(source)?;
        self.capture_record(
            source,
            ActorId::new(*start.actor_bytes()).map_err(|_| ControlError::InvalidInput)?,
            WorkspaceId::new(*start.workspace_bytes()).map_err(|_| ControlError::InvalidInput)?,
            source_revision,
            true,
            current.revision(),
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
        self.prepare_execution_capture(start, captured, request)
    }

    /// Binds a provider request to the immutable source revision already owned by an accepted
    /// continuation. Newer queued input remains pending for a later request.
    pub fn prepare_execution_revision(
        &mut self,
        start: &ControlOperation,
        source_revision: u64,
        generation: u64,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<InputAdmission, Error> {
        let captured = self.capture_execution_revision(start, source_revision)?;
        if captured.inputs().generation() != generation {
            return Err(Error::Corrupt("continuation source generation changed"));
        }
        self.prepare_execution_capture(start, captured, request)
    }

    /// Binds a later provider request from the same continuation attempt to the sealed
    /// post-incorporation view, still excluding input accepted after the continuation receipt.
    pub fn prepare_execution_revision_incorporated(
        &mut self,
        start: &ControlOperation,
        source_revision: u64,
        generation: u64,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<InputAdmission, Error> {
        let captured = self.capture_execution_revision_incorporated(start, source_revision)?;
        if captured.inputs().generation() != generation {
            return Err(Error::Corrupt("continuation source generation changed"));
        }
        self.prepare_execution_capture(start, captured, request)
    }

    fn prepare_execution_capture(
        &mut self,
        start: &ControlOperation,
        captured: CapturedConversation,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<InputAdmission, Error> {
        let mut identity = b"peritus-workbench/invocation/v1".to_vec();
        identity.extend_from_slice(start.id().as_bytes());
        identity.extend_from_slice(&captured.source_revision().to_be_bytes());
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
        let revision = record.revision();
        self.capture_record(record, actor, workspace, revision, false, revision)
    }

    fn capture_record(
        &self,
        record: peritus_product_runner::control::ConversationRecord,
        actor: ActorId,
        workspace: WorkspaceId,
        source_revision: u64,
        source_incorporated: bool,
        revision: u64,
    ) -> Result<CapturedConversation, Error> {
        if record.owner_bytes() != actor.as_bytes()
            || record.workspace_bytes() != workspace.as_bytes()
        {
            return Err(ControlError::ScopeMismatch.into());
        }
        let user_capture = record.inputs().capture()?;
        let user_context = inline_user_context(record.inputs(), &user_capture)?;
        let reply_bytes = record
            .replies()
            .iter()
            .map(|reply| (reply.after_invocation(), reply.bytes()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let inline = record.inline_reply_sources(&reply_bytes, true)?;
        let replies = record
            .replies()
            .iter()
            .filter(|reference| inline.contains(&reference.after_invocation()))
            .map(|reference| {
                self.reply_text(reference).map(|text| (reference.after_invocation(), text))
            })
            .collect::<Result<std::collections::BTreeMap<_, _>, _>>()?;
        let inputs = record.capture_with_reply_sources(&reply_bytes, &replies, true)?;
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
                    .map(|revision| {
                        manifest::InputSource::from_revision(
                            revision,
                            inputs.out_of_line().contains(selection),
                        )
                    })
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
        let images =
            image_sources.iter().map(|image| self.image_media(image)).collect::<Result<_, _>>()?;
        let file_sources = record
            .eligible_files(inputs.included())
            .into_iter()
            .map(manifest::FileSource::selected)
            .collect::<Vec<_>>();
        let app_conversation =
            peritus_app_protocol::ConversationId::new(*record.id().as_bytes())
            .map_err(|_| ControlError::InvalidInput)?;
        let guidance = self.guidance_for_request(workspace, app_conversation)?;
        if inputs.conversation().len().saturating_add(guidance.text().len())
            > MAX_REQUEST_CONTEXT_BYTES
        {
            return Err(ControlError::Capacity.into());
        }
        Ok(CapturedConversation {
            conversation: record.id(),
            actor,
            workspace,
            source_revision,
            source_incorporated,
            revision,
            inputs,
            user_context,
            replies,
            sources,
            brief,
            image_sources,
            images,
            file_sources,
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

fn inline_user_context(
    ledger: &peritus_product_runner::control::InputLedger,
    captured: &InputCapture,
) -> Result<String, Error> {
    let mut context = String::new();
    for selection in captured.included() {
        if captured.out_of_line().contains(selection) {
            continue;
        }
        let revision = ledger
            .revisions()
            .iter()
            .find(|revision| revision.selection() == *selection)
            .ok_or(Error::Corrupt("captured authority revision missing"))?;
        if !context.is_empty() {
            context.push_str("\n\n");
        }
        context.push_str("User: ");
        context.push_str(revision.text());
    }
    Ok(context)
}

pub(super) fn incorporated_source_view(
    source: peritus_product_runner::control::ConversationRecord,
) -> Result<peritus_product_runner::control::ConversationRecord, Error> {
    let items = source.inputs().capture()?.pending().to_vec();
    if items.is_empty() {
        return Err(Error::Corrupt("continuation source has no pending input"));
    }
    let execution = source.execution().ok_or(ControlError::NotFound)?;
    let mut identity = b"peritus-workbench/sealed-continuation-view/v1".to_vec();
    identity.extend_from_slice(execution.start_operation().as_bytes());
    identity.extend_from_slice(&source.revision().to_be_bytes());
    let digest = peritus_codec::sha256(&identity);
    let digest_bytes = digest.into_bytes();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest_bytes[..16]);
    bytes[0] |= 1;
    let operation = ControlOperation::new(
        OperationId::new(bytes)?,
        source.id(),
        ActorId::new(*source.owner_bytes()).map_err(|_| ControlError::InvalidInput)?,
        WorkspaceId::new(*source.workspace_bytes()).map_err(|_| ControlError::InvalidInput)?,
        source.revision(),
        ControlIntent::Queue(QueueIntent::Incorporate {
            invocation: InvocationId::new(bytes)?,
            request_digest: digest_bytes,
            manifest_digest: digest_bytes,
            items,
        }),
    );
    peritus_product_runner::control::ConversationRecord::apply(Some(&source), &operation)
        .map(|(record, _)| record)
        .map_err(Into::into)
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
