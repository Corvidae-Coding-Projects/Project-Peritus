//! Sealed, content-free provenance. Raw model messages never enter public projections.

use super::{ControlError, Error};
use peritus_model_protocol::{ModelRequest, ProtocolLimits, Role, encode_messages};
use peritus_product_runner::control::{InputRevision, InputSelection};
use serde::Deserialize;
use serde::Serialize;

/// Exact source descriptor and one immutable version selected for this request only.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::product_control) struct FileSource {
    pub attachment: peritus_product_runner::control::FileAttachment,
    pub version: peritus_product_runner::control::FileVersion,
}
impl FileSource {
    pub fn selected(entry: &peritus_product_runner::control::FileSelection) -> Self {
        Self { attachment: entry.file().clone(), version: entry.current().clone() }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::product_control) struct InputSource {
    pub selection: InputSelection,
    pub digest: [u8; 32],
    pub bytes: u64,
}

impl InputSource {
    pub fn from_revision(input: &InputRevision) -> Self {
        Self {
            selection: input.selection(),
            digest: peritus_codec::sha256(input.text().as_bytes()).into_bytes(),
            bytes: input.text().len() as u64,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::product_control) enum MessageRole {
    System,
    Developer,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::product_control) struct MessageSource {
    pub role: MessageRole,
    pub digest: [u8; 32],
    pub encoded_bytes: u64,
    pub blocks: u32,
}

pub(in crate::product_control) fn messages(
    request: &ModelRequest,
) -> Result<Vec<MessageSource>, Error> {
    request
        .messages()
        .iter()
        .map(|message| {
            let bytes = encode_messages(std::slice::from_ref(message), ProtocolLimits::PRODUCTION)
                .map_err(|_| ControlError::InvalidInput)?;
            let role = match message.role() {
                Role::System => MessageRole::System,
                Role::Developer => MessageRole::Developer,
                Role::User => MessageRole::User,
                Role::Assistant => MessageRole::Assistant,
                Role::Tool => MessageRole::Tool,
            };
            Ok(MessageSource {
                role,
                digest: peritus_codec::sha256(&bytes).into_bytes(),
                encoded_bytes: bytes.len() as u64,
                blocks: u32::try_from(message.content().len())
                    .map_err(|_| ControlError::Capacity)?,
            })
        })
        .collect()
}
