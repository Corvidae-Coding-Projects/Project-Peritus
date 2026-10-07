//! Control-safe output previews measured in their final representation.

use core::fmt::Write;

use crate::{BoundedText, Truncation};

/// Renders an escaped output tail within the selected and representable byte capacity.
///
/// Omission and empty-output markers count toward the same capacity. Escape units remain
/// intact, and omitted bytes are reported even when the selected capacity exceeds the
/// protocol representation. Work and allocation depend on the preview capacity.
/// A zero capacity is treated as one byte; accepted [`crate::CallLimits`] require positive
/// rendering capacities before execution.
///
/// # Panics
/// Panics if internally generated text violates the nonempty, NUL-free byte-window invariant.
#[must_use]
pub fn render_output_tail(bytes: &[u8], maximum: u32) -> (BoundedText, Truncation) {
    let maximum = (maximum as usize).clamp(1, BoundedText::MAX_BYTES);
    if bytes.is_empty() {
        let marker = "(no output)";
        let rendered = marker[..maximum.min(marker.len())].to_owned();
        return (
            BoundedText::new(rendered).expect("nonempty bounded empty-output marker"),
            Truncation::Complete,
        );
    }
    let start = tail_start(bytes, maximum);
    let (start, marker, truncation) = if start == 0 {
        (0, "", Truncation::Complete)
    } else {
        let marker = "[earlier output omitted]\n";
        let marker = &marker[..maximum.min(marker.len())];
        (tail_start(bytes, maximum - marker.len()), marker, Truncation::HeadDropped)
    };
    // A selected display capacity may be much larger than this actual observation. Avoid
    // reserving the entire allowance for a short message or output tail.
    let capacity = bytes[start..]
        .len()
        .saturating_mul(4)
        .saturating_add(marker.len())
        .min(maximum);
    let mut rendered = String::with_capacity(capacity);
    rendered.push_str(marker);
    escape_into(&mut rendered, &bytes[start..]);
    (
        BoundedText::new(rendered).expect("escaped output fits its nonempty protocol window"),
        truncation,
    )
}

fn tail_start(bytes: &[u8], maximum: usize) -> usize {
    let mut start = bytes.len();
    let mut available = maximum;
    for (index, &byte) in bytes.iter().enumerate().rev() {
        let length = match byte {
            b'\n' | 0x20..=0x7e => 1,
            b'\r' | b'\t' => 2,
            _ => 4,
        };
        if length > available {
            break;
        }
        available -= length;
        start = index;
    }
    start
}

fn escape_into(target: &mut String, bytes: &[u8]) {
    for &byte in bytes {
        match byte {
            b'\n' => target.push('\n'),
            b'\r' => target.push_str("\\r"),
            b'\t' => target.push_str("\\t"),
            0x20..=0x7e => target.push(char::from(byte)),
            _ => write!(target, "\\x{byte:02x}").expect("writing to a string cannot fail"),
        }
    }
}
