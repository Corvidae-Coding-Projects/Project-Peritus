use super::{
    InventoryStore, MaterializationPhase, MaterializationRequest, MaterializationState, OWNER_FLAG,
    PAGE_BYTES, PAGE_RECORDS, PageReceipt, SCHEMA_VERSION,
};
use crate::{
    error::{Result, problem, uncertain},
    git::effects,
    state::{hex, save},
};

pub(super) struct Launched {
    child: tokio::process::Child,
    binding: effects::OwnerBinding,
    armed: bool,
}

impl Launched {
    pub(super) fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.child.try_wait()
    }

    pub(super) async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        self.child.wait().await
    }

    pub(super) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for Launched {
    fn drop(&mut self) {
        if self.armed {
            let _ = effects::terminate_detached_owner(&self.binding);
            let _ = self.child.start_kill();
        }
    }
}
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::Path,
    process::Stdio,
};

pub(super) async fn launch(
    store: &InventoryStore,
) -> Result<(String, Launched)> {
    let (request, path) = store.create_request()?;
    let mut command = tokio::process::Command::new(std::env::current_exe()?);
    command
        .arg(OWNER_FLAG)
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    effects::configure_detached_owner(&mut command);
    let mut child = command.spawn()?;
    let launch = child
        .id()
        .ok_or_else(|| problem("The Git inventory owner has no operating-system identity"))
        .and_then(effects::detached_launcher_binding);
    let binding = match launch {
        Ok(binding) => binding,
        Err(error) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(error);
        }
    };
    if let Err(error) = (|| {
        save(
            &path.with_file_name("launch.json"),
            &serde_json::to_vec(&binding).map_err(problem)?,
        )
    })() {
        let _ = child.kill().await;
        let _ = child.wait().await;
        return Err(error);
    }
    Ok((request.snapshot, Launched { child, binding, armed: true }))
}

pub(super) fn run(request_path: &Path) -> Result<()> {
    let request_path = request_path.canonicalize()?;
    if request_path.file_name().and_then(|name| name.to_str()) != Some("request.json") {
        return Err(problem("The Git inventory owner received an invalid request path"));
    }
    let request: MaterializationRequest =
        serde_json::from_slice(&std::fs::read(&request_path)?)?;
    let store = InventoryStore::from_request(&request_path, &request)?;
    if request.repository.canonicalize()? != request.repository || !request.repository.is_dir() {
        return Err(problem("The Git inventory repository binding is no longer canonical"));
    }
    let directory = request_path
        .parent()
        .ok_or_else(|| problem("The Git inventory request has no directory"))?;
    let (mut containment, owner) = effects::activate_detached_owner(&request.job_name)?;
    let mut state = MaterializationState {
        schema_version: SCHEMA_VERSION,
        workspace: request.workspace.clone(),
        store: request.store.clone(),
        repository: request.repository.clone(),
        snapshot: request.snapshot.clone(),
        owner,
        phase: MaterializationPhase::Running,
        pages: 0,
        bytes: 0,
        digest: None,
        error: None,
    };
    save_state(directory, &state)?;
    match create(&store, directory, &request.snapshot) {
        Ok((bytes, digest, pages)) => {
            state.phase = MaterializationPhase::Completed;
            state.bytes = bytes;
            state.pages = pages;
            state.digest = Some(digest);
        }
        Err(error) => {
            state.phase = MaterializationPhase::Failed;
            state.error = Some(error.0);
        }
    }
    save_state(directory, &state)?;
    effects::complete_detached_owner(&mut containment);
    Ok(())
}

fn create(store: &InventoryStore, directory: &Path, snapshot: &str) -> Result<(u64, String, u64)> {
    let mut records = RecordWriter::new(directory, snapshot)?;
    materialize_remote_names(store, &mut records)?;
    materialize_remote_urls(store, &mut records)?;
    materialize_refs(store, &mut records)?;
    records.finish()
}

fn materialize_remote_names(store: &InventoryStore, records: &mut RecordWriter) -> Result<()> {
    stream_git(&store.repository, &["remote"], b'\n', false, |record| {
        let name = utf8_trimmed(record, b'\n')?;
        if !name.is_empty() {
            records.write(&json!({"kind":"remote","name":name,"fetch":[],"push":[]}))?;
        }
        Ok(())
    })
}

fn materialize_remote_urls(store: &InventoryStore, records: &mut RecordWriter) -> Result<()> {
    stream_git(
        &store.repository,
        &["config", "--null", "--get-regexp", "^remote\\..*\\.(url|pushurl)$"],
        b'\0',
        true,
        |record| {
            let entry = utf8_trimmed(record, b'\0')?;
            let Some((key, value)) = entry.split_once('\n') else {
                return Err(problem("Git returned a malformed remote configuration record"));
            };
            let Some(key) = key.strip_prefix("remote.") else { return Ok(()) };
            let (name, push) = if let Some(name) = key.strip_suffix(".pushurl") {
                (name, true)
            } else if let Some(name) = key.strip_suffix(".url") {
                (name, false)
            } else {
                return Ok(());
            };
            records.write(&json!({
                "kind":"remote","name":name,
                "fetch":if push { Vec::<String>::new() } else { vec![value.to_owned()] },
                "push":if push { vec![value.to_owned()] } else { Vec::<String>::new() }
            }))
        },
    )
}

