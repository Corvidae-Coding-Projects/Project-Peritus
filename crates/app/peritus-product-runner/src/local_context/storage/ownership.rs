//! Exclusive lineage ownership waits until actual release or caller cancellation.

use peritus_agent::DeveloperLoopError;
use peritus_provider_core::CancellationToken;
use peritus_types::Sha256Digest;
use serde::{Deserialize, Serialize};
use std::{fs::{self, File}, io::{Read, Write}, path::Path, time::Duration};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    schema_version: u16,
    scope: [u8; 32],
    nonce: [u8; 16],
    process: u32,
}

pub(super) fn acquire(owner: &File, root: &Path, scope: Sha256Digest, cancellation: &CancellationToken) -> Result<(), DeveloperLoopError> {
    let mut waiting = false;
    loop {
        if cancellation.is_cancelled() { return Err(DeveloperLoopError::Cancelled); }
        match owner.try_lock() {
            Ok(()) => {
                if cancellation.is_cancelled() { return Err(DeveloperLoopError::Cancelled); }
                if let Err(reason) = publish_claim(root, scope) {
                    // The kernel lock remains authoritative. Diagnostics cannot veto ownership.
                    crate::diagnostic::report(&format!("local context owner acquired at {}, but claim diagnostics are unavailable: {reason}", root.display()));
                }
                return Ok(());
            }
            Err(fs::TryLockError::WouldBlock) => {
                if !waiting {
                    crate::diagnostic::report(&format!("waiting for local context ownership at {}; {}", root.display(), observed_claim(root, scope)));
                    waiting = true;
                }
                // Retry cadence, not an elapsed deadline. No ownership is stolen.
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(failure) => return Err(DeveloperLoopError::Context(format!("local working memory: lineage ownership I/O failure at {}: {failure}", root.display()))),
        }
    }
}

fn observed_claim(root: &Path, scope: Sha256Digest) -> String {
    let read = (|| -> Result<Claim, String> {
        let file = File::open(root.join("owner.json")).map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        file.take(4097).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
        if bytes.len() > 4096 { return Err("claim exceeds its fixed schema envelope".to_owned()); }
        let claim: Claim = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if claim.schema_version != 1 || claim.scope != scope.into_bytes() { return Err("claim scope mismatch".to_owned()); }
        Ok(claim)
    })();
    match read {
        Ok(claim) => format!("last recorded ownership claim process={} nonce={:032x}; liveness is determined by the kernel lock", claim.process, u128::from_be_bytes(claim.nonce)),
        Err(_) => "ownership claim diagnostics are unavailable; liveness is determined by the kernel lock".to_owned(),
    }
}

fn publish_claim(root: &Path, scope: Sha256Digest) -> Result<(), String> {
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).map_err(|error| error.to_string())?;
    let claim = Claim { schema_version: 1, scope: scope.into_bytes(), nonce, process: std::process::id() };
    let bytes = serde_json::to_vec(&claim).map_err(|error| error.to_string())?;
    let mut staged = tempfile::NamedTempFile::new_in(root).map_err(|error| error.to_string())?;
    staged.write_all(&bytes).map_err(|error| error.to_string())?;
    staged.as_file().sync_all().map_err(|error| error.to_string())?;
    staged.persist(root.join("owner.json")).map_err(|error| error.to_string())?;
    #[cfg(windows)]
    let directory = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new().read(true).custom_flags(0x0200_0000).open(root)
    };
    #[cfg(not(windows))]
    let directory = File::open(root);
    directory.and_then(|file| file.sync_all()).map_err(|error| error.to_string())
}
