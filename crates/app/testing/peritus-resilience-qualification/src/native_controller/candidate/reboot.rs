//! Actual disposable-host reboot routes driven through an owned QEMU/KVM guest.

mod guest;
mod media;
mod parse;

use std::fs;
use std::time::Instant;

use super::config::bytes_sha256;
use super::{InjectedCandidate, RecoveredCandidate, RuntimePaths};
use crate::native_controller::args::ControllerPaths;
use crate::native_controller::request::RebootPhase;

const JOURNAL: &str = "/var/lib/peritus-h1/state/peritus.sqlite3";

pub(super) struct RebootRuntime {
    guest: guest::Guest,
    image_sha256: String,
    image_bytes: u64,
    candidate_sha256: String,
    history: Option<RebootHistory>,
}

struct RebootHistory {
    phase: RebootPhase,
    initial_boot_id: String,
    first_boot_id: String,
    recovery_boot_id: String,
    initial_stage_sha256: String,
    reconciliation_stage_sha256: Option<String>,
}

pub(super) fn prepare(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
    _phase: RebootPhase,
) -> Result<RebootRuntime, Box<dyn std::error::Error>> {
    let image = paths
        .controller_resource
        .as_ref()
        .ok_or("host reboot qualification requires a guest image")?;
    let image_sha256 = paths
        .controller_resource_sha256
        .clone()
        .ok_or("host reboot qualification guest image has no digest binding")?;
    let image_bytes = fs::metadata(image)?.len();
    if image_bytes == 0 {
        return Err("host reboot qualification guest image is empty".into());
    }
    let guest = guest::Guest::launch(paths, &runtime.root)?;
    if !guest.version().starts_with("peritusd ") {
        return Err("disposable guest did not start the release candidate".into());
    }
    Ok(RebootRuntime {
        guest,
        image_sha256,
        image_bytes,
        candidate_sha256: paths.build_sha256.clone(),
        history: None,
    })
}

pub(super) fn inject(
    _paths: &ControllerPaths,
    runtime: &mut RebootRuntime,
    phase: RebootPhase,
    request_sha256: &str,
) -> Result<InjectedCandidate, Box<dyn std::error::Error>> {
    if runtime.history.is_some() {
        return Err("host reboot route was injected more than once".into());
    }
    let initial_boot_id = runtime.guest.boot_id().to_owned();
    let initial_stage = runtime.guest.start_checkpoint(initial_command(phase))?;
    let initial = parse::stage(&initial_stage, phase, false)?;
    let expected_initial_effects = u64::from(phase != RebootPhase::OutstandingEffect);
    if initial.external_effects != expected_initial_effects {
        return Err("guest reboot checkpoint has the wrong pre-reboot effect count".into());
    }
    let mut effect = if initial.external_effects == 1 {
        Some(runtime.guest.file(&initial.effect_path)?)
    } else {
        None
    };
    let (_, first_boot_id) = runtime.guest.reboot()?;
    let mut recovery_boot_id = first_boot_id.clone();
    let mut reconciliation_stage_sha256 = None;
    let (effect_path, claim_fence) = if phase == RebootPhase::StartupReconciliation {
        let line = runtime.guest.start_checkpoint("qualify-reboot-startup-reconciliation-stage")?;
        let reconciliation = parse::stage(&line, phase, true)?;
        if reconciliation.effect_path != initial.effect_path
            || reconciliation.external_effects != 1
            || reconciliation.claim_fence <= initial.claim_fence
        {
            return Err("startup reconciliation changed the exact guest delivery identity".into());
        }
        effect = Some(runtime.guest.file(&reconciliation.effect_path)?);
        reconciliation_stage_sha256 = Some(bytes_sha256(line.as_bytes()));
        let (_, second_boot_id) = runtime.guest.reboot()?;
        recovery_boot_id = second_boot_id;
        (reconciliation.effect_path, reconciliation.claim_fence)
    } else {
        (initial.effect_path, initial.claim_fence)
    };
    let initial_stage_sha256 = bytes_sha256(initial_stage.as_bytes());
    let checkpoint = format!(
        "peritus-h1-host-reboot phase={} image_sha256={} candidate_sha256={} request_sha256={} initial_boot_id={} first_boot_id={} recovery_boot_id={} initial_stage_sha256={} reconciliation_stage_sha256={}",
        phase.code(),
        runtime.image_sha256,
        runtime.candidate_sha256,
        request_sha256,
        initial_boot_id,
        first_boot_id,
        recovery_boot_id,
        initial_stage_sha256,
        reconciliation_stage_sha256.as_deref().unwrap_or("none"),
    );
    runtime.history = Some(RebootHistory {
        phase,
        initial_boot_id,
        first_boot_id,
        recovery_boot_id,
        initial_stage_sha256,
        reconciliation_stage_sha256,
    });
    Ok(InjectedCandidate {
        checkpoint,
        claim_fence: Some(claim_fence),
        request_sha256: Some(request_sha256.to_owned()),
        effect_path: Some(effect_path),
        effect_sha256: effect.as_ref().map(|value| value.sha256.clone()),
        effect_bytes: effect.as_ref().map(|value| value.bytes),
        artifact_sha256: Some(runtime.image_sha256.clone()),
        artifact_bytes: Some(runtime.image_bytes),
        snapshot: None,
        lease: None,
        patch: None,
        gate: None,
        promotion: None,
        projection: None,
        fault_process_exit: "disposable-guest-kernel-rebooted".to_owned(),
    })
}

