//! Capacity-aware whole-field rendering with strict raw-byte validation.

use super::{
    MAX_RENDERED_APPROVAL_BYTES, MAX_RENDERED_FIELD_BYTES, RENDER_TRUNCATION_SUFFIX_BYTES,
};
use vstd::prelude::*;

verus! {

pub(super) fn safe_ascii(value: &[u8]) -> bool {
    let mut index = 0;
    while index < value.len()
        invariant 0 <= index <= value.len(),
        decreases value.len() - index,
    {
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
    pub(super) const fn new() -> Self { Self { text: String::new(), truncated: false } }

    #[allow(
        clippy::needless_as_bytes,
        clippy::redundant_as_str,
        reason = "pinned Verus supports byte lengths through explicit str and byte-slice views"
    )]
    pub(super) fn field(
        &mut self,
        name: &str,
        value: &str,
    ) -> Result<bool, crate::ApprovalError> {
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
        if length > MAX_RENDERED_FIELD_BYTES
            || !safe_ascii(name)
            || !safe_ascii(value)
        {
            return Err(crate::ApprovalError::UnsafeRenderingInput);
        }
        let next = self.text.as_str().as_bytes().len().saturating_add(length);
        if next > MAX_RENDERED_APPROVAL_BYTES - RENDER_TRUNCATION_SUFFIX_BYTES {
            self.truncated = true;
            return Ok(false);
        }
        let mut index = 0;
        while index < name.len()
            invariant 0 <= index <= name.len(),
            decreases name.len() - index,
        {
            self.text.push(name[index] as char);
            index += 1;
        }
        self.text.push('=');
        let mut index = 0;
        while index < value.len()
            invariant 0 <= index <= value.len(),
            decreases value.len() - index,
        {
            self.text.push(value[index] as char);
            index += 1;
        }
        self.text.push(';');
        Ok(true)
    }
}

} // verus!

#[cfg(test)]
mod tests {
    use super::Builder;

    #[test]
    fn raw_invalid_utf8_and_controls_are_rejected() {
        let mut builder = Builder::new();
        assert!(builder.raw_field(b"field", &[0xff]).is_err());
        assert!(builder.raw_field(&[0xff], b"value").is_err());
        assert!(builder.raw_field(b"field", b"line\nbreak").is_err());
    }
}
