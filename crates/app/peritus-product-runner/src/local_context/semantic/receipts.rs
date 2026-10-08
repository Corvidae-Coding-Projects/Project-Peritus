//! Optional inference receipts have their own durable owner, outside context admission.

use peritus_codec::sha256;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

const TRANSFER_BYTES: usize = 64 * 1024;
const DIAGNOSTIC_TAIL_BYTES: usize = 16 * 1024;
const RECEIPT_SCHEMA_VERSION: u16 = 3;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Intent {
    ordinal: u64,
    frontier: u64,
    input_digest: [u8; 32],
    input_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proposal_digest: Option<[u8; 32]>,
    next: Cursor,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct Cursor {
    pub(super) source: u64,
    pub(super) offset: u64,
    pub(super) entry: u64,
    pub(super) entry_id: Option<[u8; 16]>,
    pub(super) entry_offset: u64,
    pub(super) entry_digest: Option<[u8; 32]>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Outcome {
    Applied,
    Unavailable,
    Rejected,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema_version: u16,
    scope: [u8; 32],
    intent: Intent,
    outcome: Outcome,
    output_digest: Option<[u8; 32]>,
    output_bytes: Option<u64>,
    diagnostic_digest: Option<[u8; 32]>,
    diagnostic_bytes: Option<u64>,
    previous: Option<[u8; 32]>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Head {
    schema_version: u16,
    scope: [u8; 32],
    ordinal: u64,
    failures: u64,
    completed: Option<[u8; 32]>,
    pending: Option<Intent>,
    cursor: Cursor,
}

pub(super) struct Receipts {
    directory: PathBuf,
    head: Head,
}

pub(super) struct CapturedOutput {
    file: File,
    digest: [u8; 32],
    bytes: u64,
}

impl CapturedOutput {
    pub(super) fn reader(&mut self) -> Result<CapturedOutputReader<'_>, String> {
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|error| error.to_string())?;
        Ok(CapturedOutputReader {
            file: &mut self.file,
            expected_digest: self.digest,
            expected_bytes: self.bytes,
            observed_digest: Sha256::new(),
            observed_bytes: 0,
            verified: false,
        })
    }

    pub(super) const fn bytes(&self) -> u64 {
        self.bytes
    }

    pub(super) const fn digest(&self) -> [u8; 32] {
        self.digest
    }
}

pub(super) struct CapturedOutputReader<'a> {
    file: &'a mut File,
    expected_digest: [u8; 32],
    expected_bytes: u64,
    observed_digest: Sha256,
    observed_bytes: u64,
    verified: bool,
}

impl CapturedOutputReader<'_> {
    pub(super) const fn verified(&self) -> bool {
        self.verified
    }
}

impl Read for CapturedOutputReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.verified {
            return Ok(0);
        }
        let read = self.file.read(buffer)?;
        if read == 0 {
            let digest: [u8; 32] = self.observed_digest.clone().finalize().into();
            if self.observed_bytes != self.expected_bytes || digest != self.expected_digest {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "optional inference output changed after durable capture",
                ));
            }
            self.verified = true;
            return Ok(0);
        }
        self.observed_bytes = self
            .observed_bytes
            .checked_add(u64::try_from(read).map_err(io::Error::other)?)
            .ok_or_else(|| io::Error::other("optional inference output size overflow"))?;
        if self.observed_bytes > self.expected_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "optional inference output exceeds its durable capture",
            ));
        }
        self.observed_digest.update(&buffer[..read]);
        Ok(read)
    }
}

#[derive(Clone, Copy)]
struct CapturedDiagnostic {
    digest: [u8; 32],
    bytes: u64,
}

impl Receipts {
    pub(super) fn open(root: &Path, scope: [u8; 32]) -> Result<Self, String> {
        let directory = root.join("optional-inference");
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        sync_directory(root)?;
        let head = match read_json::<Head>(&directory.join("head.json")) {
            Ok(head) => head,
            Err(ReadError::Missing) => Head {
                schema_version: RECEIPT_SCHEMA_VERSION, scope, ordinal: 0, failures: 0,
                completed: None, pending: None, cursor: Cursor::default(),
            },
            Err(ReadError::Invalid(reason)) => return Err(reason),
        };
        if !matches!(head.schema_version, 1..=RECEIPT_SCHEMA_VERSION) || head.scope != scope
            || head.pending.as_ref().is_some_and(|intent| intent.ordinal != head.ordinal)
        {
            return Err("optional inference receipt identity mismatch".to_owned());
        }
        if let Some(digest) = head.completed {
            let bytes = read_bytes(&directory.join(format!("{}.json", hex(&digest))))
                .map_err(read_reason)?;
            if sha256(&bytes).into_bytes() != digest {
                return Err("optional inference completion digest mismatch".to_owned());
            }
            let receipt: Receipt = canonical(&bytes)?;
            if !matches!(receipt.schema_version, 1..=RECEIPT_SCHEMA_VERSION)
                || receipt.scope != scope
                || receipt.intent.ordinal > head.ordinal
            {
                return Err("optional inference completion lineage mismatch".to_owned());
            }
        }
        let mut owner = Self { directory, head };
        if let Some(intent) = owner.head.pending.as_ref() {
            // The sandbox command owns process recovery. An intent-only optional
            // attempt never authorizes re-dispatch or a context-state mutation.
            let output = observe_output(&owner.directory, intent.ordinal)?;
            let diagnostic = observe_diagnostic(&owner.directory, intent.ordinal).or_else(|| {
                owner.capture_diagnostic(
                    intent.ordinal,
                    "optional attempt completion was not durably published",
                )
            });
            owner.complete_captured(Outcome::Unknown, output.as_ref(), diagnostic)?;
        }
        Ok(owner)
    }

