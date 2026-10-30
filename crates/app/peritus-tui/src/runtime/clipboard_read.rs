//! Explicit, bounded text paste; clipboard content never becomes an automatic action.

use std::time::Duration;
use tokio::process::Command;

use crate::local::ClipboardSource;

pub(super) async fn read(source: ClipboardSource) -> Result<String, String> {
    let mut command = command(source)?;
    let bytes =
        super::local::helper_output(&mut command, 32 * 1024 * 1024, Duration::from_secs(5)).await?;
    String::from_utf8(bytes)
        .map_err(|_| "Clipboard text is not valid UTF-8; draft retained.".into())
}

fn command(source: ClipboardSource) -> Result<Command, String> {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            let mut command = Command::new("wl-paste");
            command.args(["--no-newline", "--type", "text"]);
            if source == ClipboardSource::Primary {
                command.arg("--primary");
            }
            return Ok(command);
        }
        if std::env::var_os("DISPLAY").is_some() {
            let mut command = Command::new("xclip");
            command.args([
                "-selection",
                if source == ClipboardSource::Primary { "primary" } else { "clipboard" },
                "-target",
                "UTF8_STRING",
                "-out",
            ]);
            return Ok(command);
        }
        Err("No desktop clipboard is available; terminal bracketed paste remains available.".into())
    }
    #[cfg(not(target_os = "linux"))]
    {
        if source == ClipboardSource::Primary {
            return Err("Primary-selection paste is only available on Linux.".into());
        }
        #[cfg(target_os = "macos")]
        {
            let mut command = Command::new("pbpaste");
            command.args(["-Prefer", "txt"]).env("LC_CTYPE", "en_US.UTF-8");
            Ok(command)
        }
        #[cfg(windows)]
        {
            let mut command = Command::new("powershell.exe");
            command.args(["-NoProfile", "-NonInteractive", "-STA", "-Command",
                "Add-Type -AssemblyName System.Windows.Forms; [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); if (![System.Windows.Forms.Clipboard]::ContainsText()) { exit 2 }; [Console]::Write([System.Windows.Forms.Clipboard]::GetText())"]);
            Ok(command)
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        Err("Desktop text paste is unavailable on this platform.".into())
    }
}
