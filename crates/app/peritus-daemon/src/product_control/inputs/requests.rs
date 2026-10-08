//! Exact typed user-request and assistant-history catalog for one admitted provider request.

use super::{CapturedConversation, ControlError, ControlStore, Error};
use peritus_product_runner::{
    ContextSource, ContextSourceKind, ContextSourcePage, ContextSourceSlice,
    MAX_CONTEXT_SOURCE_PAGE, MAX_CONTEXT_SOURCE_SLICE_BYTES,
};
use std::collections::BTreeMap;

#[derive(Clone)]
pub(crate) enum RequestSourceBody {
    InlineUser {
        descriptor: ContextSource,
        text: String,
    },
    ArtifactUser {
        descriptor: ContextSource,
    },
    Assistant {
        descriptor: ContextSource,
        reference: peritus_product_runner::control::PublicReplyReference,
    },
}

#[derive(Clone)]
pub(crate) struct RequestSourceSnapshot {
    authority_binding: [u8; 32],
    catalog_binding: [u8; 32],
    generation: u64,
    required: bool,
    bodies: Vec<RequestSourceBody>,
}

impl RequestSourceBody {
    pub(crate) const fn descriptor(&self) -> &ContextSource {
        match self {
            Self::InlineUser { descriptor, .. }
            | Self::ArtifactUser { descriptor }
            | Self::Assistant { descriptor, .. } => descriptor,
        }
    }

    pub(crate) fn read_inline(&self, offset: u64) -> Result<ContextSourceSlice, Error> {
        let Self::InlineUser { descriptor, text } = self else {
            return Err(ControlError::InvalidInput.into());
        };
        read_verified_text(descriptor, text, offset)
    }
}

pub(crate) fn read_verified_text(
    descriptor: &ContextSource,
    text: &str,
    offset: u64,
) -> Result<ContextSourceSlice, Error> {
    if peritus_codec::sha256(text.as_bytes()).as_bytes() != descriptor.digest()
        || u64::try_from(text.len()).ok() != Some(descriptor.bytes())
    {
        return Err(Error::Corrupt("request source differs from its immutable descriptor"));
    }
    let start = usize::try_from(offset).map_err(|_| ControlError::InvalidInput)?;
    if start >= text.len() || !text.is_char_boundary(start) {
        return Err(ControlError::InvalidInput.into());
    }
    let mut end = start
        .saturating_add(MAX_CONTEXT_SOURCE_SLICE_BYTES)
        .min(text.len());
    while !text.is_char_boundary(end) {
        end = end.checked_sub(1).ok_or(ControlError::InvalidInput)?;
    }
    let next = (end < text.len())
        .then(|| u64::try_from(end).map_err(|_| ControlError::Capacity))
        .transpose()?;
    ContextSourceSlice::new(descriptor, offset, text[start..end].to_owned(), next)
        .map_err(|_| Error::Corrupt("invalid request-source slice"))
}

impl RequestSourceSnapshot {
    #[must_use]
    pub(crate) const fn authority_binding(&self) -> [u8; 32] {
        self.authority_binding
    }

    #[must_use]
    pub(crate) const fn catalog_binding(&self) -> [u8; 32] {
        self.catalog_binding
    }

    /// Returns the exact input generation admitted with the provider request.
    #[must_use]
    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub(crate) const fn required(&self) -> bool {
        self.required
    }

    pub(crate) fn request_sources(
        &self,
        after: Option<u64>,
    ) -> Result<ContextSourcePage, Error> {
        let start = match after {
            None => 0,
            Some(0) => return Err(ControlError::InvalidInput.into()),
            Some(after) => usize::try_from(after).map_err(|_| ControlError::InvalidInput)?,
        };
        if start > self.bodies.len() {
            return Err(ControlError::NotFound.into());
        }
        let sources = self
            .bodies
            .iter()
            .skip(start)
            .take(MAX_CONTEXT_SOURCE_PAGE)
            .map(|body| body.descriptor().clone())
            .collect::<Vec<_>>();
        let next = if start.saturating_add(sources.len()) < self.bodies.len() {
            sources.last().map(ContextSource::ordinal)
        } else {
            None
        };
        ContextSourcePage::new(after, sources, next)
            .map_err(|_| Error::Corrupt("invalid authoritative request-source page"))
    }

