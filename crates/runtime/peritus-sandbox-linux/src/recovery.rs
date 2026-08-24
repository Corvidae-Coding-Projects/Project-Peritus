//! Checksummed nonsensitive Linux runtime records and exact reopen classification.

use crate::{LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery, NativePhase};
use peritus_types::Sha256Digest;
use std::fs;
use std::path::Path;

const MAGIC: [u8; 8] = *b"PRTLNXR1";
const VERSION: u16 = 1;

/// Exact recovery classification; only `LiveOwned` may be acted on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryClassification {
    /// Exact cgroup leaf and root process birth identity still match.
    LiveOwned,
    /// Record proves cleanup and the leaf is absent.
    AbsentClean,
    /// Native state exists but differs from the recorded ownership.
    Mismatched,
    /// Access or missing evidence prevents a safe conclusion.
    Indeterminate,
}

/// Version-one nonsensitive native recovery record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeRecord {
    preparation_digest: Sha256Digest,
    helper_digest: Sha256Digest,
    cgroup_leaf_id: String,
    root_pid: Option<u32>,
    start_token: Option<u64>,
    phase: NativePhase,
    cleanup_complete: bool,
}

impl RuntimeRecord {
    /// Creates an exact bounded record. `cgroup_leaf_id` is a single relative name, never a path.
    ///
    /// # Errors
    /// Rejects unsafe identifiers or inconsistent PID/birth-token pairs.
    pub fn new(
        preparation_digest: Sha256Digest,
        helper_digest: Sha256Digest,
        cgroup_leaf_id: String,
        root_pid: Option<u32>,
        start_token: Option<u64>,
        phase: NativePhase,
        cleanup_complete: bool,
    ) -> Result<Self, LinuxError> {
        if cgroup_leaf_id.is_empty()
            || cgroup_leaf_id.len() > 128
            || !cgroup_leaf_id.starts_with("peritus-")
            || !cgroup_leaf_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || root_pid == Some(0)
            || (root_pid.is_none() && start_token.is_some())
        {
            return Err(recovery_error(
                LinuxRecovery::CorrectRequest,
                "runtime recovery identity is invalid",
            ));
        }
        Ok(Self {
            preparation_digest,
            helper_digest,
            cgroup_leaf_id,
            root_pid,
            start_token,
            phase,
            cleanup_complete,
        })
    }
    /// Returns preparation identity.
    #[must_use]
    pub const fn preparation_digest(&self) -> Sha256Digest {
        self.preparation_digest
    }
    /// Returns the nonsensitive cgroup leaf identifier.
    #[must_use]
    pub fn cgroup_leaf_id(&self) -> &str {
        &self.cgroup_leaf_id
    }
    /// Returns recorded lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> NativePhase {
        self.phase
    }
    /// Reports recorded complete cleanup.
    #[must_use]
    pub const fn cleanup_complete(&self) -> bool {
        self.cleanup_complete
    }
    /// Encodes a checksummed bounded version-one record.
    ///
    /// # Errors
    /// Returns a protocol bound error if the record is not representable.
    pub fn encode(&self) -> Result<Vec<u8>, LinuxError> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&VERSION.to_be_bytes());
        bytes.extend_from_slice(self.preparation_digest.as_bytes());
        bytes.extend_from_slice(self.helper_digest.as_bytes());
        crate::canonical::push_str(&mut bytes, &self.cgroup_leaf_id)?;
        match self.root_pid {
            Some(pid) => {
                bytes.push(1);
                bytes.extend_from_slice(&pid.to_be_bytes());
            }
            None => bytes.push(0),
        }
        match self.start_token {
            Some(token) => {
                bytes.push(1);
                bytes.extend_from_slice(&token.to_be_bytes());
            }
            None => bytes.push(0),
        }
        bytes.push(phase_tag(self.phase));
        bytes.push(u8::from(self.cleanup_complete));
        let checksum = peritus_codec::sha256(&bytes);
        bytes.extend_from_slice(checksum.as_bytes());
        Ok(bytes)
    }
    /// Decodes and verifies a version-one record.
    ///
    /// # Errors
    /// Rejects checksum, version, bound, lifecycle, or identity corruption.
    pub fn decode(bytes: &[u8]) -> Result<Self, LinuxError> {
        if bytes.len() < 32 || bytes.len() > 1024 {
            return Err(recovery_error(
                LinuxRecovery::Quarantine,
                "runtime recovery record length is invalid",
            ));
        }
        let split = bytes.len() - 32;
        let (body, checksum) = bytes.split_at(split);
        if peritus_codec::sha256(body).as_bytes() != checksum {
            return Err(recovery_error(
                LinuxRecovery::Quarantine,
                "runtime recovery checksum differs",
            ));
        }
        let mut reader = crate::canonical::Reader::new(body);
        if reader.fixed::<8>()? != MAGIC || reader.u16()? != VERSION {
            return Err(recovery_error(
                LinuxRecovery::Quarantine,
                "runtime recovery magic or version is unsupported",
            ));
        }
        let preparation_digest = Sha256Digest::new(reader.fixed()?);
        let helper_digest = Sha256Digest::new(reader.fixed()?);
        let cgroup_leaf_id = reader.string()?;
        let root_pid = match reader.u8()? {
            0 => None,
            1 => Some(reader.u32()?),
            _ => {
                return Err(recovery_error(LinuxRecovery::Quarantine, "PID option tag is invalid"));
            }
        };
        let start_token = match reader.u8()? {
            0 => None,
            1 => Some(reader.u64()?),
            _ => {
                return Err(recovery_error(
                    LinuxRecovery::Quarantine,
                    "birth-token option tag is invalid",
                ));
            }
        };
        let phase = decode_phase(reader.u8()?)?;
        let cleanup_complete = match reader.u8()? {
            0 => false,
            1 => true,
            _ => {
                return Err(recovery_error(
                    LinuxRecovery::Quarantine,
                    "cleanup-complete tag is invalid",
                ));
            }
        };
        reader.finish()?;
        Self::new(
            preparation_digest,
            helper_digest,
            cgroup_leaf_id,
            root_pid,
            start_token,
            phase,
            cleanup_complete,
        )
    }

    /// Classifies current native state without acting on it.
    #[must_use]
    pub fn classify(&self, cgroup_root: &Path) -> RecoveryClassification {
        let leaf = cgroup_root.join(&self.cgroup_leaf_id);
        if leaf.parent() != Some(cgroup_root) {
            return RecoveryClassification::Mismatched;
        }
        if !leaf.exists() {
            return if self.cleanup_complete {
                RecoveryClassification::AbsentClean
            } else {
                RecoveryClassification::Indeterminate
            };
        }
        if self.cleanup_complete {
            return RecoveryClassification::Mismatched;
        }
        let Some(pid) = self.root_pid else {
            return RecoveryClassification::Indeterminate;
        };
        match (self.start_token, process_start_token(pid)) {
            (Some(expected), Some(actual)) if expected != actual => {
                return RecoveryClassification::Mismatched;
            }
            (Some(_), Some(_)) => {}
            _ => return RecoveryClassification::Indeterminate,
        }
        let Ok(procs) = fs::read_to_string(leaf.join("cgroup.procs")) else {
            return RecoveryClassification::Indeterminate;
        };
        let mut ancestry_indeterminate = false;
        for member in procs.split_whitespace().filter_map(|value| value.parse::<u32>().ok()) {
            match process_descends_from(member, pid) {
                Some(true) => return RecoveryClassification::LiveOwned,
                Some(false) => {}
                None => ancestry_indeterminate = true,
            }
        }
        if ancestry_indeterminate {
            RecoveryClassification::Indeterminate
        } else {
            RecoveryClassification::Mismatched
        }
    }

    /// Cleans only state classified as exact live ownership.
    ///
    /// # Errors
    /// Returns indeterminate/mismatch rather than acting on unproved state.
    pub fn cleanup_exact(&self, cgroup_root: &Path) -> Result<crate::CleanupOutcome, LinuxError> {
        if self.classify(cgroup_root) != RecoveryClassification::LiveOwned {
            return Err(recovery_error(
                LinuxRecovery::Quarantine,
                "recovery cleanup requires exact live ownership",
            ));
        }
        let leaf = cgroup_root.join(&self.cgroup_leaf_id);
        crate::CgroupHandle::reopen_exact(cgroup_root.to_path_buf(), leaf).cleanup()
    }
}

