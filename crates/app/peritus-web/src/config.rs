//! Explicit gateway paths and user-editable TOML configuration.

use crate::error::{Result, problem};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub struct Options {
    pub(crate) port: u16,
    pub(crate) root: PathBuf,
    pub(crate) assets: PathBuf,
    pub(crate) config_file: PathBuf,
    pub(crate) state_file: PathBuf,
    pub(crate) daemon_config_root: PathBuf,
    pub(crate) product_state_root: PathBuf,
    pub(crate) daemon_config: Option<PathBuf>,
    pub(crate) endpoint: Option<PathBuf>,
    pub(crate) cli: PathBuf,
}
impl Options {
    pub(crate) fn parse() -> Result<Self> {
        let (config_root, state_root) = native_roots()?;
        Self::from_args(std::env::args().skip(1), config_root, &state_root)
    }

    fn from_args(
        args: impl IntoIterator<Item = String>,
        config_root: PathBuf,
        state_root: &Path,
    ) -> Result<Self> {
        let mut value = Self {
            port: 4173,
            root: std::env::current_dir()?,
            assets: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../webui/dist"),
            config_file: config_root.join("webui.toml"),
            state_file: state_root.join("webui/workspace.json"),
            daemon_config_root: config_root,
            product_state_root: state_root.join("product-state"),
            daemon_config: None,
            endpoint: None,
            cli: std::env::var_os("PERITUS_BIN").map_or_else(
                || {
                    let sibling = std::env::current_exe().ok().and_then(|path| {
                        path.parent().map(|parent| {
                            parent.join(format!("peritus{}", std::env::consts::EXE_SUFFIX))
                        })
                    });
                    sibling
                        .filter(|path| path.is_file())
                        .unwrap_or_else(|| PathBuf::from("peritus"))
                },
                PathBuf::from,
            ),
        };
        let mut args = args.into_iter();
        while let Some(flag) = args.next() {
            if flag == "--help" {
                println!(
                    "peritus-web [--port 4173] [--root PATH] [--assets PATH] [--config PATH] [--state PATH] [--daemon-config PATH] [--product-state DIR] [--endpoint PATH] [--cli PATH]"
                );
                std::process::exit(0);
            }
            let argument = args.next().ok_or_else(|| problem(format!("{flag} needs a value")))?;
            match flag.as_str() {
                "--port" => value.port = argument.parse().map_err(problem)?,
                "--root" => value.root = argument.into(),
                "--assets" => value.assets = argument.into(),
                "--config" => value.config_file = argument.into(),
                "--state" => value.state_file = argument.into(),
                "--daemon-config" => value.daemon_config = Some(argument.into()),
                "--product-state" => value.product_state_root = argument.into(),
                "--endpoint" => value.endpoint = Some(argument.into()),
                "--cli" => value.cli = argument.into(),
                _ => return Err(problem(format!("Unknown option {flag}"))),
            }
        }
        value.root = value.root.canonicalize()?;
        Ok(value)
    }
}

// Match peritus-launcher::AppLayout without pulling the daemon and TUI into this gateway.
#[cfg(all(unix, not(target_os = "macos")))]
fn native_roots() -> Result<(PathBuf, PathBuf)> {
    let home = absolute_environment("HOME")?;
    let config =
        optional_absolute_environment("XDG_CONFIG_HOME")?.unwrap_or_else(|| home.join(".config"));
    let state = optional_absolute_environment("XDG_STATE_HOME")?
        .unwrap_or_else(|| home.join(".local/state"));
    Ok((config.join("peritus"), state.join("peritus")))
}

#[cfg(target_os = "macos")]
fn native_roots() -> Result<(PathBuf, PathBuf)> {
    let support = absolute_environment("HOME")?.join("Library/Application Support/Peritus");
    Ok((support.join("Config"), support.join("State")))
}

#[cfg(windows)]
fn native_roots() -> Result<(PathBuf, PathBuf)> {
    Ok((
        absolute_environment("APPDATA")?.join("Peritus"),
        absolute_environment("LOCALAPPDATA")?.join("Peritus/State"),
    ))
}