    pub(crate) fn body(&self, source: u64) -> Result<RequestSourceBody, Error> {
        let index = source.checked_sub(1).ok_or(ControlError::InvalidInput)?;
        let index = usize::try_from(index).map_err(|_| ControlError::InvalidInput)?;
        self.bodies.get(index).cloned().ok_or(ControlError::NotFound.into())
    }
}

impl CapturedConversation {
    /// Returns whether any governing body is retained out of line and must be tool-read.
    pub(crate) fn request_sources_required(&self) -> bool {
        self.sources.iter().any(|source| source.out_of_line)
    }
}

enum BoundRequestSource {
    User(super::manifest::InputSource),
    Assistant(peritus_product_runner::control::PublicReplyReference),
}

impl ControlStore {
    pub(crate) fn request_source_snapshot(
        &self,
        captured: &CapturedConversation,
    ) -> Result<RequestSourceSnapshot, Error> {
        let record = self
            .load(captured.conversation)?
            .ok_or(ControlError::NotFound)?;
        if record.owner_bytes() != captured.actor.as_bytes()
            || record.workspace_bytes() != captured.workspace.as_bytes()
        {
            return Err(ControlError::ScopeMismatch.into());
        }
        let required = captured.request_sources_required();
        let mut users = captured
            .sources
            .iter()
            .cloned()
            .map(|source| (source.selection, source))
            .collect::<BTreeMap<_, _>>();
        let mut replies = captured
            .replies
            .iter()
            .cloned()
            .map(|reply| (reply.after_invocation(), reply))
            .collect::<BTreeMap<_, _>>();
        let mut ordered = Vec::with_capacity(users.len().saturating_add(replies.len()));
        for invocation in record.inputs().invocations() {
            for selection in invocation.items() {
                if let Some(source) = users.remove(selection) {
                    ordered.push(BoundRequestSource::User(source));
                }
            }
            if let Some(reply) = replies.remove(&invocation.invocation()) {
                ordered.push(BoundRequestSource::Assistant(reply));
            }
        }
        for selection in captured.inputs().included() {
            if let Some(source) = users.remove(selection) {
                ordered.push(BoundRequestSource::User(source));
            }
        }
        if !users.is_empty() || !replies.is_empty() {
            return Err(Error::Corrupt(
                "request-source history cannot be ordered by its control bindings",
            ));
        }
        let bodies = ordered
            .into_iter()
            .enumerate()
            .map(|(index, binding)| match binding {
                BoundRequestSource::User(binding) => {
                    let descriptor = user_descriptor(index, &binding, !binding.out_of_line)?;
                    let revision = record
                        .inputs()
                        .revisions()
                        .iter()
                        .find(|revision| revision.selection() == binding.selection)
                        .ok_or(Error::Corrupt("authoritative request-source revision missing"))?;
                    if super::manifest::InputSource::from_revision(
                        revision,
                        binding.out_of_line,
                    ) != binding
                    {
                        return Err(Error::Corrupt(
                            "authoritative request-source binding changed",
                        ));
                    }
                    match revision.source() {
                        Some(source) => {
                            peritus_types::ArtifactId::new(source.artifact_bytes()).map_err(
                                |_| Error::Corrupt("invalid request-source artifact identity"),
                            )?;
                            Ok(RequestSourceBody::ArtifactUser { descriptor })
                        }
                        None => Ok(RequestSourceBody::InlineUser {
                            descriptor,
                            text: revision.text().to_owned(),
                        }),
                    }
                }
                BoundRequestSource::Assistant(reply) => {
                    let descriptor = assistant_descriptor(index, &reply)?;
                    Ok(RequestSourceBody::Assistant {
                        descriptor,
                        reference: reply,
                    })
                }
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let catalog_binding = catalog_binding(&bodies)?;
        Ok(RequestSourceSnapshot {
            authority_binding: authority_binding(&captured.sources)?,
            catalog_binding,
            generation: captured.inputs().generation(),
            required,
            bodies,
        })
    }
}

fn authority_binding(sources: &[super::manifest::InputSource]) -> Result<[u8; 32], Error> {
    let mut exact = b"peritus/request-source-authority/v1\0".to_vec();
    exact.extend_from_slice(
        &u64::try_from(sources.len())
            .map_err(|_| ControlError::Capacity)?
            .to_be_bytes(),
    );
    for source in sources {
        exact.extend_from_slice(source.selection.id().as_bytes());
        exact.extend_from_slice(&source.selection.revision().to_be_bytes());
        exact.extend_from_slice(&source.digest);
        exact.extend_from_slice(&source.bytes.to_be_bytes());
        exact.extend_from_slice(&source.artifact.unwrap_or([0; 16]));
        exact.push(u8::from(source.out_of_line));
    }
    Ok(peritus_codec::sha256(&exact).into_bytes())
}

fn catalog_binding(bodies: &[RequestSourceBody]) -> Result<[u8; 32], Error> {
    let mut exact = b"peritus/request-source-catalog/v1\0".to_vec();
    exact.extend_from_slice(
        &u64::try_from(bodies.len())
            .map_err(|_| ControlError::Capacity)?
            .to_be_bytes(),
    );
    for body in bodies {
        let descriptor = body.descriptor();
        exact.extend_from_slice(&descriptor.ordinal().to_be_bytes());
        exact.push(match descriptor.kind() {
            ContextSourceKind::ExternalContext => 0,
            ContextSourceKind::UserRequest => 1,
            ContextSourceKind::AssistantHistory => 2,
        });
        exact.extend_from_slice(
            &u64::try_from(descriptor.label().len())
                .map_err(|_| ControlError::Capacity)?
                .to_be_bytes(),
        );
        exact.extend_from_slice(descriptor.label().as_bytes());
        exact.extend_from_slice(descriptor.digest());
        exact.extend_from_slice(&descriptor.bytes().to_be_bytes());
        exact.push(u8::from(descriptor.requires_read()));
    }
    Ok(peritus_codec::sha256(&exact).into_bytes())
}

fn user_descriptor(
    index: usize,
    source: &super::manifest::InputSource,
    in_prompt: bool,
) -> Result<ContextSource, Error> {
    let ordinal = ordinal(index)?;
    ContextSource::new(
        ordinal,
        format!(
            "User input {} revision {}",
            hex(source.selection.id().as_bytes()),
            source.selection.revision()
        ),
        source.digest,
        source.bytes,
    )
    .map(|source| {
        let source = source.with_kind(ContextSourceKind::UserRequest);
        if in_prompt { source.with_prompt_body() } else { source }
    })
    .map_err(|_| Error::Corrupt("invalid authoritative request-source descriptor"))
}

fn assistant_descriptor(
    index: usize,
    source: &peritus_product_runner::control::PublicReplyReference,
) -> Result<ContextSource, Error> {
    ContextSource::new(
        ordinal(index)?,
        format!(
            "Assistant public reply after invocation {}",
            hex(source.after_invocation().as_bytes())
        ),
        source.digest().into_bytes(),
        source.bytes(),
    )
    .map(|source| source.with_kind(ContextSourceKind::AssistantHistory))
    .map_err(|_| Error::Corrupt("invalid assistant-history source descriptor"))
}

fn ordinal(index: usize) -> Result<u64, Error> {
    u64::try_from(index)
        .ok()
        .and_then(|index| index.checked_add(1))
        .ok_or_else(|| ControlError::Capacity.into())
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    text
}
