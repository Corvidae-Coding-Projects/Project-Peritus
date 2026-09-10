//! Private immutable request artifacts installed with the control receipt.

use super::{
    CapturedConversation, ControlError, ControlIntent, ControlOperation, Error, InvocationId,
    QueueIntent,
};
use peritus_product_runner::control::{ConversationId, InputSelection};
use serde::Deserialize;
use serde::Serialize;

pub(in crate::product_control) mod inspect;

pub(in crate::product_control) struct RequestArchive {
    request: Vec<u8>,
    manifest: Manifest,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u16,
    conversation: ConversationId,
    invocation: InvocationId,
    actor: [u8; 16],
    workspace: [u8; 16],
    revision: u64,
    generation: u64,
    request_id: String,
    request_digest: [u8; 32],
    included: Vec<InputSelection>,
    newly_incorporated: Vec<InputSelection>,
    public_replies: Vec<peritus_product_runner::control::PublicReplyReference>,
    sources: Vec<super::manifest::InputSource>,
    messages: Vec<super::manifest::MessageSource>,
    request_bytes: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    brief: Vec<peritus_product_runner::control::BriefBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    images: Vec<peritus_product_runner::control::ImageAttachment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    files: Vec<super::manifest::FileSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    guidance: Option<GuidanceManifest>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GuidanceManifest {
    dependency_revision: u64,
    identities: Vec<[u8; 16]>,
    digest: [u8; 32],
    bytes: u64,
}

impl RequestArchive {
    pub(in crate::product_control) const fn request_digest(&self) -> [u8; 32] {
        self.manifest.request_digest
    }

    pub(in crate::product_control) fn manifest_digest(&self) -> Result<[u8; 32], Error> {
        Ok(peritus_codec::sha256(&self.manifest_bytes()?).into_bytes())
    }

    fn manifest_bytes(&self) -> Result<Vec<u8>, Error> {
        serde_json::to_vec(&self.manifest)
            .map_err(|_| Error::Corrupt("cannot encode request manifest"))
    }

