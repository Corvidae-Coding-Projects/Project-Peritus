//! UTF-8 selection and replacement semantics for the message composer.

use std::ops::Range;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn range(anchor: Option<usize>, cursor: usize) -> Option<Range<usize>> {
    anchor.filter(|anchor| *anchor != cursor).map(|anchor| anchor.min(cursor)..anchor.max(cursor))
}

pub fn edit(
    buffer: &mut String,
    cursor: &mut usize,
    anchor: &mut Option<usize>,
    key: KeyEvent,
) -> bool {
    if !super::is_active_key(key) {
        return false;
    }
    let selected = range(*anchor, *cursor);
    if let Some(motion) = super::text_motion(key) {
        if key.modifiers.contains(KeyModifiers::SHIFT) {
            anchor.get_or_insert(*cursor);
        } else {
            *anchor = None;
            if let Some(selected) = selected
                && matches!(motion, super::TextMotion::Left | super::TextMotion::Right)
            {
                *cursor = if key.code == KeyCode::Left { selected.start } else { selected.end };
                return true;
            }
        }
        return super::edit_text(buffer, cursor, key);
    }
    let typing = matches!(key.code, KeyCode::Char(_))
        && !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER);
    let deleting = matches!(key.code, KeyCode::Backspace | KeyCode::Delete);
    if typing || deleting {
        *anchor = None;
        if let Some(selected) = selected {
            *cursor = selected.start;
            buffer.replace_range(selected, "");
            if deleting {
                return true;
            }
        }
    }
    super::edit_text(buffer, cursor, key)
}
