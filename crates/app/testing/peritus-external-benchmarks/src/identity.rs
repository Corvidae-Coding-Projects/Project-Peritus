//! Immutable identity for the native benchmark executable.

use std::{fs::File, io::Read as _};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::BenchmarkError;

const COMPILED_SOURCE_REVISION: Option<&str> = option_env!("PERITUS_SOURCE_REVISION");

/// Report schema emitted by both native external-benchmark compositions.
pub const INVOCATION_REPORT_SCHEMA_VERSION: u32 = 6;

/// Provider-free executable contract used before an external trial starts.
#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct BenchmarkAgentProtocol {
    schema_version: u32,
    invocation_report_schema_version: u32,
    terminalbench_report_schema_version: u32,
    agent_identity: BenchmarkAgentIdentity,
}

/// Source and binary identity retained with every external benchmark invocation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BenchmarkAgentIdentity {
    /// Cargo package version compiled into the native agent.
    pub package_version: &'static str,
    /// Exact clean Git source revision supplied to the compiler.
    pub source_revision: Option<&'static str>,
    /// SHA-256 of the executable bytes used for this invocation.
    pub binary_sha256: String,
}

impl BenchmarkAgentIdentity {
    /// Inspects the current executable and returns its immutable benchmark identity.
    pub(super) fn current() -> Result<Self, BenchmarkError> {
        let source_revision = optional_source_revision(COMPILED_SOURCE_REVISION)?;
        let executable = std::env::current_exe().map_err(|error| {
            BenchmarkError::filesystem(
                "resolve current benchmark executable",
                "peritus-benchmark-agent",
                error,
            )
        })?;
        let mut file = File::open(&executable).map_err(|error| {
            BenchmarkError::filesystem("open current benchmark executable", &executable, error)
        })?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 8 * 1024];
        loop {
            let read = file.read(&mut buffer).map_err(|error| {
                BenchmarkError::filesystem("hash current benchmark executable", &executable, error)
            })?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok(Self {
            package_version: env!("CARGO_PKG_VERSION"),
            source_revision,
            binary_sha256: lowercase_hex(&hasher.finalize()),
        })
    }
}

impl BenchmarkAgentProtocol {
    /// Inspects the running executable and binds its supported report contract.
    pub(crate) fn current() -> Result<Self, BenchmarkError> {
        Ok(Self {
            schema_version: 1,
            invocation_report_schema_version: INVOCATION_REPORT_SCHEMA_VERSION,
            terminalbench_report_schema_version: INVOCATION_REPORT_SCHEMA_VERSION,
            agent_identity: BenchmarkAgentIdentity::current()?,
        })
    }
}

fn optional_source_revision(
    value: Option<&'static str>,
) -> Result<Option<&'static str>, BenchmarkError> {
    value.map(|revision| validate_source_revision(revision).map(|()| revision)).transpose()
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn validate_source_revision(value: &str) -> Result<(), BenchmarkError> {
    if matches!(value.len(), 40 | 64)
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Ok(());
    }
    Err(BenchmarkError::Identity(
        "compiled source revision must be a full lowercase Git object ID".to_owned(),
    ))
}
