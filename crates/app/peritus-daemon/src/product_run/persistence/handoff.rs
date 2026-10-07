//! Immutable typed deltas for boundaries that must survive before the canonical run head moves.

use std::{
    ffi::OsStr,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{PersistedRecord, ProductRunServiceError, RunRecord, continuation, record_directory};

const HANDOFF_VERSION: u16 = 1;
const HANDOFF_STAGING_DIRECTORY: &str = "handoff-staging";
const RETRY_DELAY: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::product_run) enum HandoffKind {
    Migration,
    RejectedFindingUpdate,
    Terminal,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Handoff {
    version: u16,
    header: HandoffHeader,
    payload: HandoffPayload,
}

struct RetainedHandoff {
    handoff: Handoff,
    evidence_digest: [u8; 32],
}

struct CoveredRetryLineage {
    authority: RunRecord,
    fingerprint: [u8; 32],
}

/// Exact covered rejection/terminal authority proven outside the records registry.
///
/// Construction is private to immutable handoff replay. The retry boundary carries this value
/// back into its final registry CAS so a same-frontier canonical projection cannot exchange
/// checkpoint, settlement, continuation, remaining-work, or effect authority after validation.
pub(in crate::product_run) struct RetryHandoffCoverage {
    run_id: peritus_types::RunId,
    attempt_sequence: u64,
    handoff_sequence: u64,
    accepted_head: peritus_types::Sha256Digest,
    rejection: super::super::RejectedFindingUpdate,
    history_fingerprint: [u8; 32],
    proof_fingerprint: [u8; 32],
    authority: RunRecord,
}

impl RetryHandoffCoverage {
    pub(in crate::product_run) fn binds(
        &self,
        record: &RunRecord,
    ) -> Result<bool, ProductRunServiceError> {
        Ok(self.proof_fingerprint
            == retry_proof_fingerprint(
                self.history_fingerprint,
                self.run_id,
                self.attempt_sequence,
                self.handoff_sequence,
                self.accepted_head,
            )
            && self.run_id == record.request.run_id()
            && self.attempt_sequence == record.attempt_sequence
            && self.handoff_sequence == record.handoff_sequence
            && self.accepted_head == record.finding_catalog.head_digest
            && record.rejected_finding_update.as_ref() == Some(&self.rejection)
            && terminal_authority_matches(&self.authority, record)?)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct HandoffHeader {
    run_id: [u8; 16],
    workspace_id: [u8; 16],
    control_operation: [u8; 16],
    conversation: [u8; 16],
    actor: [u8; 16],
    attempt_sequence: u64,
    prior_sequence: u64,
    sequence: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", content = "record", rename_all = "snake_case")]
enum HandoffPayload {
    Migration(PersistedRecord),
    RejectedFindingUpdate(PersistedRecord),
    Terminal(PersistedRecord),
}

impl HandoffPayload {
    const fn kind(&self) -> HandoffKind {
        match self {
            Self::Migration(_) => HandoffKind::Migration,
            Self::RejectedFindingUpdate(_) => HandoffKind::RejectedFindingUpdate,
            Self::Terminal(_) => HandoffKind::Terminal,
        }
    }

    fn into_record(self) -> PersistedRecord {
        match self {
            Self::Migration(record)
            | Self::RejectedFindingUpdate(record)
            | Self::Terminal(record) => record,
        }
    }
}

/// Publishes one no-clobber handoff after its continuation pages are durable.
pub(in crate::product_run) fn write_handoff(
    directory: &Path,
    record: &RunRecord,
    kind: HandoffKind,
) -> Result<(), ProductRunServiceError> {
    if record.handoff_sequence == 0 || record.attempt_sequence == 0 {
        return Err(ProductRunServiceError::internal(
            "publish product-run handoff",
            "handoff and attempt sequences must be nonzero",
        ));
    }
    let workbench_root = workbench_root(directory)?;
    let resume_root = record
        .resume
        .as_ref()
        .map(|resume| continuation::publish(&workbench_root, resume))
        .transpose()?;
    let persisted = PersistedRecord::from_record(record, resume_root)?;
    let start = &record.interaction.workbench;
    let header = HandoffHeader {
        run_id: *record.request.run_id().as_bytes(),
        workspace_id: *record.request.workspace_id().as_bytes(),
        control_operation: *start.id().as_bytes(),
        conversation: *start.conversation().as_bytes(),
        actor: *start.actor_bytes(),
        attempt_sequence: record.attempt_sequence,
        prior_sequence: record.handoff_sequence - 1,
        sequence: record.handoff_sequence,
    };
    let payload = match kind {
        HandoffKind::Migration => HandoffPayload::Migration(persisted),
        HandoffKind::RejectedFindingUpdate => HandoffPayload::RejectedFindingUpdate(persisted),
        HandoffKind::Terminal => HandoffPayload::Terminal(persisted),
    };
    let handoff = Handoff { version: HANDOFF_VERSION, header, payload };
    let bytes = serde_json::to_vec_pretty(&handoff).map_err(|error| {
        ProductRunServiceError::persistence("encode product-run handoff", error)
    })?;
    let run_directory = handoff_directory(&workbench_root, record.request.run_id().as_bytes());
    let staging_directory =
        handoff_staging_directory(&workbench_root, record.request.run_id().as_bytes());
    fs::create_dir_all(&run_directory).map_err(|error| {
        ProductRunServiceError::persistence("create product-run handoff directory", error)
    })?;
    require_directory(&run_directory, "inspect product-run handoff directory")?;
    continuation::sync_directory(
        run_directory.parent().unwrap_or(&workbench_root),
        "sync product-run handoff parent",
    )?;
    fs::create_dir_all(&staging_directory).map_err(|error| {
        ProductRunServiceError::persistence("create product-run handoff staging directory", error)
    })?;
    let staging_root = handoff_staging_root(&workbench_root);
    require_directory(&staging_root, "inspect product-run handoff staging root")?;
    require_directory(
        &staging_directory,
        "inspect product-run handoff staging directory",
    )?;
    continuation::sync_directory(
        &staging_root,
        "sync product-run handoff staging root",
    )?;
    continuation::sync_directory(&workbench_root, "sync product-run handoff root")?;
    retire_handoff_stages(&workbench_root, record)?;
    let path = run_directory.join(handoff_name(record.handoff_sequence, kind));
    let staging_path =
        staging_directory.join(staged_handoff_name(record.handoff_sequence, kind));
    let mut staging = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging_path)
        .map_err(|error| {
            ProductRunServiceError::persistence("create product-run handoff staging file", error)
        })?;
    staging.write_all(&bytes).map_err(|error| {
        ProductRunServiceError::persistence("write product-run handoff", error)
    })?;
    staging.sync_all().map_err(|error| {
        ProductRunServiceError::persistence("sync product-run handoff", error)
    })?;
    match fs::hard_link(&staging_path, &path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = fs::read(&path).map_err(|error| {
                ProductRunServiceError::persistence("read existing product-run handoff", error)
            })?;
            if existing != bytes {
                return Err(ProductRunServiceError::internal(
                    "publish product-run handoff",
                    "an immutable handoff sequence already contains different evidence",
                ));
            }
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .and_then(|file| file.sync_all())
                .map_err(|error| {
                    ProductRunServiceError::persistence("sync existing product-run handoff", error)
                })?;
        }
        Err(error) => {
            return Err(ProductRunServiceError::persistence(
                "publish product-run handoff without clobber",
                error,
            ));
        }
    }
    continuation::sync_directory(&run_directory, "sync product-run handoff directory")?;
    retire_handoff_stages(&workbench_root, record)?;
    continuation::sync_directory(&workbench_root, "sync product-run handoff root")
}

/// Retries on the existing joined owner. Shutdown is checked between bounded attempts.
pub(in crate::product_run) fn write_handoff_retrying(
    directory: &Path,
    record: &RunRecord,
    kind: HandoffKind,
    shutdown: &peritus_journal::JournalCancellation,
) -> Result<(), ProductRunServiceError> {
    let mut reported = false;
    loop {
        match write_handoff(directory, record, kind) {
            Ok(()) => return Ok(()),
            Err(error) => {
                if !reported {
                    crate::diagnostic::report(&format!(
                        "peritusd: immutable product-run handoff is retrying under its retained owner: {}",
                        error.describe(),
                    ));
                    reported = true;
                }
                if shutdown.is_cancelled() {
                    return Err(error);
                }
                std::thread::sleep(RETRY_DELAY);
            }
        }
    }
}

/// Applies the complete contiguous suffix after the canonical covered sequence.
///
/// Files are intentionally retained. Any malformed, duplicate, gapped, or incompatible member
/// fails the whole suffix so callers can expose recovery without discarding evidence.
pub(in crate::product_run) fn replay_handoffs(
    root: &Path,
    governed: &str,
    record: &mut RunRecord,
) -> Result<bool, ProductRunServiceError> {
    replay_handoffs_internal(root, governed, record).map(|(changed, _)| changed)
}

/// Proves the complete covered rejection lineage without applying a pending suffix.
pub(in crate::product_run) fn validate_retry_handoff_coverage(
    root: &Path,
    governed: &str,
    record: &RunRecord,
) -> Result<RetryHandoffCoverage, ProductRunServiceError> {
    let mut validated = record.clone();
    let (changed, proof) = replay_handoffs_internal(root, governed, &mut validated)?;
    let proof = proof.ok_or_else(|| {
        ProductRunServiceError::internal(
            "validate retry handoff coverage",
            "the canonical rejected finding has no covered authority proof",
        )
    })?;
    if changed || !proof.binds(record)? {
        return Err(ProductRunServiceError::InvalidState);
    }
    Ok(proof)
}

fn replay_handoffs_internal(
    root: &Path,
    governed: &str,
    record: &mut RunRecord,
) -> Result<(bool, Option<RetryHandoffCoverage>), ProductRunServiceError> {
    retire_handoff_stages(root, record)?;
    let canonical_sequence = record.handoff_sequence;
    let canonical_attempt = record.attempt_sequence;
    let canonical_rejection = record.rejected_finding_update.clone();
    let directory = handoff_directory(root, record.request.run_id().as_bytes());
    let directory_metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && canonical_sequence == 0
                && canonical_rejection.is_none() =>
        {
            return Ok((false, None));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ProductRunServiceError::internal(
                "validate covered product-run handoffs",
                "the canonical handoff frontier has no immutable handoff directory",
            ));
        }
        Err(error) => {
            return Err(ProductRunServiceError::persistence(
                "inspect product-run handoff directory",
                error,
            ));
        }
    };
    if !directory_metadata.file_type().is_dir() {
        return Err(ProductRunServiceError::internal(
            "inspect product-run handoff directory",
            "the per-run handoff path is not a regular directory",
        ));
    }
    let mut handoffs = Vec::new();
    for entry in fs::read_dir(&directory).map_err(|error| {
        ProductRunServiceError::persistence("enumerate product-run handoffs", error)
    })? {
        let entry = entry.map_err(|error| {
            ProductRunServiceError::persistence("enumerate product-run handoff", error)
        })?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            ProductRunServiceError::persistence("inspect product-run handoff", error)
        })?;
        if !metadata.file_type().is_file()
            || path.extension().and_then(|value| value.to_str()) != Some("json")
        {
            return Err(ProductRunServiceError::internal(
                "inspect product-run handoff",
                "the handoff directory contains a non-regular or unknown member",
            ));
        }
        let bytes = fs::read(&path).map_err(|error| {
            ProductRunServiceError::persistence("read product-run handoff", error)
        })?;
        let handoff: Handoff = serde_json::from_slice(&bytes).map_err(|error| {
            ProductRunServiceError::internal(
                "decode product-run handoff",
                format!("{}: {error}", path.display()),
            )
        })?;
        let expected_name = handoff_name(handoff.header.sequence, handoff.payload.kind());
        if handoff.version != HANDOFF_VERSION
            || path.file_name().and_then(|value| value.to_str())
                != Some(expected_name.as_str())
        {
            return Err(ProductRunServiceError::internal(
                "validate product-run handoff",
                "handoff version, filename, or typed payload does not agree",
            ));
        }
        handoffs.push(RetainedHandoff {
            handoff,
            evidence_digest: peritus_codec::sha256(&bytes).into_bytes(),
        });
    }
    handoffs.sort_by_key(|retained| retained.handoff.header.sequence);
    if handoffs.windows(2).any(|pair| {
        pair[0].handoff.header.sequence == pair[1].handoff.header.sequence
    }) {
        return Err(ProductRunServiceError::internal(
            "validate product-run handoff",
            "duplicate immutable handoff sequence",
        ));
    }
    let mut previous_attempt = 0;
    for (index, retained) in handoffs.iter().enumerate() {
        let handoff = &retained.handoff;
        let expected = u64::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(1))
            .ok_or_else(|| {
                ProductRunServiceError::internal(
                    "validate product-run handoff",
                    "the immutable handoff collection exceeds its sequence space",
                )
            })?;
        validate_header(&handoff.header, record)?;
        if handoff.header.sequence != expected
            || handoff.header.prior_sequence != expected - 1
            || handoff.header.attempt_sequence < previous_attempt
        {
            return Err(ProductRunServiceError::internal(
                "validate product-run handoff",
                "the immutable handoff history is gapped, mislinked, or attempt-regressive",
            ));
        }
        previous_attempt = handoff.header.attempt_sequence;
    }
    if u64::try_from(handoffs.len()).unwrap_or(u64::MAX) < canonical_sequence {
        return Err(ProductRunServiceError::internal(
            "validate covered product-run handoffs",
            "the immutable handoff history does not cover the canonical frontier",
        ));
    }
    let mut changed = false;
    let mut replayed = record.clone();
    let mut covered_lineage: Option<CoveredRetryLineage> = None;
    let mut covered_terminal_sequence = None;
    for retained in handoffs {
        let Handoff { header, payload, .. } = retained.handoff;
        validate_header(&header, &replayed)?;
        let kind = payload.kind();
        let projected = payload
            .into_record()
            .into_record_with_storage(
                Some(governed),
                Some(root),
                Some(peritus_types::Sha256Digest::new(retained.evidence_digest)),
            )?;
        if projected.handoff_sequence != header.sequence
            || projected.attempt_sequence != header.attempt_sequence
            || projected.request.run_id() != replayed.request.run_id()
            || projected.request.workspace_id() != replayed.request.workspace_id()
            || projected.interaction.workbench != replayed.interaction.workbench
        {
            return Err(ProductRunServiceError::internal(
                "replay product-run handoff",
                "handoff payload identity differs from its immutable header",
            ));
        }
        validate_payload_shape(kind, &projected)?;
        if header.sequence <= replayed.handoff_sequence {
            if header.attempt_sequence > replayed.attempt_sequence {
                return Err(ProductRunServiceError::internal(
                    "validate covered product-run handoff",
                    "a covered handoff names a future attempt",
                ));
            }
            if let Some(rejected) = canonical_rejection.as_ref()
                && header.attempt_sequence == canonical_attempt
            {
                match kind {
                    HandoffKind::RejectedFindingUpdate => {
                        if covered_terminal_sequence.is_some() {
                            return Err(ProductRunServiceError::internal(
                                "validate covered product-run rejection",
                                "a rejection handoff follows a terminal member of the same attempt",
                            ));
                        }
                        let exact = projected.rejected_finding_update.as_ref() == Some(rejected)
                            && rejected.accepted_head == record.finding_catalog.head_digest
                            && projected.finding_catalog.head_digest
                                == record.finding_catalog.head_digest;
                        if exact {
                            let fingerprint = advance_retry_fingerprint(
                                covered_lineage.as_ref().map(|lineage| lineage.fingerprint),
                                header.sequence,
                                retained.evidence_digest,
                            );
                            if let Some(lineage) = covered_lineage.as_mut() {
                                let mut repeated = projected;
                                repeated.handoff_recovery_pending = false;
                                if !terminal_authority_matches(&lineage.authority, &repeated)? {
                                    return Err(ProductRunServiceError::internal(
                                        "validate covered product-run rejection",
                                        "a repeated rejection changed retained terminal authority",
                                    ));
                                }
                                lineage.authority = repeated;
                                lineage.fingerprint = fingerprint;
                            } else {
                                let mut authority = projected;
                                authority.handoff_recovery_pending = false;
                                covered_lineage = Some(CoveredRetryLineage {
                                    authority,
                                    fingerprint,
                                });
                            }
                        } else if covered_lineage.is_some() {
                            return Err(ProductRunServiceError::internal(
                                "validate covered product-run rejection",
                                "a later rejection handoff diverged from the canonical diagnostic",
                            ));
                        }
                    }
                    HandoffKind::Terminal => {
                        if let Some(lineage) = covered_lineage.as_mut() {
                            if projected.rejected_finding_update.as_ref() != Some(rejected)
                                || projected.finding_catalog.head_digest
                                    != record.finding_catalog.head_digest
                            {
                                return Err(ProductRunServiceError::internal(
                                    "validate covered product-run rejection",
                                    "a terminal successor did not retain the exact rejected finding diagnostic",
                                ));
                            }
                            validate_payload(
                                HandoffKind::Terminal,
                                &lineage.authority,
                                &projected,
                            )?;
                            apply(HandoffKind::Terminal, &mut lineage.authority, projected);
                            lineage.authority.handoff_sequence = header.sequence;
                            lineage.authority.handoff_recovery_pending = false;
                            lineage.fingerprint = advance_retry_fingerprint(
                                Some(lineage.fingerprint),
                                header.sequence,
                                retained.evidence_digest,
                            );
                        }
                        covered_terminal_sequence = Some(header.sequence);
                    }
                    HandoffKind::Migration => {}
                }
            }
            continue;
        }
        let expected = replayed.handoff_sequence.checked_add(1).ok_or_else(|| {
            ProductRunServiceError::internal(
                "replay product-run handoff",
                "handoff sequence overflow",
            )
        })?;
        if header.sequence != expected
            || header.prior_sequence != replayed.handoff_sequence
            || header.attempt_sequence != replayed.attempt_sequence
        {
            return Err(ProductRunServiceError::internal(
                "replay product-run handoff",
                "the pending handoff suffix is gapped or belongs to another attempt",
            ));
        }
        validate_payload(kind, &replayed, &projected)?;
        apply(kind, &mut replayed, projected);
        replayed.handoff_sequence = header.sequence;
        replayed.handoff_recovery_pending = false;
        changed = true;
    }
    if canonical_rejection.is_some() && covered_lineage.is_none() {
        return Err(ProductRunServiceError::internal(
            "validate covered product-run rejection",
            "the canonical rejected finding diagnostic has no exact same-attempt immutable source",
        ));
    }
    let proof = match (canonical_rejection, covered_lineage) {
        (Some(rejection), Some(lineage)) => {
            if !terminal_authority_matches(&lineage.authority, record)? {
                return Err(ProductRunServiceError::internal(
                    "validate covered product-run terminal authority",
                    "the canonical projection diverges from its covered rejection/terminal lineage",
                ));
            }
            let history_fingerprint = lineage.fingerprint;
            let proof_fingerprint = retry_proof_fingerprint(
                history_fingerprint,
                record.request.run_id(),
                canonical_attempt,
                canonical_sequence,
                record.finding_catalog.head_digest,
            );
            Some(RetryHandoffCoverage {
                run_id: record.request.run_id(),
                attempt_sequence: canonical_attempt,
                handoff_sequence: canonical_sequence,
                accepted_head: record.finding_catalog.head_digest,
                rejection,
                history_fingerprint,
                proof_fingerprint,
                authority: lineage.authority,
            })
        }
        (None, None) => None,
        _ => {
            return Err(ProductRunServiceError::internal(
                "validate covered product-run rejection",
                "the canonical rejection and its immutable coverage disagree",
            ));
        }
    };
    if changed {
        *record = replayed;
    }
    Ok((changed, proof))
}

