//! Immutable local selections. Viewing never attaches; sending names exact retained snapshots.
use crate::{
    error::{Result, problem},
    state::{App, id},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{
    io::{Read, Write},
    path::PathBuf,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
    let mut file = std::fs::File::open(path)?;
    let before = file.metadata()?;
    if !before.is_file() {
        return Err(problem("Choose a regular UTF-8 text file"));
    }
    let directory = directory(app)?;
    std::fs::create_dir_all(&directory)?;
    let mut snapshot = tempfile::NamedTempFile::new_in(&directory)?;
    let (bytes, digest) = copy_snapshot(&mut file, snapshot.as_file_mut())?;
    let after = file.metadata()?;
    if bytes != before.len()
        || after.len() != before.len()
        || after.modified()? != before.modified()?
    {
        return Err(problem("The source changed while taking its snapshot; attach it again."));
    }
    snapshot.as_file_mut().sync_all()?;
    let attachment = Attachment {
        id: id()?,
        session: session.id,
        project: project.id,
        path: relative.to_owned(),
        bytes,
        digest,
        media: "text/plain".to_owned(),
    };
    let retained_path = directory.join(&attachment.id);
    snapshot.persist_noclobber(&retained_path).map_err(problem)?;
    if let Err(error) = app.update(|state| {
        state.attachments.insert(attachment.id.clone(), attachment.clone());
        Ok(())
    }) {
        std::fs::remove_file(&retained_path).map_err(|cleanup| {
            problem(format!("Snapshot metadata could not be retained: {error}; snapshot cleanup failed: {cleanup}"))
        })?;
        return Err(error);
    }
    Ok(json!(attachment))
}
fn copy_snapshot(source: &mut impl Read, destination: &mut impl Write) -> Result<(u64, String)> {
    let mut hash = Sha256::new();
    let mut count = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut incomplete = Vec::new();
    loop {
        let length = source.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        count = count
            .checked_add(length as u64)
            .ok_or_else(|| problem("Snapshot length is not representable"))?;
        hash.update(&buffer[..length]);
        destination.write_all(&buffer[..length])?;
        incomplete.extend_from_slice(&buffer[..length]);
        let valid = match std::str::from_utf8(&incomplete) {
            Ok(text) => {
                validate_text(text)?;
                incomplete.len()
            }
            Err(error) if error.error_len().is_none() => {
                let text =
                    std::str::from_utf8(&incomplete[..error.valid_up_to()]).map_err(problem)?;
                validate_text(text)?;
                error.valid_up_to()
            }
            Err(_) => return Err(problem("Native text attachments must contain valid UTF-8.")),
        };
        incomplete.drain(..valid);
    }
    if !incomplete.is_empty() {
        return Err(problem("Native text attachment ends with incomplete UTF-8."));
    }
    Ok((count, crate::state::hex(&hash.finalize())))
}
fn validate_text(text: &str) -> Result<()> {
    if text.chars().any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')) {
        return Err(problem(
            "Native text attachments cannot contain binary or terminal control characters.",
        ));
    }
    Ok(())
}

/// Removes both stored bytes and metadata, retaining snapshots needed by unresolved sends.
pub fn remove(app: &App, input: &Value) -> Result<Value> {
    let id =
        input["attachment"].as_str().ok_or_else(|| problem("Choose an attachment snapshot"))?;
    valid_id(id)?;
    let path = directory(app)?.join(id);
    app.update(|state| {
        let Some(attachment) = state.attachments.get(id) else { return Ok(()) };
        if Some(attachment.session.as_str()) != input["session"].as_str() {
            return Err(problem("Attachment belongs to a different session"));
        }
        if state.operations.values().any(|operation| {
            operation.result.is_none()
                && operation.input["command"] == "send"
                && operation.input["attachments"]
                    .as_array()
                    .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(id)))
        }) {
            return Err(problem("Resolve the pending message before removing its snapshot."));
        }
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        state.attachments.remove(id);
        Ok(())
    })?;
    Ok(json!({"removed":id}))
}

fn valid_id(id: &str) -> Result<()> {
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(problem("Invalid attachment identity"));
    }
    Ok(())
}

/// Opens the exact retained local snapshot for streaming transfer; the receiver checks its digest.
pub fn open(app: &App, attachment: &Attachment) -> Result<std::fs::File> {
    valid_id(&attachment.id)?;
    let retained = app.snapshot()?.attachments.get(&attachment.id).cloned();
    if retained.as_ref() != Some(attachment) {
        return Err(problem("Attachment snapshot metadata changed"));
    }
    let file = std::fs::File::open(directory(app)?.join(&attachment.id))?;
    if file.metadata()?.len() != attachment.bytes {
        return Err(problem("Attachment snapshot length changed"));
    }
    Ok(file)
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_stream_past_old_ceiling_and_validate_split_utf8() {
        let text = format!("{}λ界\r\n", "x".repeat(256 * 1024 - 1));
        let mut output = Vec::new();
        let (length, digest) = copy_snapshot(&mut text.as_bytes(), &mut output).unwrap();
        assert_eq!(output, text.as_bytes());
        assert_eq!(length, text.len() as u64);
        assert_eq!(digest, super::super::revision(text.as_bytes()));
        assert!(copy_snapshot(&mut b"audio\0bytes".as_slice(), &mut Vec::new()).is_err());
        assert!(copy_snapshot(&mut b"incomplete\xc3".as_slice(), &mut Vec::new()).is_err());
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn many_snapshots_can_be_removed_with_bytes_and_pending_sends_keep_exact_sources() {
        let root = tempfile::tempdir().unwrap();
        let app = App::open(
            crate::config::Options {
                port: 4173,
                root: root.path().to_owned(),
                assets: root.path().join("assets"),
                config_file: root.path().join("webui.toml"),
                state_file: root.path().join("workspace.json"),
                daemon_config_root: root.path().to_owned(),
                product_state_root: root.path().to_owned(),
                daemon_config: None,
                endpoint: None,
                cli: "peritus".into(),
            },
            4173,
        )
        .unwrap();
        let session = app.snapshot().unwrap().sessions[0].clone();
        std::fs::write(root.path().join("source.txt"), "exact immutable λ界\n".repeat(10_000))
            .unwrap();
        let mut snapshots = Vec::new();
        for _ in 0..20 {
            snapshots.push(
                stage(
                    &app,
                    &json!({"session":session.id,"project":session.project,"path":"source.txt"}),
                )
                .unwrap(),
            );
        }
        assert_eq!(app.snapshot().unwrap().attachments.len(), 20);
        let attachment = snapshots[0]["id"].as_str().unwrap();
        let operation = id().unwrap();
        app.record_operation(
            operation.clone(),
            json!({"command":"send","attachments":[attachment]}),
        )
        .unwrap();
        let removal = json!({"session":session.id,"attachment":attachment});
        assert!(remove(&app, &removal).is_err());
        assert!(directory(&app).unwrap().join(attachment).is_file());
        app.update(|state| {
            state.operations.get_mut(&operation).unwrap().result = Some(json!({"settled":true}));
            Ok(())
        })
        .unwrap();
        remove(&app, &removal).unwrap();
        assert!(!directory(&app).unwrap().join(attachment).exists());
        assert!(!app.snapshot().unwrap().attachments.contains_key(attachment));
        remove(&app, &removal).unwrap();
        assert_eq!(app.snapshot().unwrap().attachments.len(), 19);
    }
}
