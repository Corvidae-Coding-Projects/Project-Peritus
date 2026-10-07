//! Control-safe independently bounded render windows.

use peritus_tool_protocol::{BoundedText, Truncation, render_output_tail};

use crate::error::truncate_utf8;

pub struct RenderedOutput {
    pub(crate) model: BoundedText,
    pub(crate) human: BoundedText,
    pub(crate) model_truncation: Truncation,
    pub(crate) human_truncation: Truncation,
}

pub fn output(bytes: &[u8], model_limit: u32, human_limit: u32) -> RenderedOutput {
    let (model, model_truncation) = render_output_tail(bytes, model_limit);
    let (human, human_truncation) = render_output_tail(bytes, human_limit);
    RenderedOutput {
        model,
        human,
        model_truncation,
        human_truncation,
    }
}

pub fn checked_text(mut value: String) -> BoundedText {
    value = value.replace('\0', "\\0");
    if value.is_empty() {
        value.push_str("(no output)");
    }
    truncate_utf8(&mut value, BoundedText::MAX_BYTES);
    BoundedText::new(value).expect("sanitized nonempty bounded rendering")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_controls_and_labels_tail_truncation() {
        let rendered = output(b"begin\0\x1bend", 12, 64);
        assert_eq!(rendered.model_truncation, Truncation::HeadDropped);
        assert!(!rendered.human.as_str().contains('\0'));
        assert!(rendered.human.as_str().contains("\\x1b"));
    }
}