fn advance_retry_fingerprint(
    prior: Option<[u8; 32]>,
    sequence: u64,
    evidence_digest: [u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus-product-run-retry-handoff-lineage-v1\0");
    hasher.update(prior.unwrap_or([0; 32]));
    hasher.update(sequence.to_be_bytes());
    hasher.update(evidence_digest);
    hasher.finalize().into()
}

fn retry_proof_fingerprint(
    history: [u8; 32],
    run: peritus_types::RunId,
    attempt_sequence: u64,
    handoff_sequence: u64,
    accepted_head: peritus_types::Sha256Digest,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus-product-run-retry-coverage-proof-v1\0");
    hasher.update(history);
    hasher.update(run.as_bytes());
    hasher.update(attempt_sequence.to_be_bytes());
    hasher.update(handoff_sequence.to_be_bytes());
    hasher.update(accepted_head.into_bytes());
    hasher.finalize().into()
}

fn terminal_authority_matches(
    retained: &RunRecord,
    canonical: &RunRecord,
) -> Result<bool, ProductRunServiceError> {
    let mut expected = retained.clone();
    if expected.user_cancelled && !canonical.user_cancelled {
        return Ok(false);
    }
    expected.user_cancelled = canonical.user_cancelled;
    if canonical.user_cancelled {
        if canonical.snapshot.phase()
            != peritus_app_protocol::ProductRunPhase::Cancelled
        {
            return Ok(false);
        }
        expected.snapshot = super::super::replace_snapshot(
            &expected.snapshot,
            peritus_app_protocol::ProductRunPhase::Cancelled,
            canonical.snapshot.status(),
            canonical.snapshot.summary(),
        )?;
    }
    // Interaction and preview are explicitly live overlays. They do not authorize replacing any
    // terminal checkpoint, settlement, continuation, remaining work, or deliverable evidence.
    expected.interaction = canonical.interaction.clone();
    expected.preview = canonical.preview.clone();

    if encoded_resume(&expected)? != encoded_resume(canonical)? {
        return Ok(false);
    }
    expected.resume = None;
    let mut canonical_without_resume = canonical.clone();
    canonical_without_resume.resume = None;
    let expected = PersistedRecord::from_record(&expected, None)?;
    let canonical = PersistedRecord::from_record(&canonical_without_resume, None)?;
    let expected = serde_json::to_vec(&expected).map_err(|error| {
        ProductRunServiceError::persistence(
            "encode retained terminal authority proof",
            error,
        )
    })?;
    let canonical = serde_json::to_vec(&canonical).map_err(|error| {
        ProductRunServiceError::persistence(
            "encode canonical terminal authority proof",
            error,
        )
    })?;
    Ok(expected == canonical)
}

fn encoded_resume(record: &RunRecord) -> Result<Option<Vec<u8>>, ProductRunServiceError> {
    record
        .resume
        .as_ref()
        .map(|resume| {
            let mut bytes = Vec::new();
            resume.encode_durable_into(&mut bytes).map_err(|error| {
                ProductRunServiceError::internal(
                    "encode retained terminal continuation proof",
                    error.to_string(),
                )
            })?;
            Ok(bytes)
        })
        .transpose()
}

fn validate_payload_shape(
    kind: HandoffKind,
    projected: &RunRecord,
) -> Result<(), ProductRunServiceError> {
    if !projected.handoff_recovery_pending {
        return Err(ProductRunServiceError::internal(
            "validate product-run handoff payload",
            "an immutable pending handoff was encoded as already covered",
        ));
    }
    match kind {
        HandoffKind::Migration
            if projected.review_artifact_migration_version
                != super::CURRENT_REVIEW_ARTIFACT_MIGRATION_VERSION
                || !projected.finding_catalog.review_artifacts_externalized
                || projected.opaque_resume.is_some()
                || projected.resume.as_ref().is_some_and(|resume| {
                    !resume.review_artifacts_externalized()
                }) =>
        {
            Err(ProductRunServiceError::internal(
                "validate review-artifact migration handoff",
                "migration handoff does not contain complete external review authority",
            ))
        }
        HandoffKind::RejectedFindingUpdate => {
            let Some(rejected) = projected.rejected_finding_update.as_ref() else {
                return Err(ProductRunServiceError::internal(
                    "validate rejected finding update handoff",
                    "rejection handoff omitted its diagnostic",
                ));
            };
            if rejected.accepted_head != projected.finding_catalog.head_digest
                || !matches!(
                    projected.snapshot.phase(),
                    peritus_app_protocol::ProductRunPhase::RecoveryRequired
                        | peritus_app_protocol::ProductRunPhase::Cancelled
                )
            {
                return Err(ProductRunServiceError::internal(
                    "validate rejected finding update handoff",
                    "rejection handoff does not bind its retained valid finding head",
                ));
            }
            Ok(())
        }
        HandoffKind::Terminal => {
            if projected.rejected_finding_update.as_ref().is_some_and(|rejected| {
                rejected.accepted_head != projected.finding_catalog.head_digest
            }) || !matches!(
                projected.snapshot.phase(),
                peritus_app_protocol::ProductRunPhase::RecoveryRequired
                    | peritus_app_protocol::ProductRunPhase::Cancelled
            ) {
                return Err(ProductRunServiceError::internal(
                    "validate terminal product-run handoff",
                    "terminal handoff does not bind its retained recovery lineage",
                ));
            }
            Ok(())
        }
        HandoffKind::Migration => Ok(()),
    }
}

fn validate_payload(
    kind: HandoffKind,
    current: &RunRecord,
    projected: &RunRecord,
) -> Result<(), ProductRunServiceError> {
    match kind {
        HandoffKind::Migration => {
            // Shape validation above establishes the complete external authority.
        }
        HandoffKind::RejectedFindingUpdate => {
            let Some(rejected) = projected.rejected_finding_update.as_ref() else {
                return Err(ProductRunServiceError::internal(
                    "validate rejected finding update handoff",
                    "rejection handoff omitted its diagnostic",
                ));
            };
            if projected.finding_catalog.head_digest != current.finding_catalog.head_digest
                || projected.finding_state != current.finding_state
                || rejected.accepted_head != current.finding_catalog.head_digest
                || !matches!(
                    projected.snapshot.phase(),
                    peritus_app_protocol::ProductRunPhase::RecoveryRequired
                        | peritus_app_protocol::ProductRunPhase::Cancelled
                )
            {
                return Err(ProductRunServiceError::internal(
                    "validate rejected finding update handoff",
                    "rejection handoff does not retain the exact prior valid finding head",
                ));
            }
        }
        HandoffKind::Terminal => {
            if projected.finding_catalog.head_digest != current.finding_catalog.head_digest
                || projected.finding_state != current.finding_state
                || projected.rejected_finding_update != current.rejected_finding_update
                || !continuation_sources_advance(
                    &current.continuation_sources,
                    &projected.continuation_sources,
                )
                || !matches!(
                    projected.snapshot.phase(),
                    peritus_app_protocol::ProductRunPhase::RecoveryRequired
                        | peritus_app_protocol::ProductRunPhase::Cancelled
                )
            {
                return Err(ProductRunServiceError::internal(
                    "validate terminal product-run handoff",
                    "terminal handoff changed valid finding authority or its recovery lineage",
                ));
            }
        }
    }
    Ok(())
}

fn apply(kind: HandoffKind, current: &mut RunRecord, projected: RunRecord) {
    match kind {
        HandoffKind::Migration => {
            current.finding_state = projected.finding_state;
            current.finding_catalog = projected.finding_catalog;
            current.resume = projected.resume;
            current.opaque_resume = projected.opaque_resume;
            current.review_artifact_migration_version =
                projected.review_artifact_migration_version;
        }
        HandoffKind::RejectedFindingUpdate | HandoffKind::Terminal => {
            let terminal = kind == HandoffKind::Terminal;
            let user_cancelled = current.user_cancelled || projected.user_cancelled;
            current.snapshot = projected.snapshot;
            current.progress = projected.progress;
            current.checkpoint = projected.checkpoint;
            current.settlement = projected.settlement;
            current.resume = projected.resume;
            current.opaque_resume = projected.opaque_resume;
            current.remaining_work = projected.remaining_work;
            current.interruption_cause = projected.interruption_cause;
            current.candidate_actionable = projected.candidate_actionable;
            current.task_baseline_required = projected.task_baseline_required;
            current.task_baseline = projected.task_baseline;
            current.rejected_finding_update = projected.rejected_finding_update;
            current.review_artifact_migration_version =
                projected.review_artifact_migration_version;
            if terminal {
                current.continuation_sources = projected.continuation_sources;
            }
            // A user cancellation can race durable recovery and remains canonical.
            current.user_cancelled = user_cancelled;
            if user_cancelled
                && current.snapshot.phase()
                    != peritus_app_protocol::ProductRunPhase::Cancelled
                && let Ok(snapshot) = super::super::replace_snapshot(
                    &current.snapshot,
                    peritus_app_protocol::ProductRunPhase::Cancelled,
                    "Run cancelled",
                    current.snapshot.summary(),
                )
            {
                current.snapshot = snapshot;
            }
        }
    }
}

fn continuation_sources_advance(
    current: &[super::super::ContinuationSource],
    projected: &[super::super::ContinuationSource],
) -> bool {
    current.len() == projected.len()
        && current.iter().zip(projected).all(|(current, projected)| {
            current.operation == projected.operation
                && current.revision == projected.revision
                && current.generation == projected.generation
                && (!current.settled || projected.settled)
        })
}

fn validate_header(
    header: &HandoffHeader,
    record: &RunRecord,
) -> Result<(), ProductRunServiceError> {
    let start = &record.interaction.workbench;
    if header.run_id != *record.request.run_id().as_bytes()
        || header.workspace_id != *record.request.workspace_id().as_bytes()
        || header.control_operation != *start.id().as_bytes()
        || header.conversation != *start.conversation().as_bytes()
        || header.actor != *start.actor_bytes()
        || header.attempt_sequence == 0
        || header.sequence == 0
        || header.prior_sequence.checked_add(1) != Some(header.sequence)
    {
        return Err(ProductRunServiceError::internal(
            "validate product-run handoff",
            "handoff identity, attempt, or sequence is incompatible with its run owner",
        ));
    }
    Ok(())
}

fn workbench_root(directory: &Path) -> Result<PathBuf, ProductRunServiceError> {
    let runs = record_directory(directory)?;
    Ok(runs
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(runs.as_path())
        .to_path_buf())
}

fn handoff_directory(root: &Path, run: &[u8; 16]) -> PathBuf {
    root.join("handoffs").join(hex(run))
}

fn handoff_staging_root(root: &Path) -> PathBuf {
    root.join(HANDOFF_STAGING_DIRECTORY)
}

fn handoff_staging_directory(root: &Path, run: &[u8; 16]) -> PathBuf {
    handoff_staging_root(root).join(hex(run))
}

fn retire_handoff_stages(
    root: &Path,
    record: &RunRecord,
) -> Result<(), ProductRunServiceError> {
    let staging_root = handoff_staging_root(root);
    let staging_root_metadata = match fs::symlink_metadata(&staging_root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(ProductRunServiceError::persistence(
                "inspect product-run handoff staging root",
                error,
            ));
        }
    };
    if !staging_root_metadata.file_type().is_dir() {
        return Err(ProductRunServiceError::internal(
            "inspect product-run handoff staging root",
            "the handoff staging root is not a regular directory",
        ));
    }
    let directory = handoff_staging_directory(root, record.request.run_id().as_bytes());
    let directory_metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(ProductRunServiceError::persistence(
                "inspect product-run handoff staging directory",
                error,
            ));
        }
    };
    if !directory_metadata.file_type().is_dir() {
        return Err(ProductRunServiceError::internal(
            "inspect product-run handoff staging directory",
            "the per-run handoff staging path is not a regular directory",
        ));
    }
    let committed_directory = handoff_directory(root, record.request.run_id().as_bytes());
    let mut stages = Vec::new();
    for entry in fs::read_dir(&directory).map_err(|error| {
        ProductRunServiceError::persistence("enumerate product-run handoff stages", error)
    })? {
        let entry = entry.map_err(|error| {
            ProductRunServiceError::persistence("enumerate product-run handoff stage", error)
        })?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            ProductRunServiceError::persistence("inspect product-run handoff stage", error)
        })?;
        let Some(committed_name) = staged_handoff_target(&entry.file_name()) else {
            return Err(ProductRunServiceError::internal(
                "inspect product-run handoff stage",
                "the handoff staging directory contains an unknown member",
            ));
        };
        if !metadata.file_type().is_file() {
            return Err(ProductRunServiceError::internal(
                "inspect product-run handoff stage",
                "a typed handoff stage is not a regular file",
            ));
        }
        let committed_path = committed_directory.join(committed_name);
        match fs::symlink_metadata(&committed_path) {
            Ok(metadata) if !metadata.file_type().is_file() => {
                return Err(ProductRunServiceError::internal(
                    "inspect staged product-run handoff target",
                    "the committed handoff target is not a regular file",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(ProductRunServiceError::persistence(
                    "inspect staged product-run handoff target",
                    error,
                ));
            }
        }
        stages.push(path);
    }
    if stages.is_empty() {
        return Ok(());
    }
    for path in stages {
        fs::remove_file(&path).map_err(|error| {
            ProductRunServiceError::persistence("retire product-run handoff stage", error)
        })?;
    }
    continuation::sync_directory(
        &directory,
        "sync product-run handoff staging directory",
    )?;
    continuation::sync_directory(&staging_root, "sync product-run handoff staging root")?;
    continuation::sync_directory(root, "sync product-run handoff root")
}