fn absolute_environment(name: &str) -> Result<PathBuf> {
    optional_absolute_environment(name)?
        .ok_or_else(|| problem(format!("{name} is unavailable for the current user")))
}

fn optional_absolute_environment(name: &str) -> Result<Option<PathBuf>> {
    let Some(value) = std::env::var_os(name) else { return Ok(None) };
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(problem(format!("{name} must contain an absolute path")));
    }
    Ok(Some(path))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent user-editable TOML toggles are not mutually exclusive states"
)]
pub struct Preferences {
    pub(crate) theme: String,
    pub(crate) density: String,
    pub(crate) motion: bool,
    pub(crate) sound: bool,
    pub(crate) font_size: u16,
    pub(crate) font_family: String,
    pub(crate) mono_family: String,
    pub(crate) explorer_width: u16,
    pub(crate) controls_visible: bool,
    pub(crate) explorer_visible: bool,
    pub(crate) word_wrap: bool,
    pub(crate) markdown_preview: bool,
    pub(crate) shortcuts: BTreeMap<String, String>,
    pub(crate) tokens: BTreeMap<String, String>,
    pub(crate) aliases: BTreeMap<String, String>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: "nixie".into(),
            density: "comfortable".into(),
            motion: true,
            sound: false,
            font_size: 14,
            font_family: "Barlow, sans-serif".into(),
            mono_family: "Iosevka, monospace".into(),
            explorer_width: 248,
            controls_visible: true,
            explorer_visible: true,
            word_wrap: true,
            markdown_preview: true,
            shortcuts: BTreeMap::from([
                ("commands".into(), "Mod+k".into()),
                ("files".into(), "Mod+Shift+e".into()),
                ("git".into(), "Mod+Shift+g".into()),
                ("new".into(), "Mod+Alt+n".into()),
                ("settings".into(), "Mod+,".into()),
            ]),
            tokens: BTreeMap::new(),
            aliases: BTreeMap::new(),
        }
    }
}
impl Preferences {
    pub(crate) fn parse(text: &str) -> Result<Self> {
        let value: Self = toml::from_str(text).map_err(problem)?;
        if !(12..=22).contains(&value.font_size) || !(180..=480).contains(&value.explorer_width) {
            return Err(problem("font_size must be 12–22; explorer_width must be 180–480"));
        }
        if !["nixie", "daylight", "blueprint"].contains(&value.theme.as_str()) {
            return Err(problem("theme must be nixie, daylight, or blueprint"));
        }
        if !["comfortable", "compact"].contains(&value.density.as_str()) {
            return Err(problem("density must be comfortable or compact"));
        }
        for (key, color) in &value.tokens {
            if !["background", "panel", "display", "text", "muted", "accent", "line"]
                .contains(&key.as_str())
                || !color.starts_with('#')
                || ![4, 7].contains(&color.len())
                || !color[1..].bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(problem(
                    "tokens accepts named color roles with #RGB or #RRGGBB values",
                ));
            }
        }
        if value.aliases.len() > 100 || value.shortcuts.len() > 100 {
            return Err(problem("At most 100 aliases and shortcuts are supported"));
        }
        for (name, command) in &value.aliases {
            if name.is_empty()
                || !name.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
                || !command.starts_with('/')
                || command.contains(['\r', '\n'])
            {
                return Err(problem("Aliases need lowercase names and a single slash command"));
            }
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_path_overrides_do_not_redirect_daemon_discovery() {
        let root = tempfile::tempdir().unwrap();
        let config_root = root.path().join("native-config");
        let state_root = root.path().join("native-state");
        let config = root.path().join("custom/webui.toml");
        let state = root.path().join("custom/workspace.json");
        let options = Options::from_args(
            [
                "--config".into(),
                config.to_string_lossy().into_owned(),
                "--state".into(),
                state.to_string_lossy().into_owned(),
            ],
            config_root.clone(),
            &state_root,
        )
        .unwrap();
        assert_eq!(options.config_file, config);
        assert_eq!(options.state_file, state);
        assert_eq!(options.daemon_config_root, config_root);
        assert_eq!(options.product_state_root, state_root.join("product-state"));
        assert!(options.daemon_config.is_none());
        assert!(options.endpoint.is_none());
    }
}