    pub(in crate::product_control) fn new(
        captured: &CapturedConversation,
        invocation: InvocationId,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<Self, Error> {
        let mut requested_images = request
            .messages()
            .iter()
            .flat_map(peritus_model_protocol::Message::content)
            .filter_map(|block| match block {
                peritus_model_protocol::ContentBlock::Image(image) => Some(image),
                _ => None,
            });
        if !requested_images.by_ref().eq(captured.images.iter()) {
            return Err(ControlError::InvalidInput.into());
        }
        if !captured.file_context.is_empty()
            && !request.messages().iter().flat_map(peritus_model_protocol::Message::content).any(|block| {
                matches!(block, peritus_model_protocol::ContentBlock::Text(text) if text.expose_for_wire().contains(&captured.file_context))
            })
        {
            return Err(ControlError::InvalidInput.into());
        }
        if !captured.guidance.identities().is_empty()
            && !request
                .messages()
                .iter()
                .flat_map(peritus_model_protocol::Message::content)
                .any(|block| {
                    matches!(block, peritus_model_protocol::ContentBlock::Text(text) if text.expose_for_wire().contains(captured.guidance.text()))
                })
        {
            return Err(ControlError::InvalidInput.into());
        }
        let bytes = request.canonical_bytes_bounded(16 * 1024 * 1024).map_err(|error| {
            if error.kind() == peritus_model_protocol::ProtocolErrorKind::InvalidLimit {
                ControlError::Capacity
            } else {
                ControlError::InvalidInput
            }
        })?;
        Ok(Self {
            manifest: Manifest {
                version: 1,
                conversation: captured.conversation,
                invocation,
                actor: captured.actor.into_bytes(),
                workspace: captured.workspace.into_bytes(),
                revision: captured.revision,
                generation: captured.inputs.generation(),
                request_id: request.request_id().expose_for_wire().to_owned(),
                request_digest: peritus_codec::sha256(&bytes).into_bytes(),
                included: captured.inputs.included().to_vec(),
                newly_incorporated: captured.inputs.pending().to_vec(),
                public_replies: captured.replies.clone(),
                sources: captured.sources.clone(),
                messages: super::manifest::messages(request)?,
                request_bytes: bytes.len() as u64,
                brief: captured.brief.clone(),
                images: captured.image_sources.clone(),
                files: captured.file_sources.clone(),
                guidance: (!captured.guidance.identities().is_empty()).then(|| GuidanceManifest {
                    dependency_revision: captured.guidance.dependency_revision(),
                    identities: captured
                        .guidance
                        .identities()
                        .iter()
                        .map(|identity| *identity.id().as_bytes())
                        .collect(),
                    digest: captured.guidance.digest().into_bytes(),
                    bytes: captured.guidance.text().len() as u64,
                }),
            },
            request: bytes,
        })
    }
}

type ArchivedBytes = (Vec<u8>, Vec<u8>);

pub(in crate::product_control) fn validate_archive(
    operation: &ControlOperation,
    archive: Option<RequestArchive>,
) -> Result<Option<ArchivedBytes>, Error> {
    match (operation.intent(), archive) {
        (ControlIntent::Queue(QueueIntent::Incorporate { request_digest, .. }), Some(archive)) => {
            if peritus_codec::sha256(&archive.request).as_bytes() != request_digest {
                return Err(ControlError::InvalidInput.into());
            }
            let manifest = archive.manifest_bytes()?;
            verify_manifest(operation, &manifest, None)?;
            Ok(Some((archive.request, manifest)))
        }
        (
            ControlIntent::Queue(QueueIntent::Incorporate { .. })
            | ControlIntent::PublishReply(_)
            | ControlIntent::AttachImage { .. }
            | ControlIntent::AttachFile { .. }
            | ControlIntent::RefreshFile { .. },
            None,
        )
        | (_, Some(_)) => Err(ControlError::InvalidInput.into()),
        (_, None) => Ok(None),
    }
}

pub(in crate::product_control) fn verify_manifest(
    operation: &ControlOperation,
    bytes: &[u8],
    before: Option<&peritus_product_runner::control::ConversationRecord>,
) -> Result<(), Error> {
    if bytes.len() > 1024 * 1024 {
        return Err(ControlError::Capacity.into());
    }
    let manifest: Manifest =
        serde_json::from_slice(bytes).map_err(|_| Error::Corrupt("invalid request manifest"))?;
    let ControlIntent::Queue(QueueIntent::Incorporate {
        invocation,
        request_digest,
        manifest_digest,
        items,
    }) = operation.intent()
    else {
        return Err(ControlError::InvalidInput.into());
    };
    if manifest.version != 1
        || manifest.conversation != operation.conversation()
        || manifest.invocation != *invocation
        || manifest.actor != *operation.actor_bytes()
        || manifest.workspace != *operation.workspace_bytes()
        || manifest.revision != operation.expected_revision()
        || manifest.request_digest != *request_digest
        || peritus_codec::sha256(bytes).as_bytes() != manifest_digest
        || manifest.newly_incorporated != *items
        || manifest.included.len() > 1024
        || manifest.request_id.is_empty()
        || !manifest.included.ends_with(items)
        || manifest.sources.len() != manifest.included.len()
        || manifest.sources.iter().zip(&manifest.included).any(|(source, selection)| {
            source.selection != *selection || source.bytes == 0 || source.bytes > 8192
        })
        || manifest.messages.is_empty()
        || manifest.messages.len()
            > peritus_model_protocol::ProtocolLimits::PRODUCTION.max_messages()
        || manifest.request_bytes == 0
        || manifest.request_bytes > 16 * 1024 * 1024
        || manifest.brief.len() > 4
        || manifest.images.len() > peritus_product_runner::attachment::MAX_IMAGE_COUNT
        || manifest
            .images
            .iter()
            .try_fold(0_u64, |total, image| total.checked_add(image.bytes()))
            .is_none_or(|total| {
                total > peritus_product_runner::attachment::MAX_IMAGE_SELECTION_BYTES
            })
        || manifest.guidance.as_ref().is_some_and(|guidance| {
            guidance.dependency_revision == 0
                || guidance.identities.is_empty()
                || guidance.identities.len() > peritus_app_protocol::MAX_WORKBENCH_GUIDANCE_PAGE
                || guidance.bytes == 0
                || guidance.bytes > peritus_app_protocol::MAX_WORKBENCH_GUIDANCE_RENDER_BYTES as u64
                || guidance.identities.contains(&[0; 16])
                || guidance
                    .identities
                    .iter()
                    .enumerate()
                    .any(|(index, identity)| guidance.identities[..index].contains(identity))
        })
    {
        return Err(Error::Corrupt("request manifest does not match incorporation"));
    }
    if let Some(before) = before {
        let reply_metadata = before
            .replies()
            .iter()
            .map(|reply| (reply.after_invocation(), String::new()))
            .collect();
        let capture = before.capture_with_replies(&reply_metadata, true)?;
        if !manifest.images.iter().eq(before.eligible_images(capture.included())) {
            return Err(Error::Corrupt("request images differ from explicit input selection"));
        }
        if !manifest.files.iter().cloned().eq(before
            .eligible_files(capture.included())
            .into_iter()
            .map(super::manifest::FileSource::selected))
        {
            return Err(Error::Corrupt("request files differ from exact selected versions"));
        }
        let expected_brief: Vec<_> = before
            .brief()
            .bindings()
            .iter()
            .filter(|binding| capture.included().contains(&binding.selected()))
            .copied()
            .collect();
        if manifest.brief != expected_brief {
            return Err(Error::Corrupt("request brief differs from governing source bindings"));
        }
        if manifest.included != capture.included()
            || manifest.newly_incorporated != capture.pending()
            || manifest.generation != capture.generation()
        {
            return Err(Error::Corrupt("request manifest differs from its governing input view"));
        }
        for source in &manifest.sources {
            let input = before
                .inputs()
                .revisions()
                .iter()
                .find(|input| input.selection() == source.selection)
                .ok_or(Error::Corrupt("request source revision missing"))?;
            if *source != super::manifest::InputSource::from_revision(input) {
                return Err(Error::Corrupt("request source digest differs from exact input"));
            }
        }
        let expected: Vec<_> = before
            .replies()
            .iter()
            .filter(|reply| capture.public_replies().contains(&reply.after_invocation()))
            .cloned()
            .collect();
        if manifest.public_replies != expected {
            return Err(Error::Corrupt("request public reply sources differ from durable history"));
        }
    }
    Ok(())
}
