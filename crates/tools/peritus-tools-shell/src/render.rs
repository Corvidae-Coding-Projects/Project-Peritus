//! Control-safe independently bounded render windows.

use core::fmt::Write;

use peritus_tool_protocol::{BoundedText, Truncation};

use crate::error::truncate_utf8;

pub struct RenderedOutput {
    pub(crate) model: BoundedText,
    pub(crate) human: BoundedText,
    pub(crate) model_truncation: Truncation,
    pub(crate) human_truncation: Truncation,
}

pub fn output(bytes: &[u8], model_limit: u32, human_limit: u32) -> RenderedOutput {
    let escaped = escape(bytes);
    let (model, model_truncation) = bounded_tail(&escaped, model_limit as usize);
    let (human, human_truncation) = bounded_tail(&escaped, human_limit as usize);
    RenderedOutput {
        model: checked_text(model),
        human: checked_text(human),
        model_truncation,
        human_truncation,
    }
}

pub fn checked_text(mut value: String) -> BoundedText {
    value = value.replace('\0', "\\0");
    if value.is_empty() {
        value.push_str("(no output)");
    }
    truncate_utf8(&mut value, 16 * 1_024);
    BoundedText::new(value).expect("sanitized nonempty bounded rendering")
}

fn escape(bytes: &[u8]) -> String {
    let mut rendered = String::with_capacity(bytes.len().min(16 * 1_024));
    for &byte in bytes {
        match byte {
            b'\n' => rendered.push('\n'),
            b'\r' => rendered.push_str("\\r"),
            b'\t' => rendered.push_str("\\t"),
            0x20..=0x7e => rendered.push(char::from(byte)),
            _ => write!(rendered, "\\x{byte:02x}").expect("writing to a string cannot fail"),
        }
    }
    rendered
}

fn bounded_tail(value: &str, maximum: usize) -> (String, Truncation) {
    if value.len() <= maximum {
        return (value.to_owned(), Truncation::Complete);
    }
    let marker = "[earlier output omitted]\n";
    if maximum <= marker.len() {
        return (marker[..maximum].to_owned(), Truncation::HeadDropped);
    }
    let keep = maximum - marker.len();
    let start = value.len() - keep;
    (format!("{marker}{}", &value[start..]), Truncation::HeadDropped)
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
