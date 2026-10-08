//! Exact retained terminal input bodies and their owner settlement state.

use crate::{error::{Result, problem}, state::save};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{io::Read, path::{Path, PathBuf}};

pub(crate) const INPUTS_DIRECTORY: &str = "inputs";
pub(crate) const INPUT_BODY_FILE: &str = "body.bin";
pub(super) const INPUT_RECEIPT_FILE: &str = "input.json";
const INPUT_PURPOSE_FILE: &str = "purpose.json";
const INPUT_REVIEW_FILE: &str = "review.json";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum InputState {
    Pending,
    Unknown,
    Settled,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InputReceipt {
    schema_version: u32,
    pub(crate) input: String,
    pub(crate) digest: String,
    pub(crate) bytes: u64,
    pub(crate) state: InputState,
}

impl InputReceipt {
    pub(crate) fn pending(input: String, digest: String, bytes: u64) -> Result<Self> {
        if !valid_input(&input) || !valid_digest(&digest) {
            return Err(problem("Terminal input identity or digest is invalid"));
        }
        Ok(Self { schema_version: 1, input, digest, bytes, state: InputState::Pending })
    }

    pub(crate) fn recoverable(console: &Path) -> Result<Option<RecoverableInput>> {
        let inputs = console.join(INPUTS_DIRECTORY);
        if !inputs.exists() {
            return Ok(None);
        }
        if !inputs.is_dir() {
            return Err(problem("Retained terminal input storage is not a directory"));
        }
        let mut selected: Option<Self> = None;
        for entry in std::fs::read_dir(inputs)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let input = entry
                .file_name()
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| problem("Retained terminal input identity is not UTF-8"))?;
            if !valid_input(&input) {
                return Err(problem("Retained terminal input identity is invalid"));
            }
            let Some(receipt) = Self::read(console, &input)? else { continue };
            if read_purpose(console, &input)? == Some(false)
                || receipt.state == InputState::Settled
                || receipt.state == InputState::Unknown && reviewed(console, &receipt)?
            {
                continue;
            }
            let replace = selected.as_ref().is_none_or(|current| {
                recovery_order(&receipt) < recovery_order(current)
            });
            if replace {
                selected = Some(receipt);
            }
        }
        selected.map(|receipt| {
            let body = receipt.read_body(console)?;
            Ok(RecoverableInput {
                input: receipt.input,
                digest: receipt.digest,
                state: receipt.state,
                body,
            })
        }).transpose()
    }

    fn read_body(&self, console: &Path) -> Result<String> {
        let mut body = std::fs::File::open(body_path(console, &self.input)?)?;
        if body.metadata()?.len() != self.bytes {
            return Err(problem("Retained terminal form input length conflicts with its receipt"));
        }
        let capacity = usize::try_from(self.bytes).map_err(problem)?;
        let mut bytes = Vec::with_capacity(capacity);
        body.read_to_end(&mut bytes)?;
        let observed = crate::state::hex(&Sha256::digest(&bytes));
        if observed != self.digest {
            return Err(problem("Retained terminal form input digest conflicts with its receipt"));
        }
        String::from_utf8(bytes).map_err(problem)
    }

    pub(crate) fn read(console: &Path, input: &str) -> Result<Option<Self>> {
        let path = receipt_path(console, input)?;
        if !path.is_file() {
            return Ok(None);
        }
        let value: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        value.validate()?;
        Ok(Some(value))
    }

    pub(crate) fn publish(&self, console: &Path) -> Result<()> {
        self.validate()?;
        save(&receipt_path(console, &self.input)?, &serde_json::to_vec(self)?)
    }

    fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || !valid_input(&self.input)
            || !valid_digest(&self.digest)
        {
            return Err(problem("Terminal input receipt is invalid"));
        }
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InputPurpose {
    schema_version: u32,
    input: String,
    form: bool,
}

pub(crate) fn read_purpose(console: &Path, input: &str) -> Result<Option<bool>> {
    let path = purpose_path(console, input)?;
    if !path.is_file() {
        return Ok(None);
    }
    let value: InputPurpose = serde_json::from_slice(&std::fs::read(path)?)?;
    if value.schema_version != 1 || value.input != input {
        return Err(problem("Terminal input purpose receipt is invalid"));
    }
    Ok(Some(value.form))
}

pub(crate) fn publish_purpose(console: &Path, input: &str, form: bool) -> Result<()> {
    let value = InputPurpose { schema_version: 1, input: input.to_owned(), form };
    save(&purpose_path(console, input)?, &serde_json::to_vec(&value)?)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InputReview {
    schema_version: u32,
    input: String,
    digest: String,
    bytes: u64,
}

fn reviewed(console: &Path, receipt: &InputReceipt) -> Result<bool> {
    let file = match std::fs::File::open(input_directory(console, &receipt.input)?.join(INPUT_REVIEW_FILE)) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let value: InputReview = serde_json::from_reader(std::io::BufReader::new(file))?;
    if value.schema_version != 1 || value.input != receipt.input || value.digest != receipt.digest || value.bytes != receipt.bytes {
        return Err(problem("Terminal input review belongs to another retained receipt"));
    }
    Ok(true)
}

/// Acknowledges inspection without claiming delivery or changing the owner's unknown outcome.
pub(crate) fn review_unknown(console: &Path, input: &str, digest: &str) -> Result<()> {
    let receipt = InputReceipt::read(console, input)?.ok_or_else(|| problem("The original terminal input receipt is unavailable"))?;
    if receipt.input != input || receipt.digest != digest || receipt.state != InputState::Unknown {
        return Err(problem("Only the exact unknown terminal input receipt can be reviewed"));
    }
    let review = InputReview { schema_version: 1, input: input.to_owned(), digest: digest.to_owned(), bytes: receipt.bytes };
    save(&input_directory(console, input)?.join(INPUT_REVIEW_FILE), &serde_json::to_vec(&review)?)
}

#[derive(Serialize)]
pub(crate) struct RecoverableInput {
    input: String,
    digest: String,
    state: InputState,
    body: String,
}

fn recovery_order(receipt: &InputReceipt) -> (u8, &str) {
    let state = match receipt.state {
        InputState::Pending => 0,
        InputState::Unknown => 1,
        InputState::Settled => 2,
    };
    (state, &receipt.input)
}

pub(crate) fn input_directory(console: &Path, input: &str) -> Result<PathBuf> {
    if !valid_input(input) {
        return Err(problem("Terminal input identity is invalid"));
    }
    Ok(console.join(INPUTS_DIRECTORY).join(input))
}

pub(crate) fn body_path(console: &Path, input: &str) -> Result<PathBuf> {
    Ok(input_directory(console, input)?.join(INPUT_BODY_FILE))
}

pub(super) fn receipt_path(console: &Path, input: &str) -> Result<PathBuf> {
    Ok(input_directory(console, input)?.join(INPUT_RECEIPT_FILE))
}

fn purpose_path(console: &Path, input: &str) -> Result<PathBuf> {
    Ok(input_directory(console, input)?.join(INPUT_PURPOSE_FILE))
}

pub(crate) fn valid_input(value: &str) -> bool {
    valid_lower_hex(value, 32)
}

pub(crate) fn valid_digest(value: &str) -> bool {
    valid_lower_hex(value, 64)
}

fn valid_lower_hex(value: &str, bytes: usize) -> bool {
    value.len() == bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
