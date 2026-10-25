//! Versioned stage checkpoints, explicit dependency reads and exact retained output pages.
use crate::{Definition, Error, digest, invalid, publish};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

/// Committed output of one declared stage.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageOutput {
    /// Declared producing stage.
    pub stage: String,
    /// Digest of exact UTF-8 output bytes.
    pub sha256: String,
    /// Retained byte length.
    pub bytes: u64,
}
/// Small checkpoint; source text stays in separate immutable artifacts.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobState {
    /// Checkpoint format version.
    pub schema: u32,
    /// Selected module identity.
    pub module: String,
    /// Exact selected package digest.
    pub digest: String,
    /// Governing initial user revision.
    pub input_revision: u64,
    /// Original user request for this module job.
    pub task: String,
    /// Committed stages in manifest order.
    pub completed: Vec<StageOutput>,
}
/// Exclusive owner of one deterministic job. Closing it releases cross-process ownership.
pub struct Job {
    root: PathBuf,
    state: JobState,
    _lock: Option<fs::File>,
    definition: Definition,
}
impl Job {
    /// Opens/resumes a job under a caller-owned state root, bound to exact module/input identity.
    ///
    /// # Errors
    ///
    /// Returns an error when ownership, identity, checkpoint order, or retained output is invalid.
    pub fn open(
        root: &Path,
        definition: Definition,
        revision: u64,
        task: &str,
    ) -> Result<Self, Error> {
        definition.manifest.validate()?;
        if definition.digest.len() != 64
            || !definition.digest.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(invalid("invalid module identity digest"));
        }
        let key = digest(
            format!("{}:{}:{revision}:{task}", definition.manifest.id, definition.digest)
                .as_bytes(),
        );
        let root = root.join(key);
        fs::create_dir_all(&root)?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("job.lock"))?;
        fs4::FileExt::try_lock(&lock)
            .map_err(|e| invalid(format!("module job is already owned or unavailable: {e}")))?;
        let state = match fs::read(root.join("job.json")) {
            Ok(b) => serde_json::from_slice::<JobState>(&b)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => JobState {
                schema: 1,
                module: definition.manifest.id.clone(),
                digest: definition.digest.clone(),
                input_revision: revision,
                task: task.into(),
                completed: vec![],
            },
            Err(e) => return Err(e.into()),
        };
        if state.schema != 1
            || state.module != definition.manifest.id
            || state.digest != definition.digest
            || state.input_revision != revision
            || state.task != task
            || state.completed.len() > definition.manifest.stages.len()
        {
            return Err(invalid("module checkpoint identity/version differs"));
        }
        for (i, output) in state.completed.iter().enumerate() {
            if output.stage != definition.manifest.stages[i].id {
                return Err(invalid("module checkpoint stage order differs"));
            }
        }
        let job = Self { root, state, _lock: Some(lock), definition };
        for output in &job.state.completed {
            job.verify(output)?;
        }
        job.save()?;
        Ok(job)
    }
    /// Inspects a committed snapshot without taking ownership from an active job.
    ///
    /// # Errors
    ///
    /// Returns an error when the checkpoint or its retained output cannot be validated.
    pub fn inspect(
        root: &Path,
        definition: Definition,
        revision: u64,
        task: &str,
        offset: u64,
    ) -> Result<Value, Error> {
        definition.manifest.validate()?;
        let key = digest(
            format!("{}:{}:{revision}:{task}", definition.manifest.id, definition.digest)
                .as_bytes(),
        );
        let root = root.join(key);
        let state: JobState = serde_json::from_slice(&fs::read(root.join("job.json"))?)?;
        if state.schema != 1
            || state.module != definition.manifest.id
            || state.digest != definition.digest
            || state.input_revision != revision
            || state.task != task
            || state.completed.len() > definition.manifest.stages.len()
        {
            return Err(invalid("module checkpoint identity differs"));
        }
        for (i, output) in state.completed.iter().enumerate() {
            if output.stage != definition.manifest.stages[i].id {
                return Err(invalid("module checkpoint stage order differs"));
            }
        }
        let job = Self { root, state, _lock: None, definition };
        let page = match job.state.completed.last() {
            Some(output) => {
                job.read(std::slice::from_ref(&output.stage), &output.stage, offset, 32768)?
            }
            None => json!(null),
        };
        Ok(
            json!({"module":job.state.module,"digest":job.state.digest,"completed_stages":job.state.completed,"stage_count":job.definition.manifest.stages.len(),"page":page,"directory":job.root}),
        )
    }

    /// Current committed stages and exact governing inputs.
    #[must_use]
    pub const fn state(&self) -> &JobState {
        &self.state
    }
    /// Private job directory for progress/report observations.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn save(&self) -> Result<(), Error> {
        publish(&self.root.join("job.json"), &serde_json::to_vec_pretty(&self.state)?)
    }
    /// Publishes the next declared stage and only then advances the checkpoint.
    ///
    /// # Errors
    ///
    /// Returns an error when the stage is out of order, empty, or cannot be durably published.
    pub fn complete(&mut self, stage: &str, text: &str) -> Result<(), Error> {
        let expected = self
            .definition
            .manifest
            .stages
            .get(self.state.completed.len())
            .ok_or_else(|| invalid("module job is already complete"))?;
        if stage != expected.id || text.trim().is_empty() {
            return Err(invalid("module stage is out of order or returned empty output"));
        }
        let sha256 = digest(text.as_bytes());
        let artifact = self.root.join(format!("{stage}-{sha256}.txt"));
        publish(&artifact, text.as_bytes())?;
        let mut next = self.state.clone();
        next.completed.push(StageOutput { stage: stage.into(), sha256, bytes: text.len() as u64 });
        publish(&self.root.join("job.json"), &serde_json::to_vec_pretty(&next)?)?;
        self.state = next;
        Ok(())
    }
    fn output(&self, id: &str) -> Result<&StageOutput, Error> {
        self.state
            .completed
            .iter()
            .find(|o| o.stage == id)
            .ok_or_else(|| invalid("stage output has not been committed"))
    }
    fn path(&self, output: &StageOutput) -> PathBuf {
        self.root.join(format!("{}-{}.txt", output.stage, output.sha256))
    }
    fn verify(&self, output: &StageOutput) -> Result<(), Error> {
        if output.sha256.len() != 64
            || !output.sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(invalid("invalid module artifact digest"));
        }
        let mut file = fs::File::open(self.path(output))?;
        if file.metadata()?.len() != output.bytes {
            return Err(invalid("module output length changed"));
        }
        let mut hash = Sha256::new();
        let mut buf = vec![0u8; 32768];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
        }
        let actual = crate::hex(&hash.finalize());
        if actual != output.sha256 {
            return Err(invalid("module output integrity mismatch"));
        }
        Ok(())
    }
    /// Reads only outputs explicitly admitted by the current stage's dependency list.
    ///
    /// # Errors
    ///
    /// Returns an error for undeclared access, integrity drift, or invalid UTF-8 paging.
    pub fn read(
        &self,
        allowed: &[String],
        id: &str,
        offset: u64,
        max_bytes: usize,
    ) -> Result<Value, Error> {
        if !allowed.iter().any(|s| s == id) {
            return Err(invalid("this stage cannot read undeclared predecessor output"));
        }
        let output = self.output(id)?;
        self.verify(output)?;
        let mut file = fs::File::open(self.path(output))?;
        if offset > output.bytes {
            return Err(invalid("module output offset is beyond its retained bytes"));
        }
        file.seek(SeekFrom::Start(offset))?;
        let remaining = usize::try_from(output.bytes - offset).unwrap_or(usize::MAX);
        let mut bytes = vec![0; max_bytes.clamp(4, 1024 * 1024).min(remaining)];
        file.read_exact(&mut bytes)?;
        let text = match std::str::from_utf8(&bytes) {
            Ok(s) => s,
            Err(e) if e.error_len().is_none() => std::str::from_utf8(&bytes[..e.valid_up_to()])
                .map_err(|_| invalid("invalid retained UTF-8"))?,
            Err(_) => return Err(invalid("offset must be an exact UTF-8 boundary")),
        };
        let next = offset + text.len() as u64;
        Ok(
            json!({"stage":id,"sha256":output.sha256,"total_bytes":output.bytes,"offset":offset,"text":text,"next_offset":(next<output.bytes).then_some(next)}),
        )
    }
    /// Exact final output for human presentation; model inputs use paged dependency reads.
    ///
    /// # Errors
    ///
    /// Returns an error when the final artifact is absent, unreadable, or fails integrity checks.
    pub fn result(&self) -> Result<Option<String>, Error> {
        if self.state.completed.len() != self.definition.manifest.stages.len() {
            return Ok(None);
        }
        let output = self.state.completed.last().ok_or_else(|| invalid("module has no stages"))?;
        self.verify(output)?;
        Ok(Some(fs::read_to_string(self.path(output))?))
    }
}
