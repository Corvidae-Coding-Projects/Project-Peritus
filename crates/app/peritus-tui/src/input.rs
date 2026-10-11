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
    match key.code {
        KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            buffer.insert(*cursor, character);
            *cursor += character.len_utf8();
            true
        }
        KeyCode::Backspace if *cursor > 0 => {
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
        KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
            *cursor = word_left(buffer, *cursor);
            true
        }
        KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
            *cursor = word_right(buffer, *cursor);
            true
        }
        KeyCode::Left if *cursor > 0 => {
            if let Some((index, _)) = buffer[..*cursor].char_indices().next_back() {
                *cursor = index;
            }
            true
        }
        KeyCode::Right if *cursor < buffer.len() => {
            if let Some(character) = buffer[*cursor..].chars().next() {
                *cursor += character.len_utf8();
            }
            true
        }
        KeyCode::Home => {
            *cursor = 0;
            true
        }
        KeyCode::End => {
            *cursor = buffer.len();
            true
        }
        _ => false,
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