    pub(super) const fn failures(&self) -> u64 { self.head.failures }
    pub(super) const fn cursor(&self) -> Cursor { self.head.cursor }

    pub(super) fn begin(&mut self, frontier: u64, input: &[u8], next_cursor: Cursor, proposal_digest: Option<[u8; 32]>) -> Result<(), String> {
        if self.head.pending.is_some() {
            return Err("optional inference attempt remains owned".to_owned());
        }
        let ordinal = self.head.ordinal.checked_add(1)
            .ok_or_else(|| "optional inference receipt sequence overflow".to_owned())?;
        let intent = Intent {
            ordinal, frontier, input_digest: sha256(input).into_bytes(),
            input_bytes: u64::try_from(input.len()).map_err(|e| e.to_string())?,
            proposal_digest,
            next: next_cursor,
        };
        atomic_write(&self.directory, &format!("{ordinal}.input"), input)?;
        let mut next = self.head.clone();
        next.schema_version = RECEIPT_SCHEMA_VERSION;
        next.ordinal = ordinal;
        next.pending = Some(intent);
        self.publish(next)
    }

    pub(super) fn capture_output(
        &self,
        mut output: impl Read,
    ) -> Result<CapturedOutput, String> {
        let intent = self
            .head
            .pending
            .as_ref()
            .ok_or_else(|| "optional inference output has no owned intent".to_owned())?;
        let target = self.directory.join(format!("{}.output", intent.ordinal));
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)
            .map_err(|error| error.to_string())?;
        let mut hasher = Sha256::new();
        let mut bytes = 0_u64;
        let mut buffer = vec![0_u8; TRANSFER_BYTES];
        loop {
            let read = output.read(&mut buffer).map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            temporary
                .write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
            hasher.update(&buffer[..read]);
            bytes = bytes
                .checked_add(u64::try_from(read).map_err(|error| error.to_string())?)
                .ok_or_else(|| "optional inference output size overflow".to_owned())?;
        }
        temporary.as_file().sync_all().map_err(|error| error.to_string())?;
        let mut file = temporary.persist(&target).map_err(|error| error.to_string())?;
        sync_directory(&self.directory)?;
        if file.metadata().map_err(|error| error.to_string())?.len() != bytes {
            return Err("optional inference output changed while publishing".to_owned());
        }
        file.seek(SeekFrom::Start(0)).map_err(|error| error.to_string())?;
        Ok(CapturedOutput { file, digest: hasher.finalize().into(), bytes })
    }

    pub(super) fn complete_output(
        &mut self,
        outcome: Outcome,
        output: Option<&CapturedOutput>,
        diagnostic: Option<&str>,
    ) -> Result<(), String> {
        let diagnostic = self
            .head
            .pending
            .as_ref()
            .and_then(|intent| diagnostic.and_then(|text| self.capture_diagnostic(intent.ordinal, text)));
        self.complete_captured(outcome, output, diagnostic)
    }

    fn complete_captured(
        &mut self,
        outcome: Outcome,
        output: Option<&CapturedOutput>,
        diagnostic: Option<CapturedDiagnostic>,
    ) -> Result<(), String> {
        let intent = self.head.pending.clone()
            .ok_or_else(|| "optional inference completion has no owned intent".to_owned())?;
        let receipt = Receipt {
            schema_version: RECEIPT_SCHEMA_VERSION, scope: self.head.scope, intent, outcome,
            output_digest: output.map(|captured| captured.digest),
            output_bytes: output.map(|captured| captured.bytes),
            diagnostic_digest: diagnostic.map(|captured| captured.digest),
            diagnostic_bytes: diagnostic.map(|captured| captured.bytes),
            previous: self.head.completed,
        };
        let bytes = serde_json::to_vec(&receipt).map_err(|e| e.to_string())?;
        let digest = sha256(&bytes).into_bytes();
        atomic_write(&self.directory, &format!("{}.json", hex(&digest)), &bytes)?;
        let mut next = self.head.clone();
        next.schema_version = RECEIPT_SCHEMA_VERSION;
        next.completed = Some(digest);
        next.pending = None;
        next.cursor = receipt.intent.next;
        if outcome != Outcome::Applied {
            next.failures = next.failures.checked_add(1)
                .ok_or_else(|| "optional inference failure counter overflow".to_owned())?;
        }
        self.publish(next)
    }

    fn capture_diagnostic(
        &self,
        ordinal: u64,
        diagnostic: &str,
    ) -> Option<CapturedDiagnostic> {
        let tail = diagnostic_tail(diagnostic);
        atomic_write(&self.directory, &format!("{ordinal}.diagnostic"), tail.as_bytes()).ok()?;
        Some(CapturedDiagnostic {
            digest: sha256(tail.as_bytes()).into_bytes(),
            bytes: u64::try_from(tail.len()).ok()?,
        })
    }

    fn publish(&mut self, next: Head) -> Result<(), String> {
        let bytes = serde_json::to_vec(&next).map_err(|e| e.to_string())?;
        atomic_write(&self.directory, "head.json", &bytes)?;
        self.head = next;
        Ok(())
    }
}

