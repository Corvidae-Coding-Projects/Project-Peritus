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
            TextMotion::Up => vertical_line(buffer, *cursor, false),
            TextMotion::Down => vertical_line(buffer, *cursor, true),
        };
        return true;
    }
    if let Some(deletion) = text_deletion(key) {
        let range = match deletion {
            TextDeletion::Backward => {
                buffer[..*cursor].char_indices().next_back().map_or(*cursor, |(i, _)| i)..*cursor
            }
            TextDeletion::Forward => {
                *cursor
                    ..buffer[*cursor..].chars().next().map_or(*cursor, |ch| *cursor + ch.len_utf8())
            }
            TextDeletion::WordBackward => word_left(buffer, *cursor)..*cursor,
            TextDeletion::WordForward => *cursor..word_right(buffer, *cursor),
            TextDeletion::LineStart => line_start(buffer, *cursor)..*cursor,
            TextDeletion::LineEnd => {
                let end = line_end(buffer, *cursor);
                *cursor..if end == *cursor && end < buffer.len() { end + 1 } else { end }
            }
        };
        *cursor = range.start;
        buffer.replace_range(range, "");
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
    Up,
    Down,
}

pub(super) fn text_motion(key: KeyEvent) -> Option<TextMotion> {
    let word = key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
    let line = key.modifiers.contains(KeyModifiers::SUPER);
    let control = key.modifiers.contains(KeyModifiers::CONTROL) && !line;
    match key.code {
        KeyCode::Left if line => Some(TextMotion::LineStart),
        KeyCode::Right if line => Some(TextMotion::LineEnd),
        KeyCode::Left if word => Some(TextMotion::WordLeft),
        KeyCode::Right if word => Some(TextMotion::WordRight),
        KeyCode::Char('a' | 'A') if control => Some(TextMotion::LineStart),
        KeyCode::Char('e' | 'E') if control => Some(TextMotion::LineEnd),
        KeyCode::Char('b' | 'B') if key.modifiers.contains(KeyModifiers::ALT) && !line => {
            Some(TextMotion::WordLeft)
        }
        KeyCode::Char('f' | 'F') if key.modifiers.contains(KeyModifiers::ALT) && !line => {
            Some(TextMotion::WordRight)
        }
        KeyCode::Left => Some(TextMotion::Left),
        KeyCode::Right => Some(TextMotion::Right),
        KeyCode::Char('b' | 'B') if control => Some(TextMotion::Left),
        KeyCode::Char('f' | 'F') if control => Some(TextMotion::Right),
        KeyCode::Up => Some(TextMotion::Up),
        KeyCode::Down => Some(TextMotion::Down),
        KeyCode::Char('p' | 'P') if control => Some(TextMotion::Up),
        KeyCode::Char('n' | 'N') if control => Some(TextMotion::Down),
        KeyCode::Home if control => Some(TextMotion::BufferStart),
        KeyCode::End if control => Some(TextMotion::BufferEnd),
        KeyCode::Home => Some(TextMotion::LineStart),
        KeyCode::End => Some(TextMotion::LineEnd),
        _ => None,
    }
}

#[derive(Clone, Copy)]
pub(super) enum TextDeletion {
    Backward,
    Forward,
    WordBackward,
    WordForward,
    LineStart,
    LineEnd,
}

pub(super) fn text_deletion(key: KeyEvent) -> Option<TextDeletion> {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let super_key = key.modifiers.contains(KeyModifiers::SUPER);
    match key.code {
        KeyCode::Backspace if super_key => Some(TextDeletion::LineStart),
        KeyCode::Backspace if control || alt => Some(TextDeletion::WordBackward),
        KeyCode::Backspace => Some(TextDeletion::Backward),
        KeyCode::Delete if control || alt => Some(TextDeletion::WordForward),
        KeyCode::Delete => Some(TextDeletion::Forward),
        KeyCode::Char('h' | 'H') if control && alt => Some(TextDeletion::WordBackward),
        KeyCode::Char('h' | 'H') if control => Some(TextDeletion::Backward),
        KeyCode::Char('d' | 'D') if alt => Some(TextDeletion::WordForward),
        KeyCode::Char('d' | 'D') if control => Some(TextDeletion::Forward),
        KeyCode::Char('w' | 'W') if control => Some(TextDeletion::WordBackward),
        KeyCode::Char('u' | 'U') if control => Some(TextDeletion::LineStart),
        KeyCode::Char('k' | 'K') if control => Some(TextDeletion::LineEnd),
        _ => None,
    }
}

pub(crate) fn vertical_direction(key: KeyEvent) -> Option<bool> {
    match text_motion(key) {
        Some(TextMotion::Up) => Some(false),
        Some(TextMotion::Down) => Some(true),
        _ => None,
    }
}

fn line_start(text: &str, cursor: usize) -> usize {
    text[..cursor].rfind('\n').map_or(0, |i| i + 1)
}

fn line_end(text: &str, cursor: usize) -> usize {
    text[cursor..].find('\n').map_or(text.len(), |i| cursor + i)
}

fn vertical_line(text: &str, cursor: usize, down: bool) -> usize {
    let start = line_start(text, cursor);
    let column = text[start..cursor].chars().count();
    let next = if down {
        let end = line_end(text, cursor);
        if end == text.len() {
            return cursor;
        }
        end + 1
    } else {
        if start == 0 {
            return cursor;
        }
        line_start(text, start - 1)
    };
    let end = line_end(text, next);
    text[next..end].char_indices().nth(column).map_or(end, |(i, _)| next + i)
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