fn materialize_refs(store: &InventoryStore, records: &mut RecordWriter) -> Result<()> {
    stream_git(
        &store.repository,
        &[
            "for-each-ref",
            "--format=%(refname)%00%(HEAD)%00%(upstream:short)%00%(symref)",
            "refs/heads",
            "refs/remotes",
        ],
        b'\n',
        false,
        |record| {
            let line = utf8_trimmed(record, b'\n')?;
            let fields = line.split('\0').collect::<Vec<_>>();
            if fields.len() != 4 {
                return Err(problem("Git returned a malformed reference inventory record"));
            }
            if !fields[3].is_empty() { return Ok(()) }
            let remote = fields[0].starts_with("refs/remotes/");
            let prefix = if remote { "refs/remotes/" } else { "refs/heads/" };
            let name = fields[0].strip_prefix(prefix).ok_or_else(|| {
                problem("Git returned a reference outside the requested inventory")
            })?;
            records.write(&json!({
                "kind":"branch","name":name,"ref":fields[0],"remote":remote,
                "current":fields[1]=="*","upstream":fields[2]
            }))
        },
    )
}

fn stream_git(
    repository: &Path,
    args: &[&str],
    delimiter: u8,
    status_one_is_empty: bool,
    mut record: impl FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    let stderr = tempfile::tempfile()?;
    let mut command = std::process::Command::new("git");
    command
        .current_dir(repository)
        .arg("--no-pager")
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(stderr.try_clone()?));
    let mut child = command.spawn()?;
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(problem("Git inventory stdout was not captured"));
        }
    };
    let mut reader = BufReader::new(stdout);
    let consumed = (|| -> Result<()> {
        loop {
            let mut bytes = Vec::new();
            if reader.read_until(delimiter, &mut bytes)? == 0 { return Ok(()) }
            if bytes.last().copied() != Some(delimiter) {
                return Err(problem("Git returned an unterminated inventory record"));
            }
            record(&bytes)?;
        }
    })();
    if consumed.is_err() { let _ = child.kill(); }
    let status = child.wait()?;
    consumed?;
    if status.success() || status_one_is_empty && status.code() == Some(1) { return Ok(()) }
    let mut stderr = stderr;
    stderr.seek(SeekFrom::Start(0))?;
    let mut detail = Vec::new();
    stderr.take(u64::try_from(PAGE_BYTES).map_err(problem)?).read_to_end(&mut detail)?;
    let detail = String::from_utf8_lossy(&detail);
    Err(problem(if detail.trim().is_empty() {
        format!("Git inventory command failed ({status})")
    } else {
        detail.into_owned()
    }))
}

fn utf8_trimmed(bytes: &[u8], delimiter: u8) -> Result<&str> {
    let bytes = bytes
        .strip_suffix(&[delimiter])
        .ok_or_else(|| problem("Git inventory record delimiter is missing"))?;
    std::str::from_utf8(bytes).map_err(problem)
}

struct RecordWriter {
    file: File,
    directory: std::path::PathBuf,
    snapshot: String,
    digest: Sha256,
    bytes: u64,
    pages: u64,
    page_offset: u64,
    page_bytes: usize,
    page_records: usize,
    page_digest: Sha256,
}

impl RecordWriter {
    fn new(directory: &Path, snapshot: &str) -> Result<Self> {
        Ok(Self {
            file: OpenOptions::new().write(true).create_new(true).open(directory.join("records.jsonl"))?,
            directory: directory.to_owned(), snapshot: snapshot.to_owned(), digest: Sha256::new(),
            bytes: 0, pages: 0, page_offset: 0, page_bytes: 0, page_records: 0,
            page_digest: Sha256::new(),
        })
    }

    fn write(&mut self, value: &Value) -> Result<()> {
        let mut bytes = serde_json::to_vec(value)?;
        bytes.push(b'\n');
        if self.page_records != 0
            && (self.page_records == PAGE_RECORDS
                || self.page_bytes.saturating_add(bytes.len()) > PAGE_BYTES)
        {
            self.finish_page()?;
        }
        self.file.write_all(&bytes)?;
        self.digest.update(&bytes);self.page_digest.update(&bytes);
        self.page_bytes = self.page_bytes.saturating_add(bytes.len());
        self.page_records += 1;
        self.bytes = self.bytes.checked_add(u64::try_from(bytes.len()).map_err(problem)?)
            .ok_or_else(|| problem("Git inventory materialization length overflow"))?;
        Ok(())
    }

    fn finish(mut self) -> Result<(u64, String, u64)> {
        self.finish_page()?;
        self.file.flush()?;self.file.sync_all()?;sync_directory(&self.directory)?;
        Ok((self.bytes, hex(&self.digest.finalize()), self.pages))
    }

    fn finish_page(&mut self) -> Result<()> {
        if self.page_records == 0 { return Ok(()) }
        self.file.flush()?;self.file.sync_data()?;
        let receipt = PageReceipt {
            schema_version: SCHEMA_VERSION, snapshot: self.snapshot.clone(), index: self.pages,
            offset: self.page_offset, bytes: u64::try_from(self.page_bytes).map_err(problem)?,
            records: self.page_records, digest: hex(&self.page_digest.clone().finalize()),
        };
        save(
            &self.directory.join("pages").join(format!("{:020}.json", self.pages)),
            &serde_json::to_vec(&receipt)?,
        )?;
        sync_directory(&self.directory.join("pages"))?;
        self.pages = self.pages.checked_add(1).ok_or_else(|| problem("Too many Git inventory pages"))?;
        self.page_offset = self.bytes;self.page_bytes = 0;self.page_records = 0;
        self.page_digest = Sha256::new();
        Ok(())
    }
}

fn save_state(directory: &Path, state: &MaterializationState) -> Result<()> {
    save(&directory.join("state.json"), &serde_json::to_vec(state)?)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all().map_err(uncertain)
}

#[cfg(not(unix))]
fn sync_directory(path: &Path) -> Result<()> {
    let _ = std::fs::metadata(path)?;Ok(())
}
