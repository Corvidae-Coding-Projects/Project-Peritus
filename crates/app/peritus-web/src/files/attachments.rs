//! Immutable local selections. Viewing never attaches; sending names exact retained snapshots.
use crate::{
    error::{Result, problem},
    state::{App, id, save},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{io::Read, path::PathBuf};

#[derive(Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub session: String,
    pub project: String,
    pub path: String,
    pub bytes: u64,
    pub digest: String,
    pub media: String,
}
pub fn directory(app: &App) -> Result<PathBuf> {
    Ok(app
        .options
        .state_file
        .parent()
        .ok_or_else(|| problem("No state directory"))?
        .join("attachments"))
}
pub fn stage(app: &App, input: &Value) -> Result<Value> {
    let session = app.session(input["session"].as_str().unwrap_or(""))?;
    if input["project"].as_str() != Some(session.project.as_str()) {
        return Err(problem("Attach files from this session's project only"));
    }
    let project = app.project(&session.project)?;
    let relative = input["path"].as_str().ok_or_else(|| problem("Choose a file"))?;
    if relative.len() > 1024 || relative.chars().any(char::is_control) {
        return Err(problem("The file label is too long or contains control characters"));
    }
    let path = super::resolve(&project.root, relative)?;
    let file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > 48 * 1024 {
        return Err(problem(
            "Native chat supports UTF-8 file snapshots up to 48 KiB each, with a 64 KiB combined message limit. Preview supports text up to 50 MiB.",
        ));
    }
    let mut content = Vec::new();
    file.take(48 * 1024 + 1).read_to_end(&mut content)?;
    let media = classify(&content)?;
    let retained: u64 = app.snapshot()?.attachments.values().map(|a| a.bytes).sum();
    if retained + content.len() as u64 > 256 * 1024 * 1024 {
        return Err(problem(
            "The local attachment cache reached 256 MiB. Remove unneeded attachment snapshots from console storage before adding more.",
        ));
    }
    let attachment = Attachment {
        id: id()?,
        session: session.id,
        project: project.id,
        path: relative.to_owned(),
        bytes: content.len() as u64,
        digest: super::revision(&content),
        media: media.to_owned(),
    };
    save(&directory(app)?.join(&attachment.id), &content)?;
    app.update(|state| {
        state.attachments.insert(attachment.id.clone(), attachment.clone());
        Ok(())
    })?;
    Ok(json!(attachment))
}
fn classify(bytes: &[u8]) -> Result<&'static str> {
    if bytes.len() > 48 * 1024 {
        return Err(problem("Native text attachment exceeds 48 KiB"));
    }
    if !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok() {
        return Ok("text/plain");
    }
    Err(problem(
        "Native chat currently accepts UTF-8 text attachments. Images, audio, and other binary files can be viewed or downloaded; their imports use the separate Workbench console.",
    ))
}
pub fn selected(app: &App, input: &Value) -> Result<Vec<Attachment>> {
    let state = app.snapshot()?;
    let ids = input["attachments"].as_array().cloned().unwrap_or_default();
    if ids.len() > 16 {
        return Err(problem("Attach at most 16 files to one message"));
    }
    let mut selected = Vec::new();
    for id in ids {
        let a = state.attachments.get(id.as_str().unwrap_or("")).ok_or_else(|| {
            problem("Attachment snapshot is missing; remove it and attach the file again")
        })?;
        if Some(a.session.as_str()) != input["session"].as_str() {
            return Err(problem("Attachment belongs to a different session"));
        }
        if selected.iter().any(|prior: &Attachment| prior.id == a.id) {
            return Err(problem("Duplicate attachment"));
        }
        selected.push(a.clone());
    }
    Ok(selected)
}
pub fn content(app: &App, attachment: &Attachment) -> Result<Vec<u8>> {
    if attachment.id.len() != 32 || !attachment.id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(problem("Invalid attachment identity"));
    }
    let bytes = std::fs::read(directory(app)?.join(&attachment.id))?;
    if bytes.len() as u64 != attachment.bytes || super::revision(&bytes) != attachment.digest {
        return Err(problem("Attachment snapshot changed. Remove and reattach it."));
    }
    Ok(bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsupported_and_oversized_are_explicit() {
        assert_eq!(classify(b"# text\n").unwrap(), "text/plain");
        assert!(classify(&vec![b'x'; 256 * 1024 + 1]).is_err());
        assert!(classify(b"audio\0bytes").is_err());
        assert!(classify(b"\x89PNG\r\n\x1a\nrest").is_err());
    }
}
