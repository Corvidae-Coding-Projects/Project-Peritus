//! Bounded control-safe quality result rendering.

use core::fmt::Write;

use peritus_tool_protocol::{BoundedText, Truncation};

use crate::error::truncate_utf8;

pub fn text(value: impl Into<String>) -> BoundedText {
    let mut value = value.into().replace('\0', "\\0");
    if value.is_empty() {
        value.push_str("(no detail)");
    }
    truncate_utf8(&mut value, 16 * 1_024);
    BoundedText::new(value).expect("sanitized nonempty bounded quality text")
}

pub fn output(bytes: &[u8], maximum: u32) -> (BoundedText, Truncation) {
    let mut escaped = String::with_capacity(bytes.len().min(maximum as usize));
    for &byte in bytes {
        match byte {
            b'\n' => escaped.push('\n'),
            b'\r' => escaped.push_str("\\r"),
            b'\t' => escaped.push_str("\\t"),
            0x20..=0x7e => escaped.push(char::from(byte)),
            _ => write!(escaped, "\\x{byte:02x}").expect("writing to a string cannot fail"),
        }
    }
    if escaped.len() <= maximum as usize {
        return (text(escaped), Truncation::Complete);
    }
    let maximum = maximum as usize;
    let marker = "[earlier output omitted]\n";
    let rendered = if maximum <= marker.len() {
        marker[..maximum].to_owned()
    } else {
        format!("{marker}{}", &escaped[escaped.len() - (maximum - marker.len())..])
    };
    (text(rendered), Truncation::HeadDropped)
}
