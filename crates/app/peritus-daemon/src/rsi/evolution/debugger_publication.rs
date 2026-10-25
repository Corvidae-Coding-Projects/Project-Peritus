//! Native deterministic E2 staging and publication from retained actual E3/C7 sources.

use std::{
    fmt::Write as _,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_debugger::{
    AnalysisCounts, DebuggerCommand, DebuggerCommandKind, DebuggerJobId, DebuggerPhase,
    DebuggerState, PublicationDirectiveClaim, PublicationExecution, SelectionRecord, TransitionIds,
    commit_debugger_transition, decide, finalize_report_artifact, load_debugger_replay,
    publish_claimed_report, stage_and_commit_report,
};
use peritus_eval::EvaluationCampaignId;
use peritus_evidence::EvidenceStore;
use peritus_evolution::{
    EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery,
};
use peritus_journal::SqliteJournal;
use peritus_types::{CommandId, EventId, Sha256Digest};
use serde::{Deserialize, Serialize};

use super::{
    EvolutionTransitionIds, NativeDiagnosticReport, identity, incomplete, local_diagnostic_report,
    owner_error,
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeSource {
    schema: u16,
    job: [u8; 16],
    campaign: [u8; 16],
    query: [u8; 32],
    selection: [u8; 32],
    report: [u8; 32],
    artifact: [u8; 32],
}

/// Stages a real observation-only E2 job; publication remains behind its exact durable claim.
///
/// The protected source capsule is only an immutable reconstruction index. All evidence and
/// report fields are recovered and fully validated from the actual published E3/C7 owners.
///
/// # Errors
/// Rejects missing native evidence, source drift, identity reuse, or failed durable staging.
#[allow(clippy::too_many_arguments, reason = "native source and durable identities stay explicit")]
pub fn stage_local_debugger_report(
    journal: &mut SqliteJournal,
    artifacts: &ArtifactStore,
    protected_root: &Path,
    campaign: EvaluationCampaignId,
    job: DebuggerJobId,
    ids: EvolutionTransitionIds,
) -> Result<NativeDiagnosticReport, EvolutionError> {
    let native = local_diagnostic_report(journal, artifacts, campaign)?;
    let source = NativeSource {
        schema: 1,
        job: *job.as_bytes(),
        campaign: *campaign.as_bytes(),
        query: *native.query.digest().as_bytes(),
        selection: *native.selection.digest().as_bytes(),
        report: *native.report.digest().as_bytes(),
        artifact: *peritus_codec::sha256(native.report.canonical_bytes()).as_bytes(),
    };
    write_source(protected_root, job, &source)?;
    let limits_digest = limits_digest(native.query.limits());
    let revision =
        *crate::rsi::evaluation::load(journal, campaign).map_err(owner_error)?.revision();
    let mut state =
        load_debugger_replay(journal, job).map_err(owner_error)?.rebuild().map_err(owner_error)?;
    if let Some(existing) = &state {
        if existing.query_digest() != native.query.digest()
            || existing.limits_digest() != limits_digest
            || existing.revision() != &revision
            || existing.model_plan_digest().is_some()
        {
            return Err(incomplete("native E2 job identity binds different actual inputs"));
        }
    } else {
        state = Some(transition(
            journal,
            None,
            job,
            native.query.digest(),
            DebuggerCommandKind::CreateJob {
                revision,
                query_digest: native.query.digest(),
                limits_digest,
                model_plan_digest: None,
            },
            ids.child(b"native-debugger-create", native.report.digest())?,
        )?);
    }
    let mut state =
        state.ok_or_else(|| incomplete("native E2 creation produced no owner state"))?;
    let counts = native.selection.counts();
    let selection = SelectionRecord::new(
        native.selection.id(),
        native.selection.digest(),
        counts.subjects(),
        counts.events(),
    )
    .map_err(owner_error)?;
    if state.phase() == DebuggerPhase::Created {
        state = transition(
            journal,
            Some(&state),
            job,
            native.query.digest(),
            DebuggerCommandKind::RecordSelection { selection },
            ids.child(b"native-debugger-selection", native.report.digest())?,
        )?;
    }
    if state.selection() != Some(selection) {
        return Err(incomplete("native E2 selection differs from its actual source"));
    }
    let analysis_counts = AnalysisCounts::new(
        native.report.report().claims().len() as u64,
        native.report.report().causes().len() as u64,
        native.report.report().patterns().len() as u64,
    );
    if state.phase() == DebuggerPhase::Selected {
        state = transition(
            journal,
            Some(&state),
            job,
            native.query.digest(),
            DebuggerCommandKind::RecordDeterministicAnalysis {
                analysis_digest: native.report.digest(),
                counts: analysis_counts,
            },
            ids.child(b"native-debugger-analysis", native.report.digest())?,
        )?;
    }
    if state.deterministic_digest() != Some(native.report.digest())
        || state.analysis_counts() != Some(analysis_counts)
    {
        return Err(incomplete("native E2 analysis differs from its actual source"));
    }
    if state.phase() == DebuggerPhase::DeterministicComplete {
        let child = ids.child(b"native-debugger-report", native.report.digest())?;
        stage_and_commit_report(
            journal,
            artifacts,
            &state,
            &native.report,
            TransitionIds::new(child.command, child.event),
        )
        .map_err(owner_error)?;
    } else if !matches!(state.phase(), DebuggerPhase::ReportReady | DebuggerPhase::Published)
        || state.report()
            != Some(peritus_debugger::report_record(&native.report).map_err(owner_error)?)
    {
        return Err(incomplete("native E2 job cannot stage the exact reconstructed report"));
    }
    Ok(native)
}

/// Publishes only the exact real native report claimed by C0.
///
/// Reconstruction reruns actual source selection and complete report validation, then requires
/// byte-for-byte equality to the already finalized report artifact before admitting evidence.
///
/// # Errors
/// Rejects unsupported model jobs, corrupt/missing capsules, source drift, or stale claim fences.
pub fn publish_debugger_claim(
    journal: &mut SqliteJournal,
    artifacts: &ArtifactStore,
    evidence: &mut EvidenceStore,
    claim: PublicationDirectiveClaim,
    protected_root: &Path,
    maximum_bytes: u64,
) -> Result<PublicationExecution, EvolutionError> {
    let directive = claim.directive();
    let source = read_source(protected_root, directive.job_id())?;
    let campaign = EvaluationCampaignId::new(source.campaign).map_err(owner_error)?;
    let native = local_diagnostic_report(journal, artifacts, campaign)?;
    let state = load_debugger_replay(journal, directive.job_id())
        .map_err(owner_error)?
        .rebuild()
        .map_err(owner_error)?
        .ok_or_else(|| incomplete("claimed E2 job is absent"))?;
    let counts = native.selection.counts();
    let selection = SelectionRecord::new(
        native.selection.id(),
        native.selection.digest(),
        counts.subjects(),
        counts.events(),
    )
    .map_err(owner_error)?;
    let analysis_counts = AnalysisCounts::new(
        native.report.report().claims().len() as u64,
        native.report.report().causes().len() as u64,
        native.report.report().patterns().len() as u64,
    );
    let source_state = crate::rsi::evaluation::load(journal, campaign).map_err(owner_error)?;
    if state.phase() != DebuggerPhase::ReportReady
        || state.model_plan_digest().is_some()
        || state.revision() != source_state.revision()
        || state.selection() != Some(selection)
        || state.limits_digest() != limits_digest(native.query.limits())
        || state.deterministic_digest() != Some(native.report.digest())
        || state.analysis_counts() != Some(analysis_counts)
        || source.schema != 1
        || source.job != *directive.job_id().as_bytes()
        || source.query != *native.query.digest().as_bytes()
        || source.selection != *native.selection.digest().as_bytes()
        || source.report != *native.report.digest().as_bytes()
        || source.artifact != *directive.report().digest().as_bytes()
        || state.query_digest() != native.query.digest()
        || state.report() != Some(directive.report())
        || directive.report()
            != peritus_debugger::report_record(&native.report).map_err(owner_error)?
    {
        return Err(incomplete("claimed native E2 report differs from fully recovered source"));
    }
    let bytes = artifacts
        .read(ArtifactDigest::from_sha256(directive.report().digest()), maximum_bytes)
        .map_err(owner_error)?;
    if bytes != native.report.canonical_bytes() {
        return Err(incomplete(
            "native E2 finalized report differs from actual source reconstruction",
        ));
    }
    let aggregate =
        peritus_debugger::debugger_aggregate_key(directive.job_id()).map_err(owner_error)?;
    let records = journal.records_for_aggregate(aggregate).map_err(owner_error)?;
    let position = records
        .iter()
        .find(|record| record.event_id() == state.last_event_id())
        .ok_or_else(|| incomplete("native E2 report event is absent from owning journal"))?
        .global_position();
    let mut key = b"PERITUS-NATIVE-E2-PUBLICATION\0".to_vec();
    key.extend_from_slice(directive.job_id().as_bytes());
    key.extend_from_slice(directive.report().id().as_bytes());
    let command = CommandId::new(identity(&key)).map_err(owner_error)?;
    key.extend_from_slice(b"event");
    let event = EventId::new(identity(&key)).map_err(owner_error)?;
    let finalized =
        finalize_report_artifact(artifacts, &native.report, event).map_err(owner_error)?;
    publish_claimed_report(
        journal,
        evidence,
        artifacts,
        &state,
        &native.report,
        finalized,
        position,
        claim,
        TransitionIds::new(command, event),
    )
    .map_err(owner_error)
}

fn transition(
    journal: &mut SqliteJournal,
    prior: Option<&DebuggerState>,
    job: DebuggerJobId,
    query: Sha256Digest,
    kind: DebuggerCommandKind,
    ids: EvolutionTransitionIds,
) -> Result<DebuggerState, EvolutionError> {
    let command = DebuggerCommand::new(
        ids.command,
        ids.event,
        job,
        prior.map_or(0, DebuggerState::sequence),
        prior.map(DebuggerState::last_event_id),
        prior.map_or(Sha256Digest::new([0; 32]), DebuggerState::state_digest),
        query,
        kind,
    )
    .map_err(owner_error)?;
    let transition = decide(prior, &command).map_err(owner_error)?;
    commit_debugger_transition(journal, &command, &transition).map_err(owner_error)?;
    Ok(transition.state().clone())
}

fn limits_digest(limits: peritus_debugger::DebuggerLimits) -> Sha256Digest {
    let mut bytes = b"peritus.native-debugger-limits.v1\0".to_vec();
    for value in limits.values() {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    peritus_codec::sha256(&bytes)
}
fn source_path(root: &Path, job: DebuggerJobId) -> Result<PathBuf, EvolutionError> {
    let protected_directory =
        root.is_absolute() && fs::symlink_metadata(root).is_ok_and(|metadata| metadata.is_dir());
    if !protected_directory {
        return Err(EvolutionError::new(
            EvolutionErrorKind::InvalidInput,
            EvolutionOperation::Publish,
            EvolutionRecovery::CorrectInput,
            "native E2 source root must be an existing absolute protected directory initialized by the host",
        ));
    }
    let directory = root.join("rsi-native-debugger-sources");
    match fs::create_dir(&directory) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(owner_error(error)),
    }
    if !fs::symlink_metadata(&directory).map_err(owner_error)?.is_dir() {
        return Err(incomplete("native E2 source directory is not a regular directory"));
    }
    let digest = peritus_codec::sha256(job.as_bytes());
    let mut name = String::with_capacity(digest.as_bytes().len() * 2);
    for byte in digest.as_bytes() {
        write!(name, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(directory.join(format!("{name}.json")))
}
fn read_source(root: &Path, job: DebuggerJobId) -> Result<NativeSource, EvolutionError> {
    let path = source_path(root, job)?;
    let bytes = read_capsule(&path)?;
    serde_json::from_slice(&bytes).map_err(owner_error)
}
fn read_capsule(path: &Path) -> Result<Vec<u8>, EvolutionError> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path).map_err(owner_error)?;
    let metadata = file.metadata().map_err(owner_error)?;
    if !metadata.is_file() || metadata.len() > 16 * 1024 {
        return Err(incomplete("native E2 source capsule is oversized or nonregular"));
    }
    let mut bytes = Vec::new();
    file.take(16 * 1024 + 1).read_to_end(&mut bytes).map_err(owner_error)?;
    if bytes.len() as u64 != metadata.len() {
        return Err(incomplete("native E2 capsule changed during read"));
    }
    Ok(bytes)
}
fn write_source(
    root: &Path,
    job: DebuggerJobId,
    source: &NativeSource,
) -> Result<(), EvolutionError> {
    let path = source_path(root, job)?;
    let bytes = serde_json::to_vec(source).map_err(owner_error)?;
    if path.try_exists().map_err(owner_error)? {
        if read_capsule(&path)? != bytes {
            return Err(incomplete("native E2 source capsule already binds different inputs"));
        }
        return Ok(());
    }
    // Only the synced complete file becomes the stable source name. An interrupted staging
    // write cannot leave a permanently corrupt reconstruction index at that name.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(owner_error)?
        .as_nanos();
    let temporary = path.with_extension(format!("{}-{nonce}.partial", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(&temporary).map_err(owner_error)?;
    file.write_all(&bytes).map_err(owner_error)?;
    file.sync_all().map_err(owner_error)?;
    drop(file);
    match fs::hard_link(&temporary, &path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if read_capsule(&path)? != bytes {
                return Err(incomplete("native E2 source capsule already binds different inputs"));
            }
        }
        Err(error) => return Err(owner_error(error)),
    }
    fs::remove_file(&temporary).map_err(owner_error)?;
    #[cfg(unix)]
    fs::File::open(
        path.parent().ok_or_else(|| incomplete("native E2 capsule has no protected parent"))?,
    )
    .and_then(|directory| directory.sync_all())
    .map_err(owner_error)?;
    Ok(())
}
