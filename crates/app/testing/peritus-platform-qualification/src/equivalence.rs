//! Repository-versus-package process-equivalence contracts.

use std::collections::BTreeSet;

use crate::{
    ArtifactDigest, Platform, QualificationError, QualificationErrorCode, QualificationRecovery,
    Sha256Digest,
};

/// Process field whose package observation differs from the release control.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EquivalenceField {
    /// Exact executable artifact bytes.
    ExecutableDigest,
    /// Structured argument vector.
    Arguments,
    /// Process terminal classification.
    Termination,
    /// Standard-output bytes.
    StandardOutput,
    /// Standard-error bytes.
    StandardError,
    /// A3 protocol or native helper observation digest.
    ProtocolObservation,
    /// Complete process-tree cleanup.
    TreeCleanup,
}

/// One stable process-equivalence mismatch.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EquivalenceDifference {
    field: EquivalenceField,
}

impl EquivalenceDifference {
    /// Returns the mismatched field.
    #[must_use]
    pub const fn field(self) -> EquivalenceField {
        self.field
    }
}

/// Bounded observation of one control or installed process invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessObservation {
    executable: ArtifactDigest,
    arguments: Vec<String>,
    termination: String,
    stdout: ArtifactDigest,
    stderr: ArtifactDigest,
    protocol: Sha256Digest,
    tree_cleanup_complete: bool,
}

impl ProcessObservation {
    /// Creates a bounded, secret-free process observation.
    ///
    /// # Errors
    ///
    /// Rejects oversized argv or unbounded/control-bearing terminal classification.
    pub fn new(
        executable: ArtifactDigest,
        arguments: Vec<String>,
        termination: impl Into<String>,
        stdout: ArtifactDigest,
        stderr: ArtifactDigest,
        protocol: Sha256Digest,
        tree_cleanup_complete: bool,
    ) -> Result<Self, QualificationError> {
        let termination = termination.into();
        if arguments.len() > 256
            || arguments.iter().any(|argument| argument.len() > 4_096 || argument.contains('\0'))
            || termination.is_empty()
            || termination.len() > 128
            || termination.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(equivalence_error("process observation exceeds a canonical bound"));
        }
        Ok(Self {
            executable,
            arguments,
            termination,
            stdout,
            stderr,
            protocol,
            tree_cleanup_complete,
        })
    }

    /// Returns the executable artifact digest.
    #[must_use]
    pub const fn executable(&self) -> ArtifactDigest {
        self.executable
    }

    /// Borrows the literal argument vector.
    #[must_use]
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    /// Borrows the stable terminal classification.
    #[must_use]
    pub fn termination(&self) -> &str {
        &self.termination
    }
}

/// Frozen rule for comparing a packaged invocation with its release control.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessEquivalenceContract {
    platform: Platform,
    allowed_differences: BTreeSet<EquivalenceField>,
}

impl ProcessEquivalenceContract {
    /// Creates the production equality contract.
    ///
    /// Package path spelling and native signal/exception representation are normalized before an
    /// observation is created; no behavior-bearing field may differ afterward.
    #[must_use]
    pub const fn production(platform: Platform) -> Self {
        Self { platform, allowed_differences: BTreeSet::new() }
    }

    /// Returns the target platform whose normalization rules produced the observations.
    #[must_use]
    pub const fn platform(&self) -> Platform {
        self.platform
    }

    /// Compares exact normalized observations.
    #[must_use]
    pub fn compare(
        &self,
        control: &ProcessObservation,
        packaged: &ProcessObservation,
    ) -> Vec<EquivalenceDifference> {
        let mut fields = Vec::new();
        difference(
            &mut fields,
            EquivalenceField::ExecutableDigest,
            control.executable != packaged.executable,
        );
        difference(
            &mut fields,
            EquivalenceField::Arguments,
            control.arguments != packaged.arguments,
        );
        difference(
            &mut fields,
            EquivalenceField::Termination,
            control.termination != packaged.termination,
        );
        difference(
            &mut fields,
            EquivalenceField::StandardOutput,
            control.stdout != packaged.stdout,
        );
        difference(&mut fields, EquivalenceField::StandardError, control.stderr != packaged.stderr);
        difference(
            &mut fields,
            EquivalenceField::ProtocolObservation,
            control.protocol != packaged.protocol,
        );
        difference(
            &mut fields,
            EquivalenceField::TreeCleanup,
            control.tree_cleanup_complete != packaged.tree_cleanup_complete
                || !packaged.tree_cleanup_complete,
        );
        fields
            .into_iter()
            .filter(|field| !self.allowed_differences.contains(field))
            .map(|field| EquivalenceDifference { field })
            .collect()
    }
}

fn difference(target: &mut Vec<EquivalenceField>, field: EquivalenceField, differs: bool) {
    if differs {
        target.push(field);
    }
}

fn equivalence_error(detail: &'static str) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::InvalidInput,
        QualificationRecovery::CorrectInput,
        "validate process-equivalence observation",
        detail,
    )
}
