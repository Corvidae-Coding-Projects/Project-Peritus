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

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{edit_text, terminal_bytes};

    #[test]
    fn terminal_key_mapping_preserves_text_and_control_semantics() {
        assert_eq!(
            terminal_bytes(KeyEvent::new(KeyCode::Char('λ'), KeyModifiers::NONE)),
            Some("λ".as_bytes().to_vec())
        );
        assert_eq!(
            terminal_bytes(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(vec![3])
        );
        assert_eq!(
            terminal_bytes(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
            Some(b"\x1b[A".to_vec())
        );
    }

    #[test]
    fn terminal_key_mapping_preserves_readline_and_modified_navigation_chords() {
        for (code, modifiers, expected) in [
            (KeyCode::Left, KeyModifiers::CONTROL, b"\x1b[1;5D".as_slice()),
            (KeyCode::Right, KeyModifiers::ALT | KeyModifiers::SHIFT, b"\x1b[1;4C"),
            (KeyCode::F(5), KeyModifiers::CONTROL, b"\x1b[15;5~"),
            (KeyCode::F(1), KeyModifiers::SHIFT, b"\x1b[1;2P"),
            (KeyCode::Delete, KeyModifiers::ALT, b"\x1b[3;3~"),
            (KeyCode::Char('b'), KeyModifiers::ALT, b"\x1bb"),
            (KeyCode::Backspace, KeyModifiers::ALT, b"\x1b\x7f"),
            (KeyCode::Char(' '), KeyModifiers::CONTROL, b"\0"),
            (KeyCode::Char('_'), KeyModifiers::CONTROL, b"\x1f"),
        ] {
            assert_eq!(terminal_bytes(KeyEvent::new(code, modifiers)).as_deref(), Some(expected));
        }
    }

    #[test]
    fn editor_cursor_remains_on_utf8_boundaries() {
        let mut buffer = "aλz".to_owned();
        let mut cursor = buffer.len();
        assert!(edit_text(
            &mut buffer,
            &mut cursor,
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)
        ));
        assert!(edit_text(
            &mut buffer,
            &mut cursor,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)
        ));
        assert_eq!(buffer, "az");
        assert_eq!(cursor, 1);
    }
}
