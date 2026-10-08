//! Private immutable request artifacts installed with the control receipt.

use super::{
    CapturedConversation, ControlError, ControlIntent, ControlOperation, Error, InvocationId,
    QueueIntent,
};
use peritus_product_runner::control::ConversationId;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    source_incorporated: bool,
    /// New archives bind the typed user/assistant out-of-line projection. Absence identifies a
    /// legacy archive whose already-admitted fixed prompt remains valid under its original view.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    typed_history_sources: bool,
    generation: u64,
    request_id: String,
    request_digest: [u8; 32],
    included: Vec<InputSelection>,
    newly_incorporated: Vec<InputSelection>,
    public_replies: Vec<peritus_product_runner::control::PublicReplyReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    out_of_line_replies: Vec<InvocationId>,
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
                source_revision: (captured.source_revision != captured.revision)
                    .then_some(captured.source_revision),
                source_incorporated: captured.source_incorporated,
                typed_history_sources: true,
                generation: captured.inputs.generation(),
                request_id: request.request_id().expose_for_wire().to_owned(),
                request_digest: peritus_codec::sha256(&bytes).into_bytes(),
                included: captured.inputs.included().to_vec(),
                newly_incorporated: captured.inputs.pending().to_vec(),
                public_replies: captured.replies.clone(),
                out_of_line_replies: captured.inputs.out_of_line_replies().to_vec(),
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
            verify_manifest(operation, &manifest, None, None)?;
            Ok(Some((archive.request, manifest)))
        }
        (
            ControlIntent::Queue(QueueIntent::Incorporate { .. })
            | ControlIntent::PublishReply(_)
            | ControlIntent::AttachImage { .. }
            | ControlIntent::AttachFile { .. }
            | ControlIntent::AttachFileSource { .. }
            | ControlIntent::RefreshFile { .. },
            None,
        )
        | (_, Some(_)) => Err(ControlError::InvalidInput.into()),
        (_, None) => Ok(None),
    }
}

pub(in crate::product_control) fn manifest_source_revision(
    bytes: &[u8],
) -> Result<Option<u64>, Error> {
    if bytes.len() > peritus_journal::MAX_STATE_BYTES {
        return Err(ControlError::Capacity.into());
    }
    let manifest: Manifest =
        serde_json::from_slice(bytes).map_err(|_| Error::Corrupt("invalid request manifest"))?;
    Ok(manifest.source_revision)
}

pub(in crate::product_control) fn verify_manifest(
    operation: &ControlOperation,
    bytes: &[u8],
    before: Option<&peritus_product_runner::control::ConversationRecord>,
    historical: Option<&peritus_product_runner::control::ConversationRecord>,
) -> Result<(), Error> {
    if bytes.len() > peritus_journal::MAX_STATE_BYTES {
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
        || manifest.source_revision.is_some_and(|source| {
            source == 0 || source >= manifest.revision
        })
        || (manifest.source_incorporated && manifest.source_revision.is_none())
        || manifest.request_digest != *request_digest
        || peritus_codec::sha256(bytes).as_bytes() != manifest_digest
        || manifest.newly_incorporated != *items
        || manifest.request_id.is_empty()
        || !manifest.included.ends_with(items)
        || manifest.sources.len() != manifest.included.len()
        || (!manifest.typed_history_sources && !manifest.out_of_line_replies.is_empty())
        || manifest.out_of_line_replies.iter().enumerate().any(|(index, invocation)| {
            manifest.out_of_line_replies[..index].contains(invocation)
                || !manifest
                    .public_replies
                    .iter()
                    .any(|reply| reply.after_invocation() == *invocation)
        })
        || manifest.sources.iter().zip(&manifest.included).any(|(source, selection)| {
            source.selection != *selection
                || source.bytes == 0
                || (source.artifact.is_none() && source.bytes > 8192)
                || source.artifact == Some([0; 16])
                || (source.artifact.is_some() && !source.out_of_line)
        })
        || manifest.messages.is_empty()
        || manifest.messages.len()
            > peritus_model_protocol::ProtocolLimits::PRODUCTION.max_messages()
        || manifest.request_bytes == 0
        || manifest.request_bytes > 16 * 1024 * 1024
        || manifest.brief.len() > 4
        || manifest.guidance.as_ref().is_some_and(|guidance| {
            guidance.dependency_revision == 0
                || guidance.identities.is_empty()
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
        let mut incorporated = None;
        let governing = match manifest.source_revision {
            Some(revision) => {
                let historical = historical.ok_or(Error::Corrupt(
                    "historical request source projection is missing",
                ))?;
                if historical.revision() != revision {
                    return Err(Error::Corrupt(
                        "historical request source revision differs from retained projection",
                    ));
                }
                if manifest.source_incorporated {
                    incorporated = Some(super::incorporated_source_view(historical.clone())?);
                    incorporated.as_ref().ok_or(Error::Corrupt(
                        "historical incorporated source view is missing",
                    ))?
                } else {
                    historical
                }
            }
            None => before,
        };
        let reply_bytes = governing
            .replies()
            .iter()
            .map(|reply| (reply.after_invocation(), reply.bytes()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let (public_replies, out_of_line_replies, out_of_line_inputs) =
            governing.reply_source_projection(&reply_bytes, true)?;
        let capture = governing.inputs().capture()?;
        if manifest.included != capture.included()
            || manifest.newly_incorporated != capture.pending()
            || manifest.generation != capture.generation()
        {
            return Err(Error::Corrupt(
                "request manifest differs from its exact governing input view",
            ));
        }
        if !manifest.images.iter().eq(governing.eligible_images(capture.included())) {
            return Err(Error::Corrupt(
                "request images differ from exact governing source selection",
            ));
        }
        if !manifest.files.iter().cloned().eq(governing
            .eligible_files(capture.included())
            .into_iter()
            .map(super::manifest::FileSource::selected))
        {
            return Err(Error::Corrupt(
                "request files differ from exact governing source versions",
            ));
        }
        let expected_brief: Vec<_> = governing
            .brief()
            .bindings()
            .iter()
            .filter(|binding| capture.included().contains(&binding.selected()))
            .copied()
            .collect();
        if manifest.brief != expected_brief {
            return Err(Error::Corrupt(
                "request brief differs from exact governing source bindings",
            ));
        }
        for source in &manifest.sources {
            let input = governing
                .inputs()
                .revisions()
                .iter()
                .find(|input| input.selection() == source.selection)
                .ok_or(Error::Corrupt("request source revision missing"))?;
            if *source
                != super::manifest::InputSource::from_revision(input, source.out_of_line)
                || (manifest.typed_history_sources
                    && source.out_of_line
                        != out_of_line_inputs.contains(&source.selection))
            {
                return Err(Error::Corrupt("request source digest differs from exact input"));
            }
        }
        let expected: Vec<_> = governing
            .replies()
            .iter()
            .filter(|reply| public_replies.contains(&reply.after_invocation()))
            .cloned()
            .collect();
        if manifest.public_replies != expected
            || (manifest.typed_history_sources
                && manifest.out_of_line_replies != out_of_line_replies)
        {
            return Err(Error::Corrupt(
                "request public reply sources differ from exact governing history",
            ));
        }
    }
    Ok(())
}
