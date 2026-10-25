//! Direct external-editor execution while the runtime lends it the terminal.

use std::process::Stdio;

use tokio::{io::AsyncReadExt as _, process::Command};

use crate::local::LocalReply;

/// The caller owns terminal suspension and keeps draining the daemon while this is pending.
pub(super) async fn execute(text: String) -> Result<LocalReply, String> {
    let configured = std::env::var("VISUAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| std::env::var("EDITOR").ok().filter(|value| !value.trim().is_empty()))
        .unwrap_or_else(|| if cfg!(windows) { "notepad.exe".into() } else { "vi".into() });
    let words = shell_words::split(&configured)
        .map_err(|error| format!("Cannot parse VISUAL/EDITOR as a direct command: {error}"))?;
    let (executable, arguments) = words
        .split_first()
        .ok_or_else(|| "VISUAL/EDITOR does not name an editor executable.".to_owned())?;
    let directory = super::local::private_directory("editor-")?;
    let path = directory.path().join("prompt.md");
    super::local::write_private(&path, text.as_bytes())?;
    let status = Command::new(executable)
        .args(arguments)
        .arg(&path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("Cannot start external editor {executable}: {error}"))?
        .wait()
        .await
        .map_err(|error| format!("Cannot collect external editor: {error}"))?;
    if !status.success() {
        return Err(format!(
            "External editor exited with {status}; the original unsent draft was retained."
        ));
    }
    let metadata = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|error| format!("Cannot inspect the edited draft: {error}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("The editor replaced the draft with a nonregular file; the original unsent draft was retained.".into());
    }
    let file = crate::image_import::open_regular(&path)
        .map_err(|error| format!("Cannot safely reopen edited draft: {error}"))?;
    let mut file = tokio::fs::File::from_std(file);
    let mut edited = String::new();
    file.read_to_string(&mut edited).await.map_err(|error| {
        format!("Cannot read edited UTF-8 draft: {error}; original draft retained")
    })?;
    Ok(LocalReply::EditedDraft(edited))
}
