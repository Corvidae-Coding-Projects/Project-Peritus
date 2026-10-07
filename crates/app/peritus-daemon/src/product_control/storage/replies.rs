//! Descriptor-only C0 publication and immutable artifact retention for public replies.

use super::{
    ControlError, ControlOperation, ControlReceipt, ControlStore, Error,
    REPLY_DESCRIPTOR_NAMESPACE, REPLY_NAMESPACE,
};
use peritus_artifact_store::{
    ArtifactDigest, ArtifactReadHandle, ArtifactStore, EncryptionMetadata, MediaType,
    ReferenceOwner, StoreConfig, WriteRequest,
};
use peritus_journal::StateInstall;
use peritus_product_runner::control::{ControlIntent, PublicReplyReference};
use peritus_types::{EventId, RunId, Sha256Digest};

use super::checkpoints::snapshot::{
    PublicationPlan, PublicationPurpose, publication_claim,
};

const REPLY_DESCRIPTOR_MAGIC: &[u8; 8] = b"pcreply1";

pub(crate) fn reply_publication_claim(
    operation: &ControlOperation,
    reference: &PublicReplyReference,
) -> Result<super::checkpoints::snapshot::PublicationClaim, Error> {
    if !matches!(operation.intent(), ControlIntent::PublishReply(stored) if stored == reference) {
        return Err(ControlError::IdempotencyConflict.into());
    }
    publication_claim(
        operation,
        REPLY_DESCRIPTOR_NAMESPACE,
        *operation.id().as_bytes(),
        &descriptor_bytes(reference),
        PublicationPurpose::ReplyDescriptor,
    )
}

pub(crate) struct PreparedPublicReply {
    operation: Option<ControlOperation>,
    reference: PublicReplyReference,
    run: RunId,
    config: StoreConfig,
}

impl PreparedPublicReply {
    #[must_use]
    pub(crate) const fn retained(&self) -> bool {
        self.operation.is_none()
    }

    #[must_use]
    pub(crate) const fn expected_revision(&self) -> Option<u64> {
        match &self.operation {
            Some(operation) => Some(operation.expected_revision()),
            None => None,
        }
    }

    #[must_use]
    pub(crate) const fn reference(&self) -> &PublicReplyReference {
        &self.reference
    }

    pub(crate) fn publish(mut self, text: &str) -> Result<PublishedPublicReply, Error> {
        let operation = self
            .operation
            .take()
            .ok_or(Error::Corrupt("retained public reply cannot be republished"))?;
        super::super::replies::verify_text(&self.reference, text.as_bytes())?;
        let artifact = spool_reply_artifact(&self.config, &self.reference, Some(text.as_bytes()))?;
        Ok(PublishedPublicReply {
            operation,
            reference: self.reference,
            artifact,
            run: self.run,
        })
    }
}

pub(crate) struct PublishedPublicReply {
    operation: ControlOperation,
    reference: PublicReplyReference,
    artifact: ArtifactDigest,
    run: RunId,
}

impl ControlStore {
    pub(crate) fn reply_artifact_config(root: &std::path::Path) -> Result<StoreConfig, Error> {
        StoreConfig::for_available_space(root.join("checkpoint-artifacts"), i64::MAX as u64)
            .map_err(|error| Error::Io(std::io::Error::other(error)))
    }

    pub(crate) fn prepare_public_reply(
        &mut self,
        start: &ControlOperation,
        digest: Sha256Digest,
        bytes: u64,
    ) -> Result<PreparedPublicReply, Error> {
        self.prepare_public_reply_inner(start, digest, bytes, None)
    }

    pub(crate) fn prepare_public_reply_for_generation(
        &mut self,
        start: &ControlOperation,
        digest: Sha256Digest,
        bytes: u64,
        expected_input_generation: u64,
    ) -> Result<PreparedPublicReply, Error> {
        self.prepare_public_reply_inner(
            start,
            digest,
            bytes,
            Some(expected_input_generation),
        )
    }