enum ReadError { Missing, Invalid(String) }

fn read_reason(error: ReadError) -> String {
    match error {
        ReadError::Missing => "optional inference receipt is missing".to_owned(),
        ReadError::Invalid(reason) => reason,
    }
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, ReadError> {
    let file = File::open(path).map_err(|error| if error.kind() == std::io::ErrorKind::NotFound {
        ReadError::Missing
    } else { ReadError::Invalid(error.to_string()) })?;
    // Only fixed-shape heads/receipts use this physical decode envelope. Input
    // and output bodies are separate files with no cumulative receipt limit.
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes).map_err(|e| ReadError::Invalid(e.to_string()))?;
    if bytes.len() > 4096 { return Err(ReadError::Invalid("invalid optional receipt envelope".to_owned())); }
    Ok(bytes)
}

fn canonical<T: serde::de::DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T, String> {
    let value = serde_json::from_slice::<T>(bytes).map_err(|e| e.to_string())?;
    if serde_json::to_vec(&value).map_err(|e| e.to_string())? != bytes {
        return Err("noncanonical optional inference receipt".to_owned());
    }
    Ok(value)
}

fn read_json<T: serde::de::DeserializeOwned + Serialize>(path: &Path) -> Result<T, ReadError> {
    canonical(&read_bytes(path)?).map_err(ReadError::Invalid)
}

fn atomic_write(directory: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(directory.join(name)).map_err(|e| e.to_string())?;
    sync_directory(directory)
}

fn observe_output(directory: &Path, ordinal: u64) -> Result<Option<CapturedOutput>, String> {
    let path = directory.join(format!("{ordinal}.output"));
    let mut file = match OpenOptions::new().read(true).write(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() {
        return Err("optional inference output is not a regular file".to_owned());
    }
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = vec![0_u8; TRANSFER_BYTES];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        bytes = bytes
            .checked_add(u64::try_from(read).map_err(|error| error.to_string())?)
            .ok_or_else(|| "optional inference output size overflow".to_owned())?;
    }
    if bytes != metadata.len() {
        return Err("optional inference output size changed while recovering".to_owned());
    }
    file.sync_all().map_err(|error| error.to_string())?;
    sync_directory(directory)?;
    file.seek(SeekFrom::Start(0)).map_err(|error| error.to_string())?;
    Ok(Some(CapturedOutput { file, digest: hasher.finalize().into(), bytes }))
}

fn observe_diagnostic(directory: &Path, ordinal: u64) -> Option<CapturedDiagnostic> {
    let path = directory.join(format!("{ordinal}.diagnostic"));
    let file = File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.file_type().is_file()
        || metadata.len() > u64::try_from(DIAGNOSTIC_TAIL_BYTES).ok()?
    {
        return None;
    }
    let limit = u64::try_from(DIAGNOSTIC_TAIL_BYTES).ok()?.checked_add(1)?;
    let mut bytes = Vec::with_capacity(DIAGNOSTIC_TAIL_BYTES);
    file.take(limit).read_to_end(&mut bytes).ok()?;
    if bytes.len() > DIAGNOSTIC_TAIL_BYTES {
        return None;
    }
    Some(CapturedDiagnostic {
        digest: sha256(&bytes).into_bytes(),
        bytes: u64::try_from(bytes.len()).ok()?,
    })
}

fn diagnostic_tail(diagnostic: &str) -> &str {
    if diagnostic.len() <= DIAGNOSTIC_TAIL_BYTES {
        return diagnostic;
    }
    let mut start = diagnostic.len() - DIAGNOSTIC_TAIL_BYTES;
    while !diagnostic.is_char_boundary(start) {
        start += 1;
    }
    &diagnostic[start..]
}

fn sync_directory(directory: &Path) -> Result<(), String> {
    #[cfg(windows)]
    let directory = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new().read(true).custom_flags(0x0200_0000).open(directory)
    };
    #[cfg(not(windows))]
    let directory = File::open(directory);
    directory.and_then(|file| file.sync_all()).map_err(|e| e.to_string())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
