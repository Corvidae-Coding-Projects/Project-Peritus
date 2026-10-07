//! Bounded control-safe quality result rendering.

use peritus_tool_protocol::BoundedText;
pub use peritus_tool_protocol::render_output_tail as output;

use crate::error::truncate_utf8;

pub fn text(value: impl Into<String>) -> BoundedText {
    let mut value = value.into().replace('\0', "\\0");
    if value.is_empty() {
        value.push_str("(no detail)");
    }
    truncate_utf8(&mut value, BoundedText::MAX_BYTES);
    BoundedText::new(value).expect("sanitized nonempty bounded quality text")
}
