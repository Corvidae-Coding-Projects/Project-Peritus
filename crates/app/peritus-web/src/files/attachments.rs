//! Immutable local selections. Viewing never attaches; sending names exact retained snapshots.
use crate::{
    error::{Result, problem},
    state::{App, id},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt as _;
mod snapshot;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub session: String,
    pub project: String,
    pub path: String,
    pub bytes: u64,
    pub digest: String,
    pub media: String,
    #[serde(default)]
    pub(crate) storage: Option<String>,
}
pub fn directory(app: &App) -> Result<PathBuf> {
    Ok(legacy_directory(app)?.join(storage(app)?))
}
fn legacy_directory(app: &App) -> Result<PathBuf> {
    Ok(app
        .options
        .state_file
        .parent()
        .ok_or_else(|| problem("No state directory"))?
        .join("attachments"))
}
fn storage(app: &App) -> Result<String> {
    use sha2::{Digest as _, Sha256};
    let state_file = app.options.state_file.canonicalize()?;
    let mut digest = Sha256::new();
    digest.update(b"peritus-web-attachment-store-v1\0");
    digest.update(app.snapshot()?.identity.as_bytes());
    digest.update([0]);
    digest.update(state_file.as_os_str().as_encoded_bytes());
    Ok(crate::state::hex(&digest.finalize()))
}
pub async fn stage(app: &App, input: &Value) -> Result<Value> {
    let session = app.session(input["session"].as_str().unwrap_or(""))?;
    if input["project"].as_str() != Some(session.project.as_str()) {
        return Err(problem("Attach files from this session's project only"));
    }
    let project = app.project(&session.project)?;
    let relative = input["path"].as_str().ok_or_else(|| problem("Choose a file"))?;
    if relative.is_empty() || relative.chars().any(char::is_control) {
        return Err(problem("The file label is empty or contains control characters"));
    }
    let mut source = super::stream::TextSource::open(project.root.clone(), relative.to_owned()).await?;
    let directory = directory(app)?;
    let staging = snapshot::temporary(directory.clone()).await?;
    let mut output = tokio::fs::File::from_std(staging.reopen()?);
    let mut completed = None;
    while let Some(page) = source.next().await? {
        if let Some(text) = page["text"].as_str() { output.write_all(text.as_bytes()).await?; }
        if page["complete"].as_bool() == Some(true) {
            completed = Some((
                page["bytes"].as_str().ok_or_else(|| problem("Missing snapshot length"))?
                    .parse::<u64>().map_err(problem)?,
                page["revision"].as_str().ok_or_else(|| problem("Missing snapshot digest"))?.to_owned(),
            ));
        }
    }
    let (bytes, digest) = completed.ok_or_else(|| problem("The file snapshot did not finish verification"))?;
    output.flush().await?;
    output.sync_all().await?;
    drop(output);
    let attachment = Attachment {
        id: id()?,
        session: session.id,
        project: project.id,
        path: relative.to_owned(),
        bytes,
        digest,
        media: "text/plain".to_owned(),
        storage: Some(storage(app)?),
    };
    snapshot::publish(staging, directory.join(&attachment.id)).await?;
    app.update(|state| {
        state.attachments.insert(attachment.id.clone(), attachment.clone());
        Ok(())
    })?;
    Ok(json!(attachment))
}
#[cfg(test)]
fn classify(bytes: &[u8]) -> Result<&'static str> {
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
pub(crate) fn path(app: &App, attachment: &Attachment) -> Result<PathBuf> {
    if attachment.id.len() != 32 || !attachment.id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(problem("Invalid attachment identity"));
    }
    match &attachment.storage {
        Some(owner) if owner == &storage(app)? => Ok(directory(app)?.join(&attachment.id)),
        Some(_) => Err(problem("The attachment belongs to another gateway snapshot store")),
        // Older metadata still owns its exact digest-bound snapshot at the legacy location.
        None => Ok(legacy_directory(app)?.join(&attachment.id)),
    }
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
