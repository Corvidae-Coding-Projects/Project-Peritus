//! Xterm input encoding, preserving modifiers instead of sending an unmodified key.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn terminal_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    if !super::is_active_key(key) {
        return None;
    }
    if let Some(sequence) = sequence(key.code) {
        return Some(sequence.encode(key.modifiers));
    }
    let mut bytes = match key.code {
        KeyCode::Char(character) if key.modifiers.contains(KeyModifiers::CONTROL) => {
            vec![control_character(character)?]
        }
        KeyCode::Char(character) => character.to_string().into_bytes(),
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => return Some(b"\x1b[Z".to_vec()),
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        _ => return None,
    };
    if key.modifiers.contains(KeyModifiers::ALT) {
        bytes.insert(0, 0x1b);
    }
    Some(bytes)
}

fn control_character(character: char) -> Option<u8> {
    let character = character.to_ascii_lowercase();
    match character {
        'a'..='z' => u8::try_from(u32::from(character)).ok().map(|byte| byte - b'a' + 1),
        '@' | ' ' | '2' => Some(0),
        '[' | '3' => Some(27),
        '\\' | '4' => Some(28),
        ']' | '5' => Some(29),
        '^' | '6' => Some(30),
        '_' | '7' => Some(31),
        '?' | '8' => Some(127),
        _ => None,
    }
}

enum Sequence {
    Cursor(char),
    Function(char),
    Tilde(u8),
}

impl Sequence {
    fn encode(&self, modifiers: KeyModifiers) -> Vec<u8> {
        let modifier = 1
            + u8::from(modifiers.contains(KeyModifiers::SHIFT))
            + 2 * u8::from(modifiers.contains(KeyModifiers::ALT))
            + 4 * u8::from(modifiers.contains(KeyModifiers::CONTROL));
        match (self, modifier) {
            (Self::Cursor(final_byte), 1) => format!("\x1b[{final_byte}"),
            (Self::Function(final_byte), 1) => format!("\x1bO{final_byte}"),
            (Self::Tilde(number), 1) => format!("\x1b[{number}~"),
            (Self::Cursor(final_byte) | Self::Function(final_byte), _) => {
                format!("\x1b[1;{modifier}{final_byte}")
            }
            (Self::Tilde(number), _) => format!("\x1b[{number};{modifier}~"),
        }
        .into_bytes()
    }
}

fn sequence(key: KeyCode) -> Option<Sequence> {
    Some(match key {
        KeyCode::Up => Sequence::Cursor('A'),
        KeyCode::Down => Sequence::Cursor('B'),
        KeyCode::Right => Sequence::Cursor('C'),
        KeyCode::Left => Sequence::Cursor('D'),
        KeyCode::Home => Sequence::Cursor('H'),
        KeyCode::End => Sequence::Cursor('F'),
        KeyCode::Insert => Sequence::Tilde(2),
        KeyCode::Delete => Sequence::Tilde(3),
        KeyCode::PageUp => Sequence::Tilde(5),
        KeyCode::PageDown => Sequence::Tilde(6),
        KeyCode::F(number @ 1..=4) => Sequence::Function(char::from(b'P' + number - 1)),
        KeyCode::F(number @ 5..=12) => {
            Sequence::Tilde([15, 17, 18, 19, 20, 21, 23, 24][usize::from(number - 5)])
        }
        _ => return None,
    })
}