fn require_directory(
    path: &Path,
    operation: &'static str,
) -> Result<(), ProductRunServiceError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| ProductRunServiceError::persistence(operation, error))?;
    if !metadata.file_type().is_dir() {
        return Err(ProductRunServiceError::internal(
            operation,
            "the owned handoff path is not a regular directory",
        ));
    }
    Ok(())
}

fn handoff_name(sequence: u64, kind: HandoffKind) -> String {
    let kind = match kind {
        HandoffKind::Migration => "migration",
        HandoffKind::RejectedFindingUpdate => "rejected-finding-update",
        HandoffKind::Terminal => "terminal",
    };
    format!("{sequence:020}-{kind}.json")
}

fn staged_handoff_name(sequence: u64, kind: HandoffKind) -> String {
    format!("{}.stage", handoff_name(sequence, kind))
}

fn staged_handoff_target(name: &OsStr) -> Option<String> {
    let name = name.to_str()?;
    let sequence_text = name.get(..20)?;
    if sequence_text.len() != 20
        || !sequence_text.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let sequence = sequence_text.parse::<u64>().ok()?;
    if sequence == 0 {
        return None;
    }
    for kind in [
        HandoffKind::Migration,
        HandoffKind::RejectedFindingUpdate,
        HandoffKind::Terminal,
    ] {
        let committed = handoff_name(sequence, kind);
        if name == staged_handoff_name(sequence, kind) {
            return Some(committed);
        }
    }
    None
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
        use core::fmt::Write as _;
        let _ = write!(text, "{byte:02x}");
        text
    })
}
