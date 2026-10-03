//! Fail-closed substitutes for effects excluded from verification-only builds.

use std::path::Path;

use peritus_types::Sha256Digest;

use super::{ProductRunResume, ProductRunner};
use crate::{ProductRunnerError, ProductRunnerErrorKind};

/// Verification-only shape of a command effect whose outcome cannot safely be replayed.
pub struct UncertainEffect {
    identity: String,
    tool: String,
    state: UncertainEffectState,
    requirements_revision: Option<u64>,
}

/// Verification-only shape of the durable uncertain-effect states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UncertainEffectState {
    /// The command was admitted and may still be running under the live run owner.
    Started,
    /// Recovery proved only that the command may have taken effect.
    Ambiguous,
    /// The unknown outcome was explicitly reviewed without being inferred.
    Reviewed,
}

impl UncertainEffect {
    /// Stable identity of the original effect admission.
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// Developer tool that owns the uncertain effect.
    pub fn tool(&self) -> &str {
        &self.tool
    }

    /// Current durable state.
    pub const fn state(&self) -> UncertainEffectState {
        self.state
    }

    /// Requirements revision that admitted the effect, when present.
    pub const fn requirements_revision(&self) -> Option<u64> {
        self.requirements_revision
    }
}

/// Verification-only builds cannot inspect the production receipt ledger.
pub fn uncertain_effects(_path: &Path) -> Result<Vec<UncertainEffect>, ProductRunnerError> {
    Err(effect_receipts_unavailable())
}

/// Verification-only builds cannot mutate the production receipt ledger.
pub fn acknowledge_uncertain_effect(
    _path: &Path,
    _identity: &str,
) -> Result<(), ProductRunnerError> {
    Err(effect_receipts_unavailable())
}

/// Verification-only builds cannot inspect workspace path metadata.
pub fn checked_protected_file(
    _root: &Path,
    _relative: &str,
    _contract: &str,
    _protected: &[std::path::PathBuf],
) -> Result<std::path::PathBuf, ProductRunnerError> {
    Err(ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidPrecondition,
        "check protected workspace file",
        "workspace inspection is unavailable in a verus_only build",
    ))
}

impl ProductRunResume {
    /// Verification-only builds do not serialize effectful continuation state.
    pub fn encode_durable(&self) -> Result<Vec<u8>, ProductRunnerError> {
        Err(ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidPrecondition,
            "encode durable product-run continuation",
            "effectful continuation state is unavailable in a verus_only build",
        ))
    }

    /// Verification-only builds do not restore effectful continuation state.
    pub fn decode_durable(_bytes: &[u8], _transcript: &str) -> Result<Self, ProductRunnerError> {
        Err(ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidPrecondition,
            "restore durable product-run continuation",
            "effectful continuation state is unavailable in a verus_only build",
        ))
    }

    /// Verification-only builds do not restore retained effectful continuation state.
    pub fn decode_durable_retained(
        _bytes: &[u8],
        _transcript: &str,
    ) -> Result<Self, ProductRunnerError> {
        Err(ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidPrecondition,
            "restore retained product-run continuation",
            "effectful continuation state is unavailable in a verus_only build",
        ))
    }

    /// Verification-only builds do not rebind retained effectful continuation state.
    pub fn reconcile_candidate(
        self,
        _checkpoint: peritus_run_settlement::CandidateCheckpoint,
    ) -> Result<Self, ProductRunnerError> {
        Err(ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidPrecondition,
            "reconcile retained product candidate",
            "effectful continuation state is unavailable in a verus_only build",
        ))
    }
}

impl ProductRunner {
    /// Verification-only builds cannot perform direct-folder effects.
    pub async fn converse_folder(
        _input: super::ProductRunInput,
        _mode: crate::ConversationMode,
        _writable: bool,
        _protected: &[std::path::PathBuf],
        _observe: super::RunObserver,
    ) -> Result<super::ProductRunOutcome, ProductRunnerError> {
        Err(ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidPrecondition,
            "execute folder conversation",
            "folder effects are unavailable in a verus_only build",
        ))
    }
    /// Verification-only builds cannot perform provider or workspace effects.
    pub async fn converse(
        _input: super::ProductRunInput,
        _mode: crate::ConversationMode,
        _observe: super::RunObserver,
    ) -> Result<super::ProductRunOutcome, ProductRunnerError> {
        Err(ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidPrecondition,
            "execute product conversation",
            "conversation effects are unavailable in a verus_only build",
        ))
    }
    /// Verification-only builds cannot inspect an effectful workspace.
    pub fn candidate_source_digest(
        _workspace_root: &Path,
    ) -> Result<Sha256Digest, ProductRunnerError> {
        Err(ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidPrecondition,
            "compute product-run source digest",
            "workspace inspection is unavailable in a verus_only build",
        ))
    }
    /// Verification-only builds cannot inspect an effectful workspace.
    pub fn candidate_digest(_workspace_root: &Path) -> Result<Sha256Digest, ProductRunnerError> {
        Err(ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidPrecondition,
            "compute product-run candidate digest",
            "workspace inspection is unavailable in a verus_only build",
        ))
    }
}

// The baseline APIs have the same explicit effect boundary as candidate_digest.
// None may claim an empty diff or a successful restore in verification-only builds.
impl ProductRunner {
    /// Baseline reads require the production filesystem boundary.
    pub fn retained_task_baseline(_trace: &Path) -> Result<String, ProductRunnerError> {
        Err(baseline_unavailable())
    }
    /// Baseline decoding requires the production persistence boundary.
    pub fn validate_task_baseline(_bytes: &str) -> Result<(), ProductRunnerError> {
        Err(baseline_unavailable())
    }
    /// Candidate export requires the production repository boundary.
    pub fn candidate_patch_from_baseline(
        _workspace: &Path,
        _bytes: &str,
    ) -> Result<Vec<u8>, ProductRunnerError> {
        Err(baseline_unavailable())
    }
    /// Candidate restore requires the production repository boundary.
    pub fn discard_from_baseline(
        _workspace: &Path,
        _bytes: &str,
        _paths: &[String],
    ) -> Result<Vec<std::path::PathBuf>, ProductRunnerError> {
        Err(baseline_unavailable())
    }

    /// Discard intent retention requires the production repository boundary.
    pub fn prepare_discard_transaction(
        _workspace: &Path,
        _baseline: &str,
        _paths: &[String],
        _state: &Path,
        _binding: Sha256Digest,
        _candidate: Sha256Digest,
    ) -> Result<Sha256Digest, ProductRunnerError> {
        Err(baseline_unavailable())
    }

    /// Read-only discard inspection requires the production persistence boundary.
    pub fn inspect_discard_transaction(
        _state: &Path,
        _binding: Sha256Digest,
        _digest: Sha256Digest,
    ) -> Result<Option<crate::DiscardTransactionState>, ProductRunnerError> {
        Err(baseline_unavailable())
    }

    /// Explicit discard recovery requires the production repository boundary.
    pub fn execute_discard_transaction(
        _state: &Path,
        _binding: Sha256Digest,
        _digest: Sha256Digest,
    ) -> Result<Vec<std::path::PathBuf>, ProductRunnerError> {
        Err(baseline_unavailable())
    }
}
fn baseline_unavailable() -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidPrecondition,
        "inspect task baseline",
        "task baseline effects are unavailable in a verus_only build",
    )
}

fn effect_receipts_unavailable() -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidPrecondition,
        "inspect developer effect receipts",
        "effect receipts are unavailable in a verus_only build",
    )
}