fn process_start_token(pid: u32) -> Option<u64> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close = text.rfind(')')?;
    let fields: Vec<&str> = text.get(close + 2..)?.split_ascii_whitespace().collect();
    fields.get(19)?.parse().ok()
}

fn process_descends_from(mut candidate: u32, ancestor: u32) -> Option<bool> {
    for _ in 0..64 {
        if candidate == ancestor {
            return Some(true);
        }
        if candidate <= 1 {
            return Some(false);
        }
        let text = fs::read_to_string(format!("/proc/{candidate}/stat")).ok()?;
        let close = text.rfind(')')?;
        let parent = text.get(close + 2..)?.split_ascii_whitespace().nth(1)?.parse().ok()?;
        if parent == candidate {
            return None;
        }
        candidate = parent;
    }
    None
}

const fn phase_tag(phase: NativePhase) -> u8 {
    match phase {
        NativePhase::Prepared => 0,
        NativePhase::Activated => 1,
        NativePhase::CancelRequested => 2,
        NativePhase::Terminated => 3,
        NativePhase::Released => 4,
    }
}

fn decode_phase(tag: u8) -> Result<NativePhase, LinuxError> {
    match tag {
        0 => Ok(NativePhase::Prepared),
        1 => Ok(NativePhase::Activated),
        2 => Ok(NativePhase::CancelRequested),
        3 => Ok(NativePhase::Terminated),
        4 => Ok(NativePhase::Released),
        _ => Err(recovery_error(LinuxRecovery::Quarantine, "runtime recovery phase is invalid")),
    }
}

fn recovery_error(recovery: LinuxRecovery, detail: &'static str) -> LinuxError {
    LinuxError::new(
        LinuxErrorKind::RecoveryIndeterminate,
        LinuxOperation::Recover,
        recovery,
        detail,
    )
}
