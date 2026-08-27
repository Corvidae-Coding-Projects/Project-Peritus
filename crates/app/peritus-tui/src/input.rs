//! Keyboard-to-terminal byte mapping and line-editor helpers.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

pub const fn is_active_key(key: KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

pub fn terminal_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    if !is_active_key(key) {
        return None;
    }
    match key.code {
        KeyCode::Char(character) if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let lower = character.to_ascii_lowercase();
            if lower.is_ascii_lowercase() {
                let byte = u8::try_from(u32::from(lower)).ok()?;
                Some(vec![byte - b'a' + 1])
            } else {
                None
            }
        }
        KeyCode::Char(character) => Some(character.to_string().into_bytes()),
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::BackTab => Some(b"\x1b[Z".to_vec()),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
        KeyCode::F(number) if (1..=4).contains(&number) => {
            Some(vec![0x1b, b'O', b'P'.saturating_add(number - 1)])
        }
        KeyCode::F(number) if (5..=12).contains(&number) => {
            let sequence = match number {
                5 => "\x1b[15~",
                6 => "\x1b[17~",
                7 => "\x1b[18~",
                8 => "\x1b[19~",
                9 => "\x1b[20~",
                10 => "\x1b[21~",
                11 => "\x1b[23~",
                12 => "\x1b[24~",
                _ => return None,
            };
            Some(sequence.as_bytes().to_vec())
        }
        _ => None,
    }
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
