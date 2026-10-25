//! Local presentation preferences. No provider, permissions, or execution settings live here.

use crate::highlight::Theme;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::PathBuf,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shortcut {
    Find,
    Files,
    Queue,
    Editor,
    PasteImage,
    Copy,
    CopyCode,
    Keys,
    Theme,
}

impl Shortcut {
    pub(crate) const ALL: [Self; 9] = [
        Self::Find,
        Self::Files,
        Self::Queue,
        Self::Editor,
        Self::PasteImage,
        Self::Copy,
        Self::CopyCode,
        Self::Keys,
        Self::Theme,
    ];
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Find => "find",
            Self::Files => "files",
            Self::Queue => "queue",
            Self::Editor => "editor",
            Self::PasteImage => "paste-image",
            Self::Copy => "copy",
            Self::CopyCode => "copy-code",
            Self::Keys => "keys",
            Self::Theme => "theme",
        }
    }
    pub(crate) const fn default_key(self) -> &'static str {
        match self {
            Self::Find => "ctrl-f",
            Self::Files => "ctrl-o",
            Self::Queue => "f4",
            Self::Editor => "ctrl-g",
            Self::PasteImage => "ctrl-v",
            Self::Copy => "alt-c",
            Self::CopyCode => "alt-y",
            Self::Keys => "f9",
            Self::Theme => "f10",
        }
    }
}

#[derive(Debug, Default)]
pub struct Preferences {
    pub(crate) theme: Theme,
    bindings: BTreeMap<String, String>,
}

impl Preferences {
    pub(crate) fn load() -> Result<Self, String> {
        let path = path()?;
        let file = match fs::File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("Cannot read {}: {e}", path.display())),
        };
        let mut bytes = Vec::new();
        file.take(65_537).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if bytes.len() > 65_536 {
            return Err("TUI preferences exceed 64 KiB".into());
        }
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| format!("Invalid TUI preferences: {e}"))?;
        let mut preferences = Self::default();
        if let Some(theme) = value.get("theme") {
            preferences.theme = theme.as_str().and_then(Theme::parse).ok_or("Unknown TUI theme")?;
        }
        if let Some(bindings) = value.get("bindings") {
            for (name, key) in bindings.as_object().ok_or("TUI bindings must be an object")? {
                let action = Shortcut::ALL
                    .into_iter()
                    .find(|a| a.name() == name)
                    .ok_or("Unknown TUI shortcut action")?;
                let key = key.as_str().ok_or("Shortcut must be a string")?;
                parse_key(key)?;
                preferences.bindings.insert(action.name().into(), key.to_ascii_lowercase());
            }
        }
        preferences.validate()?;
        Ok(preferences)
    }

    pub(crate) fn binding(&self, action: Shortcut) -> &str {
        self.bindings.get(action.name()).map_or_else(|| action.default_key(), String::as_str)
    }

    pub(crate) fn action(&self, event: KeyEvent) -> Option<Shortcut> {
        let event = KeyEvent {
            code: match event.code {
                KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
                other => other,
            },
            ..event
        };
        Shortcut::ALL.into_iter().find(|a| {
            parse_key(self.binding(*a))
                .is_ok_and(|key| key.code == event.code && key.modifiers == event.modifiers)
        })
    }

    pub(crate) fn set_binding(&mut self, action: Shortcut, key: &str) -> Result<(), String> {
        let key = key.to_ascii_lowercase();
        parse_key(&key)?;
        let previous = self.bindings.insert(action.name().into(), key);
        if let Err(error) = self.validate().and_then(|()| self.save()) {
            if let Some(previous) = previous {
                self.bindings.insert(action.name().into(), previous);
            } else {
                self.bindings.remove(action.name());
            }
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn reset(&mut self) -> Result<(), String> {
        let old = std::mem::take(&mut self.bindings);
        if let Err(error) = self.save() {
            self.bindings = old;
            return Err(error);
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), String> {
        for (index, action) in Shortcut::ALL.iter().enumerate() {
            let key = parse_key(self.binding(*action))?;
            if Shortcut::ALL[..index]
                .iter()
                .any(|other| parse_key(self.binding(*other)).is_ok_and(|existing| existing == key))
            {
                return Err(format!(
                    "{} conflicts with another convenience shortcut",
                    self.binding(*action)
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn save(&self) -> Result<(), String> {
        let path = path()?;
        let parent = path.parent().ok_or("TUI preferences have no parent")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        let value =
            serde_json::json!({"version":1, "theme":self.theme.name(), "bindings":self.bindings});
        file.write_all(serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?.as_bytes())
            .map_err(|e| e.to_string())?;
        file.as_file().sync_all().map_err(|e| e.to_string())?;
        file.persist(&path).map_err(|e| e.to_string())?;
        Ok(())
    }
}

pub fn key_name(key: KeyEvent) -> Option<String> {
    let code = match key.code {
        KeyCode::Char(c) => c.to_ascii_lowercase().to_string(),
        KeyCode::F(n) => format!("f{n}"),
        _ => return None,
    };
    let mut result = String::new();
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        result.push_str("ctrl-");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        result.push_str("alt-");
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        result.push_str("shift-");
    }
    result.push_str(&code);
    parse_key(&result).ok().map(|_| result)
}

fn parse_key(value: &str) -> Result<KeyEvent, String> {
    let mut modifiers = KeyModifiers::NONE;
    let mut parts = value.split('-').peekable();
    let mut code = None;
    while let Some(part) = parts.next() {
        if parts.peek().is_some() {
            modifiers |= match part {
                "ctrl" => KeyModifiers::CONTROL,
                "alt" => KeyModifiers::ALT,
                "shift" => KeyModifiers::SHIFT,
                _ => return Err("Use ctrl-, alt-, shift-, or F1–F12".into()),
            };
        } else {
            code = part
                .strip_prefix('f')
                .and_then(|n| n.parse::<u8>().ok())
                .filter(|n| (1..=12).contains(n))
                .map_or_else(
                    || {
                        (part.len() == 1 && part.as_bytes()[0].is_ascii_alphanumeric())
                            .then(|| KeyCode::Char(char::from(part.as_bytes()[0])))
                    },
                    |number| Some(KeyCode::F(number)),
                );
        }
    }
    let key = KeyEvent::new(code.ok_or("Unsupported shortcut key")?, modifiers);
    let reserved = matches!(key.code, KeyCode::Char('c' | 'q' | 'r'))
        && modifiers.contains(KeyModifiers::CONTROL)
        || matches!(key.code, KeyCode::F(2 | 3))
        || (matches!(key.code, KeyCode::Char(_))
            && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT));
    if reserved {
        return Err("That key is reserved for typing, copy/stop, reconnect, or history".into());
    }
    Ok(key)
}

fn path() -> Result<PathBuf, String> {
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "APPDATA" } else { "HOME" }).map(|p| {
                if cfg!(windows) { PathBuf::from(p) } else { PathBuf::from(p).join(".config") }
            })
        })
        .ok_or("No user configuration directory is available")?;
    if !root.is_absolute() {
        return Err("TUI configuration directory must be absolute".into());
    }
    Ok(root.join("peritus").join("tui.json"))
}
