//! Provenance for host-validated image observations, separate from governing input.
use peritus_model_protocol::{CompletedToolCall, MediaInput, MediaKind};
use peritus_product_runner::control::ControlError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolImageObservation {
    call_id: String,
    tool: String,
    media_type: String,
    digest: [u8; 32],
    bytes: u64,
}

impl ToolImageObservation {
    pub(crate) fn from_tool(
        call: &CompletedToolCall,
        image: &MediaInput,
    ) -> Result<Self, ControlError> {
        let bytes = image.inline_bytes_for_wire().ok_or(ControlError::InvalidInput)?;
        let value = Self {
            call_id: call.id().expose_for_wire().to_owned(),
            tool: call.name().as_str().to_owned(),
            media_type: image.media_type().as_str().to_owned(),
            digest: image.digest().ok_or(ControlError::InvalidInput)?.into_bytes(),
            bytes: bytes.len() as u64,
        };
        if image.kind() != MediaKind::Image || !value.valid() {
            return Err(ControlError::InvalidInput);
        }
        Ok(value)
    }

    pub(crate) fn valid(&self) -> bool {
        !self.call_id.is_empty()
            && self.call_id.len() <= 256
            && matches!(
                self.tool.as_str(),
                "browser_control" | "desktop_control" | "computer_snapshot" | "view_image"
            )
            && matches!(self.media_type.as_str(), "image/png" | "image/jpeg" | "image/webp")
            && self.bytes > 0
            && self.bytes <= 4 * 1024 * 1024
    }

    pub(crate) fn matches(&self, image: &MediaInput) -> bool {
        self.valid()
            && image.kind() == MediaKind::Image
            && image.media_type().as_str() == self.media_type
            && image.digest().is_some_and(|digest| digest.as_bytes() == &self.digest)
            && image.inline_bytes_for_wire().is_some_and(|bytes| bytes.len() as u64 == self.bytes)
    }
}
