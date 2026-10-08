//! Exact candidate-bound gate state retained across a product-run process boundary.

use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use peritus_gates::{GateExecutionRecord, TargetGateReport};
use peritus_run_settlement::{CandidateCheckpoint, CandidateIdentity};
use peritus_types::Sha256Digest;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{ProductRunnerError, ProductRunnerErrorKind, gates::GateReport};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RetainedGateState {
    candidate: RetainedCandidate,
    execution_context: [u8; 32],
    changed_paths: Vec<RetainedNativePath>,
    uncovered_paths: Vec<RetainedNativePath>,
    records: Vec<RetainedGateRecord>,
    passed: bool,
    output: String,
    state_digest: [u8; 32],
}

#[derive(Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RetainedCandidate {
    run_id: [u8; 16],
    workspace_id: [u8; 16],
    content_digest: [u8; 32],
    repository_digest: [u8; 32],
    execution_digest: Option<[u8; 32]>,
    requirements_revision: u64,
    checkpoint_sequence: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RetainedNativePath {
    platform: u8,
    units: Vec<u8>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RetainedGateRecord {
    command: String,
    label: String,
    exit_code: Option<i32>,
    output: String,
}

impl RetainedGateState {
    pub(super) fn capture(
        report: &GateReport,
        checkpoint: &CandidateCheckpoint,
    ) -> Result<Self, ProductRunnerError> {
        if !report_is_current(report, checkpoint) {
            return Err(invalid(
                "retained gate report does not bind the exact candidate checkpoint",
            ));
        }
        let candidate = RetainedCandidate::from_identity(*checkpoint.identity());
        let changed_paths = report
            .report
            .changed_paths()
            .iter()
            .map(|path| RetainedNativePath::capture(path.as_os_str()))
            .collect();
        let uncovered_paths = report
            .report
            .uncovered_paths()
            .iter()
            .map(|path| RetainedNativePath::capture(path.as_os_str()))
            .collect();
        let records = report
            .report
            .records()
            .iter()
            .map(|record| RetainedGateRecord {
                command: record.command.clone(),
                label: record.label.clone(),
                exit_code: record.exit_code,
                output: record.output.clone(),
            })
            .collect();
        let mut state = Self {
            candidate,
            execution_context: report.execution_context().into_bytes(),
            changed_paths,
            uncovered_paths,
            records,
            passed: report.report.passed(),
            output: report.output.clone(),
            state_digest: [0; 32],
        };
        state.state_digest = state.digest().into_bytes();
        Ok(state)
    }

    pub(super) fn encode(&self) -> Result<Vec<u8>, ProductRunnerError> {
        serde_json::to_vec(self).map_err(invalid)
    }

    pub(super) fn decode(bytes: &[u8]) -> Result<Self, ProductRunnerError> {
        let state: Self = serde_json::from_slice(bytes).map_err(invalid)?;
        if state.digest().as_bytes() != &state.state_digest {
            return Err(invalid("retained gate state digest differs from its contents"));
        }
        Ok(state)
    }

    pub(super) const fn binding_digest(&self) -> Sha256Digest {
        Sha256Digest::new(self.state_digest)
    }

    pub(super) fn restore(
        self,
        checkpoint: &CandidateCheckpoint,
    ) -> Result<GateReport, ProductRunnerError> {
        if self.candidate != RetainedCandidate::from_identity(*checkpoint.identity()) {
            return Err(invalid(
                "retained gate state belongs to another candidate checkpoint",
            ));
        }
        if self.digest().as_bytes() != &self.state_digest {
            return Err(invalid("retained gate state digest differs from its contents"));
        }
        let execution_context = Sha256Digest::new(self.execution_context);
        if checkpoint.identity().execution_digest() != Some(execution_context)
            || !checkpoint.gates().is_current_for(checkpoint.identity())
        {
            return Err(invalid(
                "retained gate state has no current checkpoint evidence binding",
            ));
        }
        let changed_paths = restore_paths(self.changed_paths)?;
        let uncovered_paths = restore_paths(self.uncovered_paths)?;
        let records = self
            .records
            .into_iter()
            .map(|record| GateExecutionRecord {
                command: record.command,
                label: record.label,
                exit_code: record.exit_code,
                output: record.output,
            })
            .collect();
        let report = TargetGateReport::from_retained(
            changed_paths,
            uncovered_paths,
            records,
            self.passed,
        )
        .map_err(invalid)?;
        Ok(GateReport { report, output: self.output, execution_context })
    }

    fn digest(&self) -> Sha256Digest {
        let mut hasher = Sha256::new();
        hasher.update(b"peritus-retained-gate-state-v1\0");
        self.candidate.hash(&mut hasher);
        hasher.update(self.execution_context);
        hash_paths(&mut hasher, &self.changed_paths);
        hash_paths(&mut hasher, &self.uncovered_paths);
        hasher.update(u64::try_from(self.records.len()).unwrap_or(u64::MAX).to_be_bytes());
        for record in &self.records {
            hash_bytes(&mut hasher, record.command.as_bytes());
            hash_bytes(&mut hasher, record.label.as_bytes());
            match record.exit_code {
                Some(code) => {
                    hasher.update([1]);
                    hasher.update(code.to_be_bytes());
                }
                None => hasher.update([0]),
            }
            hash_bytes(&mut hasher, record.output.as_bytes());
        }
        hasher.update([u8::from(self.passed)]);
        hash_bytes(&mut hasher, self.output.as_bytes());
        Sha256Digest::new(hasher.finalize().into())
    }
}

impl RetainedCandidate {
    fn from_identity(identity: CandidateIdentity) -> Self {
        Self {
            run_id: *identity.run_id().as_bytes(),
            workspace_id: *identity.workspace_id().as_bytes(),
            content_digest: identity.content_digest().into_bytes(),
            repository_digest: identity.repository_digest().into_bytes(),
            execution_digest: identity.execution_digest().map(Sha256Digest::into_bytes),
            requirements_revision: identity.requirements_revision(),
            checkpoint_sequence: identity.checkpoint_sequence(),
        }
    }

    fn hash(&self, hasher: &mut Sha256) {
        hasher.update(self.run_id);
        hasher.update(self.workspace_id);
        hasher.update(self.content_digest);
        hasher.update(self.repository_digest);
        match self.execution_digest {
            Some(digest) => {
                hasher.update([1]);
                hasher.update(digest);
            }
            None => hasher.update([0]),
        }
        hasher.update(self.requirements_revision.to_be_bytes());
        hasher.update(self.checkpoint_sequence.to_be_bytes());
    }
}

impl RetainedNativePath {
    fn capture(path: &OsStr) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt as _;

            Self { platform: 1, units: path.as_bytes().to_vec() }
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt as _;

            let mut units = Vec::new();
            for unit in path.encode_wide() {
                units.extend_from_slice(&unit.to_be_bytes());
            }
            Self { platform: 2, units }
        }
    }

    fn restore(self) -> Result<PathBuf, ProductRunnerError> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt as _;

            if self.platform != 1 {
                return Err(invalid("retained gate path belongs to another platform"));
            }
            Ok(PathBuf::from(OsString::from_vec(self.units)))
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt as _;

            if self.platform != 2 || self.units.len() % 2 != 0 {
                return Err(invalid("retained gate path has an invalid Windows encoding"));
            }
            let units = self
                .units
                .chunks_exact(2)
                .map(|unit| u16::from_be_bytes([unit[0], unit[1]]))
                .collect::<Vec<_>>();
            Ok(PathBuf::from(OsString::from_wide(&units)))
        }
    }
}

pub(super) fn report_is_current(
    report: &GateReport,
    checkpoint: &CandidateCheckpoint,
) -> bool {
    checkpoint.identity().execution_digest() == Some(report.execution_context())
        && checkpoint.gates().is_current_for(checkpoint.identity())
}

fn restore_paths(
    paths: Vec<RetainedNativePath>,
) -> Result<Vec<PathBuf>, ProductRunnerError> {
    paths.into_iter().map(RetainedNativePath::restore).collect()
}

fn hash_paths(hasher: &mut Sha256, paths: &[RetainedNativePath]) {
    hasher.update(u64::try_from(paths.len()).unwrap_or(u64::MAX).to_be_bytes());
    for path in paths {
        hasher.update([path.platform]);
        hash_bytes(hasher, &path.units);
    }
}

fn hash_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(bytes);
}

fn invalid(error: impl std::fmt::Display) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidPrecondition,
        "restore candidate-bound gate state",
        error.to_string(),
    )
}