    pub(crate) fn prepare_public_reply_recovery(
        &mut self,
        start: &ControlOperation,
        reference: &PublicReplyReference,
        expected_revision: u64,
    ) -> Result<PreparedPublicReply, Error> {
        let run = reply_run(start)?;
        self.require_conversation_scope(start.conversation())?;
        self.require_run_scope(run)?;
        let record = self.execution_record(start)?;
        let invocation = record
            .inputs()
            .invocations()
            .last()
            .ok_or(ControlError::InvalidInput)?
            .invocation();
        if reference.after_invocation() != invocation {
            return Err(ControlError::IdempotencyConflict.into());
        }
        if let Some(prior) = record
            .replies()
            .iter()
            .find(|reply| reply.after_invocation() == invocation)
        {
            if prior != reference {
                return Err(ControlError::IdempotencyConflict.into());
            }
            return Ok(PreparedPublicReply {
                operation: None,
                reference: prior.clone(),
                run,
                config: self.checkpoint_config.clone(),
            });
        }
        let operation = ControlOperation::new(
            reference.operation(),
            start.conversation(),
            peritus_types::ActorId::new(*start.actor_bytes())
                .map_err(|_| ControlError::InvalidInput)?,
            peritus_types::WorkspaceId::new(*start.workspace_bytes())
                .map_err(|_| ControlError::InvalidInput)?,
            expected_revision,
            ControlIntent::PublishReply(reference.clone()),
        );
        Ok(PreparedPublicReply {
            operation: Some(operation),
            reference: reference.clone(),
            run,
            config: self.checkpoint_config.clone(),
        })
    }

    fn prepare_public_reply_inner(
        &mut self,
        start: &ControlOperation,
        digest: Sha256Digest,
        bytes: u64,
        expected_input_generation: Option<u64>,
    ) -> Result<PreparedPublicReply, Error> {
        let run = reply_run(start)?;
        self.require_conversation_scope(start.conversation())?;
        self.require_run_scope(run)?;
        let record = self.execution_record(start)?;
        if expected_input_generation.is_some_and(|expected| {
            expected == 0 || record.inputs().generation() != expected
        })
        {
            return Err(ControlError::StaleRevision.into());
        }
        let invocation = record
            .inputs()
            .invocations()
            .last()
            .ok_or(ControlError::InvalidInput)?
            .invocation();
        if let Some(prior) = record
            .replies()
            .iter()
            .find(|reply| reply.after_invocation() == invocation)
        {
            if prior.digest() != digest || prior.bytes() != bytes {
                return Err(ControlError::IdempotencyConflict.into());
            }
            return Ok(PreparedPublicReply {
                operation: None,
                reference: prior.clone(),
                run,
                config: self.checkpoint_config.clone(),
            });
        }
        let mut identity = b"peritus-workbench/public-reply/v1".to_vec();
        identity.extend_from_slice(start.id().as_bytes());
        identity.extend_from_slice(invocation.as_bytes());
        let hash = peritus_codec::sha256(&identity);
        let mut id = [0; 16];
        id.copy_from_slice(&hash.as_bytes()[..16]);
        id[0] |= 1;
        let id = peritus_product_runner::control::OperationId::new(id)?;
        let reference = PublicReplyReference::new(id, invocation, digest.into_bytes(), bytes)?;
        let operation = ControlOperation::new(
            id,
            start.conversation(),
            peritus_types::ActorId::new(*start.actor_bytes())
                .map_err(|_| ControlError::InvalidInput)?,
            peritus_types::WorkspaceId::new(*start.workspace_bytes())
                .map_err(|_| ControlError::InvalidInput)?,
            record.revision(),
            ControlIntent::PublishReply(reference.clone()),
        );
        Ok(PreparedPublicReply {
            operation: Some(operation),
            reference,
            run,
            config: self.checkpoint_config.clone(),
        })
    }

    fn accept_reply_artifact(
        &mut self,
        operation: &ControlOperation,
        artifact: ArtifactDigest,
    ) -> Result<ControlReceipt, Error> {
        let ControlIntent::PublishReply(reference) = operation.intent() else {
            return Err(ControlError::InvalidInput.into());
        };
        if let Some(receipt) = self.resolve(operation)? {
            return Ok(receipt);
        }
        let descriptor = descriptor_bytes(reference);
        let claim = reply_publication_claim(operation, reference)?;
        let publication = PublicationPlan::new(claim, reply_owner(reference), vec![artifact])?;
        let install = StateInstall::new(
            REPLY_DESCRIPTOR_NAMESPACE,
            operation.id().as_bytes().to_vec(),
            None,
            1,
            descriptor,
        )?;
        let mut append = self.prepare_installs(operation, vec![install])?;
        append.publications.add_plan(publication);
        self.commit_prepared(append).map(|committed| committed.into_receipt())
    }

    /// Reconciles an off-C0 publication against the current aggregate revision.
    /// Unrelated queued-input revisions cannot discard an already delivered public reply.
    pub(crate) fn accept_published_reply(
        &mut self,
        published: PublishedPublicReply,
    ) -> Result<ControlReceipt, Error> {
        self.accept_published_reply_inner(published, None)
    }

