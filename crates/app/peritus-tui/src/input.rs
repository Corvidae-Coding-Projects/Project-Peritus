//! Keyboard-to-terminal byte mapping and line-editor helpers.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

pub mod composer;
pub mod output;
pub mod selection;
mod terminal;
pub use terminal::terminal_bytes;

pub const fn is_active_key(key: KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

pub fn edit_text(buffer: &mut String, cursor: &mut usize, key: KeyEvent) -> bool {
    if !is_active_key(key) {
        return false;
    }
    if let Some(motion) = text_motion(key) {
        *cursor = match motion {
            TextMotion::Left => buffer[..*cursor].char_indices().next_back().map_or(0, |(i, _)| i),
            TextMotion::Right => {
                buffer[*cursor..].chars().next().map_or(*cursor, |ch| *cursor + ch.len_utf8())
            }
            TextMotion::WordLeft => word_left(buffer, *cursor),
            TextMotion::WordRight => word_right(buffer, *cursor),
            TextMotion::LineStart => buffer[..*cursor].rfind('\n').map_or(0, |i| i + 1),
            TextMotion::LineEnd => {
                buffer[*cursor..].find('\n').map_or(buffer.len(), |i| *cursor + i)
            }
            TextMotion::BufferStart => 0,
            TextMotion::BufferEnd => buffer.len(),
        };
        return true;
    }
    match key.code {
        KeyCode::Char(character)
            if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER) =>
        {
            buffer.insert(*cursor, character);
            *cursor += character.len_utf8();
            true
        }
        KeyCode::Backspace if *cursor > 0 => {
            if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
                let start = word_left(buffer, *cursor);
                buffer.replace_range(start..*cursor, "");
                *cursor = start;
                return true;
            }
            if let Some((index, _)) = buffer[..*cursor].char_indices().next_back() {
                buffer.remove(index);
                *cursor = index;
            }
            true
        }
        KeyCode::Delete if *cursor < buffer.len() => {
            buffer.remove(*cursor);
            true
        }
        _ => false,
    }
}

#[derive(Clone, Copy)]
pub(super) enum TextMotion {
    Left,
    Right,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    BufferStart,
    BufferEnd,
}

pub(super) fn text_motion(key: KeyEvent) -> Option<TextMotion> {
    let word = key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
    let line = key.modifiers.contains(KeyModifiers::SUPER);
    match key.code {
        KeyCode::Left if line => Some(TextMotion::LineStart),
        KeyCode::Right if line => Some(TextMotion::LineEnd),
        KeyCode::Left if word => Some(TextMotion::WordLeft),
        KeyCode::Right if word => Some(TextMotion::WordRight),
        KeyCode::Char('b' | 'B') if key.modifiers.contains(KeyModifiers::ALT) && !line => {
            Some(TextMotion::WordLeft)
        }
        KeyCode::Char('f' | 'F') if key.modifiers.contains(KeyModifiers::ALT) && !line => {
            Some(TextMotion::WordRight)
        }
        KeyCode::Left => Some(TextMotion::Left),
        KeyCode::Right => Some(TextMotion::Right),
        KeyCode::Home => Some(TextMotion::BufferStart),
        KeyCode::End => Some(TextMotion::BufferEnd),
        _ => None,
    }
}

fn word_class(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn word_left(buffer: &str, cursor: usize) -> usize {
    let mut characters = buffer[..cursor].char_indices().rev().peekable();
    let mut target = cursor;
    while let Some(&(index, character)) = characters.peek() {
        if !character.is_whitespace() {
            break;
        }
        target = index;
        characters.next();
    }
    let Some((index, first)) = characters.next() else { return target };
    target = index;
    for (index, character) in characters {
        if character.is_whitespace() || word_class(character) != word_class(first) {
            break;
        }
        target = index;
    }
    target
}

fn word_right(buffer: &str, cursor: usize) -> usize {
    let mut characters = buffer[cursor..].char_indices().peekable();
    let Some(&(_, first)) = characters.peek() else { return cursor };
    while let Some(&(_, character)) = characters.peek() {
        if character.is_whitespace() || word_class(character) != word_class(first) {
            break;
        }
        characters.next();
    }
    while let Some(&(_, character)) = characters.peek() {
        if !character.is_whitespace() {
            break;
        }
        characters.next();
    }
    characters.peek().map_or(buffer.len(), |(index, _)| cursor + index)
}
