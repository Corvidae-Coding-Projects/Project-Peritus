//! Capacity-aware whole-field rendering with strict raw-byte validation.

use super::{
    MAX_RENDERED_APPROVAL_BYTES, MAX_RENDERED_FIELD_BYTES, RENDER_TRUNCATION_SUFFIX_BYTES,
};

pub(super) fn safe_ascii(value: &[u8]) -> bool {
    let mut index = 0;
    while index < value.len() {
        if value[index] < 0x20 || value[index] > 0x7e {
            return false;
        }
        index += 1;
    }
    true
}
pub(super) struct Builder {
    pub(super) text: String,
    pub(super) truncated: bool,
}
impl Builder {
    pub(super) const fn new() -> Self {
        Self { text: String::new(), truncated: false }
    }
    #[allow(
        clippy::needless_as_bytes,
        clippy::redundant_as_str,
        reason = "pinned Verus supports byte lengths through explicit str and byte-slice views"
    )]
    pub(super) fn field(&mut self, name: &str, value: &str) -> Result<bool, crate::ApprovalError> {
        self.raw_field(name.as_bytes(), value.as_bytes())
    }
    pub(super) fn required_field(
        &mut self,
        name: &str,
        value: &str,
    ) -> Result<(), crate::ApprovalError> {
        if self.field(name, value)? {
            Ok(())
        } else {
            Err(crate::ApprovalError::UnsafeRenderingInput)
        }
    }
    #[allow(
        clippy::needless_as_bytes,
        clippy::redundant_as_str,
        reason = "pinned Verus supports byte lengths through explicit str and byte-slice views"
    )]
    pub(super) fn raw_field(
        &mut self,
        name: &[u8],
        value: &[u8],
    ) -> Result<bool, crate::ApprovalError> {
        let length = name.len().saturating_add(value.len()).saturating_add(2);
        if length > MAX_RENDERED_FIELD_BYTES || !safe_ascii(name) || !safe_ascii(value) {
            return Err(crate::ApprovalError::UnsafeRenderingInput);
        }
        let next = self.text.as_str().as_bytes().len().saturating_add(length);
        if next > MAX_RENDERED_APPROVAL_BYTES - RENDER_TRUNCATION_SUFFIX_BYTES {
            self.truncated = true;
            return Ok(false);
        }
        let mut index = 0;
        while index < name.len() {
            self.text.push(name[index] as char);
            index += 1;
        }
        self.text.push('=');
        let mut index = 0;
        while index < value.len() {
            self.text.push(value[index] as char);
            index += 1;
        }
        self.text.push(';');
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printable_ascii_includes_both_contract_boundaries() {
        assert!(safe_ascii(b" ~"));
        assert!(!safe_ascii(&[0x1f]));
        assert!(!safe_ascii(&[0x7f]));
    }

    #[test]
    fn raw_fields_enforce_exact_field_and_projection_boundaries() {
        let mut exact_field = Builder::new();
        let exact_value = vec![b'x'; MAX_RENDERED_FIELD_BYTES - 3];
        assert_eq!(exact_field.raw_field(b"n", &exact_value), Ok(true));
        assert_eq!(exact_field.text.len(), MAX_RENDERED_FIELD_BYTES);

        let mut oversized_field = Builder::new();
        let oversized_value = vec![b'x'; MAX_RENDERED_FIELD_BYTES - 2];
        assert_eq!(
            oversized_field.raw_field(b"n", &oversized_value),
            Err(crate::ApprovalError::UnsafeRenderingInput)
        );
        assert!(oversized_field.text.is_empty());
        assert!(!oversized_field.truncated);

        let capacity = MAX_RENDERED_APPROVAL_BYTES - RENDER_TRUNCATION_SUFFIX_BYTES;
        let mut exact_projection = Builder { text: "x".repeat(capacity - 4), truncated: false };
        assert_eq!(exact_projection.raw_field(b"n", b"v"), Ok(true));
        assert_eq!(exact_projection.text.len(), capacity);
        assert_eq!(exact_projection.raw_field(b"n", b"v"), Ok(false));
        assert!(exact_projection.truncated);
        assert_eq!(exact_projection.text.len(), capacity);
    }

    #[test]
    fn raw_fields_reject_unsafe_names_and_values_independently() {
        let mut builder = Builder::new();
        assert_eq!(
            builder.raw_field(&[0x1f], b"safe"),
            Err(crate::ApprovalError::UnsafeRenderingInput)
        );
        assert_eq!(
            builder.raw_field(b"safe", &[0x7f]),
            Err(crate::ApprovalError::UnsafeRenderingInput)
        );
        assert!(builder.text.is_empty());
        assert!(!builder.truncated);
    }
}