    pub(crate) fn accept_published_reply_for_generation(
        &mut self,
        published: PublishedPublicReply,
        expected_input_generation: u64,
    ) -> Result<ControlReceipt, Error> {
        self.accept_published_reply_inner(published, Some(expected_input_generation))
    }

    fn accept_published_reply_inner(
        &mut self,
        published: PublishedPublicReply,
        expected_input_generation: Option<u64>,
    ) -> Result<ControlReceipt, Error> {
        let proposed = &published.operation;
        self.require_conversation_scope(proposed.conversation())?;
        self.require_run_scope(published.run)?;
        let ControlIntent::PublishReply(reference) = proposed.intent() else {
            return Err(ControlError::InvalidInput.into());
        };
        if reference != &published.reference
            || published.artifact != artifact_digest(reference)
        {
            return Err(ControlError::IdempotencyConflict.into());
        }
        if let Some(existing) = self.operation(proposed.conversation(), proposed.id())? {
            if existing.actor_bytes() != proposed.actor_bytes()
                || existing.workspace_bytes() != proposed.workspace_bytes()
                || existing.intent() != proposed.intent()
            {
                return Err(ControlError::IdempotencyConflict.into());
            }
            return self
                .resolve(&existing)?
                .ok_or(Error::Corrupt("accepted public reply has no original receipt"));
        }
        let current = self
            .load(proposed.conversation())?
            .ok_or(ControlError::NotFound)?;
        if current.owner_bytes() != proposed.actor_bytes()
            || current.workspace_bytes() != proposed.workspace_bytes()
        {
            return Err(ControlError::ScopeMismatch.into());
        }
        if expected_input_generation.is_some_and(|expected| {
            expected == 0 || current.inputs().generation() != expected
        }) {
            return Err(ControlError::StaleRevision.into());
        }
        if let Some(prior) = current
            .replies()
            .iter()
            .find(|prior| prior.after_invocation() == reference.after_invocation())
        {
            return if prior == reference {
                Err(Error::Corrupt("public reply reference exists without its operation"))
            } else {
                Err(ControlError::IdempotencyConflict.into())
            };
        }
        if current
            .inputs()
            .invocations()
            .last()
            .map(peritus_product_runner::control::InvocationInputs::invocation)
            != Some(reference.after_invocation())
        {
            return Err(ControlError::IdempotencyConflict.into());
        }
        let rebound = ControlOperation::new(
            proposed.id(),
            proposed.conversation(),
            peritus_types::ActorId::new(*proposed.actor_bytes())
                .map_err(|_| ControlError::InvalidInput)?,
            peritus_types::WorkspaceId::new(*proposed.workspace_bytes())
                .map_err(|_| ControlError::InvalidInput)?,
            current.revision(),
            proposed.intent().clone(),
        );
        self.accept_reply_artifact(&rebound, published.artifact)
    }

    pub(crate) fn legacy_reply_bytes(
        &self,
        reference: &PublicReplyReference,
    ) -> Result<Option<Vec<u8>>, Error> {
        if let Some(descriptor) = self
            .journal
            .state_record(REPLY_DESCRIPTOR_NAMESPACE, reference.operation().as_bytes())?
        {
            verify_descriptor(reference, descriptor.bytes())?;
            return Ok(None);
        }
        let artifact = self
            .journal
            .state_record(REPLY_NAMESPACE, reference.operation().as_bytes())?
            .ok_or(Error::Corrupt("public reply artifact missing"))?;
        super::super::replies::verify_text(reference, artifact.bytes())?;
        Ok(Some(artifact.bytes().to_vec()))
    }

    pub(crate) fn reply_text(&self, reference: &PublicReplyReference) -> Result<String, Error> {
        let legacy = self.legacy_reply_bytes(reference)?;
        let mut reader = open_public_reply_artifact(
            &self.checkpoint_config,
            reference,
            legacy.as_deref(),
        )?;
        let capacity = usize::try_from(reference.bytes()).map_err(|_| ControlError::Capacity)?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(capacity).map_err(|_| ControlError::Capacity)?;
        let mut offset = 0_u64;
        while let Some(chunk) = reader.read_chunk_at(offset, 64 * 1024).map_err(artifact_error)? {
            bytes.extend_from_slice(chunk.bytes());
            offset = offset
                .checked_add(u64::try_from(chunk.bytes().len()).map_err(|_| ControlError::Capacity)?)
                .ok_or(ControlError::Capacity)?;
        }
        super::super::replies::verify_text(reference, &bytes)?;
        String::from_utf8(bytes).map_err(|_| Error::Corrupt("invalid public reply encoding"))
    }
}