pub(super) fn recover(
    _paths: &ControllerPaths,
    runtime: &RebootRuntime,
    injected: &InjectedCandidate,
    phase: RebootPhase,
) -> Result<RecoveredCandidate, Box<dyn std::error::Error>> {
    let started = Instant::now();
    let history = runtime.history.as_ref().ok_or("host reboot route has no injection history")?;
    if history.phase != phase || runtime.guest.boot_id() != history.recovery_boot_id {
        return Err("host reboot recovery guest identity differs from its injected boot".into());
    }
    let line = runtime.guest.run_candidate(recovery_command(phase))?;
    let recovered = parse::recovery(&line, phase)?;
    let effect_path =
        injected.effect_path.as_deref().ok_or("host reboot injection omitted its effect path")?;
    let effect = runtime.guest.file(effect_path)?;
    if let Some(injected_sha256) = &injected.effect_sha256
        && (injected_sha256 != &effect.sha256 || injected.effect_bytes != Some(effect.bytes))
    {
        return Err("host reboot recovery changed the durable effect bytes".into());
    }
    let journal = runtime.guest.file(JOURNAL)?;
    let observation = format!(
        "{line} initial_boot_id={} first_boot_id={} recovery_boot_id={} initial_stage_sha256={} reconciliation_stage_sha256={}",
        history.initial_boot_id,
        history.first_boot_id,
        history.recovery_boot_id,
        history.initial_stage_sha256,
        history.reconciliation_stage_sha256.as_deref().unwrap_or("none"),
    );
    Ok(RecoveredCandidate {
        observation,
        destination_reconciled: recovered.destination_reconciled,
        external_effects: recovered.external_effects,
        duplicate_effects: recovered.duplicate_effects,
        exact_fence_acknowledged: recovered.exact_fence_acknowledged,
        pending_claims: recovered.pending_claims,
        committed_events: None,
        aggregate_heads: None,
        journal_sha256: journal.sha256,
        journal_bytes: journal.bytes,
        effect_sha256: Some(effect.sha256),
        effect_bytes: Some(effect.bytes),
        artifact_sha256: Some(runtime.image_sha256.clone()),
        artifact_bytes: Some(runtime.image_bytes),
        snapshot: None,
        lease: None,
        patch: None,
        gate: None,
        promotion: None,
        projection: None,
        dependency: None,
        lifecycle: None,
        elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

pub(super) fn cleanup(runtime: &mut RebootRuntime) -> Result<(), Box<dyn std::error::Error>> {
    runtime.guest.shutdown()
}

const fn initial_command(phase: RebootPhase) -> &'static str {
    match phase {
        RebootPhase::OutstandingEffect => "qualify-reboot-outstanding-stage",
        RebootPhase::DurableBeforeAck => "qualify-reboot-durable-stage",
        RebootPhase::StartupReconciliation => "qualify-reboot-startup-stage",
    }
}

const fn recovery_command(phase: RebootPhase) -> &'static str {
    match phase {
        RebootPhase::OutstandingEffect => "qualify-reboot-outstanding-recover",
        RebootPhase::DurableBeforeAck => "qualify-reboot-durable-recover",
        RebootPhase::StartupReconciliation => "qualify-reboot-startup-recover",
    }
}