pub(crate) fn open_public_reply_artifact(
    config: &StoreConfig,
    reference: &PublicReplyReference,
    legacy: Option<&[u8]>,
) -> Result<ArtifactReadHandle, Error> {
    let digest = spool_reply_artifact(config, reference, legacy)?;
    let store = artifact(ArtifactStore::open(config.clone()))?;
    artifact(store.add_reference(reply_owner(reference), digest))?;
    artifact(store.open_read(digest))
}

pub(in crate::product_control::storage) fn verify_reply_publication(
    store: &ControlStore,
    reference: &PublicReplyReference,
    position: u64,
) -> Result<(), Error> {
    if let Some(descriptor) = store
        .journal
        .state_record(REPLY_DESCRIPTOR_NAMESPACE, reference.operation().as_bytes())?
    {
        if descriptor.revision() != 1 || descriptor.producing_position() != position {
            return Err(Error::Corrupt("public reply descriptor was not published atomically"));
        }
        return verify_descriptor(reference, descriptor.bytes());
    }
    let legacy = store
        .journal
        .state_record(REPLY_NAMESPACE, reference.operation().as_bytes())?
        .ok_or(Error::Corrupt("public reply artifact missing"))?;
    if legacy.revision() != 1 || legacy.producing_position() != position {
        return Err(Error::Corrupt("public reply was not published with its reference"));
    }
    super::super::replies::verify_text(reference, legacy.bytes())
}

fn spool_reply_artifact(
    config: &StoreConfig,
    reference: &PublicReplyReference,
    bytes: Option<&[u8]>,
) -> Result<ArtifactDigest, Error> {
    let store = artifact(ArtifactStore::open(config.clone()))?;
    let digest = artifact_digest(reference);
    let existing = artifact(store.metadata(digest))?;
    if existing.as_ref().is_none_or(|metadata| !metadata.is_referenceable()) {
        let bytes = bytes.ok_or(Error::Corrupt("public reply artifact missing"))?;
        super::super::replies::verify_text(reference, bytes)?;
        let request = WriteRequest::new(
            digest,
            reference.bytes(),
            reference.bytes(),
            artifact(MediaType::new("text/plain;charset=utf-8"))?,
            EncryptionMetadata::unencrypted(),
            EventId::new(*reference.operation().as_bytes())
                .map_err(|_| Error::Corrupt("invalid public reply event identity"))?,
        );
        let mut writer = artifact(store.begin_write(request))?;
        artifact(writer.write_chunk(bytes))?;
        artifact(writer.finalize())?;
    }
    let metadata = artifact(store.verify(digest))?;
    if metadata.size() != reference.bytes() || metadata.digest() != digest {
        return Err(Error::Corrupt("public reply artifact metadata differs from its reference"));
    }
    Ok(digest)
}

fn descriptor_bytes(reference: &PublicReplyReference) -> Vec<u8> {
    let mut bytes = REPLY_DESCRIPTOR_MAGIC.to_vec();
    bytes.extend_from_slice(reference.digest().as_bytes());
    bytes.extend_from_slice(&reference.bytes().to_be_bytes());
    bytes
}

fn verify_descriptor(reference: &PublicReplyReference, bytes: &[u8]) -> Result<(), Error> {
    if bytes != descriptor_bytes(reference) {
        return Err(Error::Corrupt("public reply descriptor differs from its control reference"));
    }
    Ok(())
}

fn reply_owner(reference: &PublicReplyReference) -> ReferenceOwner {
    super::checkpoints::snapshot::reference_owner(
        REPLY_DESCRIPTOR_NAMESPACE,
        reference.operation().as_bytes(),
    )
}

fn reply_run(start: &ControlOperation) -> Result<RunId, Error> {
    let bytes = match start.intent() {
        ControlIntent::StartExecution { run, .. } | ControlIntent::StartGoal { run, .. } => run,
        _ => return Err(ControlError::InvalidInput.into()),
    };
    RunId::new(*bytes).map_err(|_| ControlError::InvalidInput.into())
}

const fn artifact_digest(reference: &PublicReplyReference) -> ArtifactDigest {
    ArtifactDigest::from_sha256(reference.digest())
}

fn artifact<T>(result: Result<T, peritus_artifact_store::ArtifactStoreError>) -> Result<T, Error> {
    result.map_err(artifact_error)
}

fn artifact_error(error: peritus_artifact_store::ArtifactStoreError) -> Error {
    Error::Io(std::io::Error::other(error))
}
