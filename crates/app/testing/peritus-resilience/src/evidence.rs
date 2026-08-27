//! Canonical report encoding and SHA-256 binding.

use sha2::{Digest as _, Sha256};

use crate::evidence_failure::{scenario_failure, suite_failure};
use crate::evidence_tags::{
    artifacts, commit_boundary, corrupt_target, crash_timing, daemon_phase, dependency, disk_scope,
    evidence_kind, journal, milestone_kind, projection, reboot_phase, recovery_outcome, terminal,
};
use crate::{
    CaseStatus, CatalogProfile, CleanupObservation, DisruptionObservation, EvidenceDigest,
    FaultInjection, NotReadyReason, PreparationObservation, QualificationConfig,
    QualificationReport, QualificationVerdict, RecoveryObservation, ScenarioReport, ScenarioSpec,
    SubjectDescriptor,
};

pub fn digest_report(report: &QualificationReport) -> EvidenceDigest {
    let mut encoder = Encoder::new();
    encoder.bytes(b"peritus-h1-resilience-evidence-v1");
    config(&mut encoder, report.config());
    encoder.u8(match report.profile() {
        CatalogProfile::H1Production => 1,
        CatalogProfile::Custom => 2,
    });
    option(&mut encoder, report.subject(), subject);
    option(&mut encoder, report.suite_failure(), suite_failure);
    encoder.usize(report.cases().len());
    for case in report.cases() {
        scenario_report(&mut encoder, case);
    }
    encoder.u8(match report.verdict() {
        QualificationVerdict::Ready => 1,
        QualificationVerdict::NotReadyForProduction(NotReadyReason::CustomCatalog) => 2,
        QualificationVerdict::NotReadyForProduction(NotReadyReason::SuiteFailure) => 3,
        QualificationVerdict::NotReadyForProduction(NotReadyReason::ScenarioFailure) => 4,
    });
    EvidenceDigest::from_bytes(encoder.finish())
}

pub struct Encoder(Sha256);

impl Encoder {
    fn new() -> Self {
        Self(Sha256::new())
    }

    fn finish(self) -> [u8; 32] {
        self.0.finalize().into()
    }

    pub fn bytes(&mut self, value: &[u8]) {
        self.u64(value.len() as u64);
        self.0.update(value);
    }

    pub fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    pub fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    pub fn u8(&mut self, value: u8) {
        self.0.update([value]);
    }

    pub fn u16(&mut self, value: u16) {
        self.0.update(value.to_be_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.0.update(value.to_be_bytes());
    }

    pub fn u64(&mut self, value: u64) {
        self.0.update(value.to_be_bytes());
    }

    pub fn usize(&mut self, value: usize) {
        self.u64(value as u64);
    }

    fn digest(&mut self, value: EvidenceDigest) {
        self.0.update(value.as_bytes());
    }
}

fn option<T>(encoder: &mut Encoder, value: Option<&T>, encode: fn(&mut Encoder, &T)) {
    match value {
        Some(value) => {
            encoder.u8(1);
            encode(encoder, value);
        }
        None => encoder.u8(0),
    }
}

fn config(encoder: &mut Encoder, value: QualificationConfig) {
    encoder.u16(value.max_scenarios());
    encoder.u16(value.max_milestones_per_scenario());
    let retries = value.retries();
    encoder.u16(retries.provider());
    encoder.u16(retries.tool());
    encoder.u16(retries.worker());
    encoder.u16(retries.reconciliation());
    let resources = value.resources();
    encoder.u32(resources.events());
    encoder.u32(resources.evidence_bytes());
    encoder.u16(resources.owned_processes());
    encoder.u16(resources.cleanup_steps());
    encoder.u64(resources.logical_ticks());
}

fn subject(encoder: &mut Encoder, value: &SubjectDescriptor) {
    encoder.text(value.id().as_str());
    encoder.text(value.implementation().as_str());
    encoder.digest(value.build_digest());
}

fn scenario_report(encoder: &mut Encoder, value: &ScenarioReport) {
    scenario(encoder, value.scenario());
    encoder.u8(match value.status() {
        CaseStatus::NotExecuted => 1,
        CaseStatus::Failed => 2,
        CaseStatus::Passed => 3,
    });
    option(encoder, value.preparation(), preparation);
    option(encoder, value.disruption(), disruption);
    option(encoder, value.recovery(), recovery);
    match value.cleanup() {
        Some(value) => {
            encoder.u8(1);
            cleanup(encoder, value);
        }
        None => encoder.u8(0),
    }
    encoder.usize(value.failures().len());
    for failure in value.failures() {
        scenario_failure(encoder, failure);
    }
}

fn scenario(encoder: &mut Encoder, value: &ScenarioSpec) {
    encoder.text(value.id().as_str());
    encoder.text(value.title().as_str());
    fault(encoder, value.fault());
    encoder.u8(recovery_outcome(value.expected_recovery()));
}

pub fn fault(encoder: &mut Encoder, value: FaultInjection) {
    match value {
        FaultInjection::CommitCrash { boundary, timing } => {
            encoder.u8(1);
            encoder.u8(commit_boundary(boundary));
            encoder.u8(crash_timing(timing));
        }
        FaultInjection::Corruption(target) => {
            encoder.u8(2);
            encoder.u8(corrupt_target(target));
        }
        FaultInjection::DiskExhaustion(scope) => {
            encoder.u8(3);
            encoder.u8(disk_scope(scope));
        }
        FaultInjection::DependencyDeath(kind) => {
            encoder.u8(4);
            encoder.u8(dependency(kind));
        }
        FaultInjection::RetryExhaustion(kind) => {
            encoder.u8(5);
            encoder.u8(dependency(kind));
        }
        FaultInjection::DaemonKill(phase) => {
            encoder.u8(6);
            encoder.u8(daemon_phase(phase));
        }
        FaultInjection::HostReboot(phase) => {
            encoder.u8(7);
            encoder.u8(reboot_phase(phase));
        }
    }
}

fn preparation(encoder: &mut Encoder, value: &PreparationObservation) {
    encoder.text(value.scenario_id().as_str());
    encoder.u8(terminal(value.terminal()));
    encoder.digest(value.journal_head());
}

fn disruption(encoder: &mut Encoder, value: &DisruptionObservation) {
    encoder.text(value.scenario_id().as_str());
    fault(encoder, value.fault());
    encoder.bool(value.reached());
}

fn recovery(encoder: &mut Encoder, value: &RecoveryObservation) {
    encoder.text(value.scenario_id().as_str());
    encoder.u8(recovery_outcome(value.outcome()));
    let acceptance = value.acceptance();
    encoder.u8(terminal(acceptance.terminal()));
    encoder.bool(acceptance.revision_bound());
    encoder.bool(acceptance.evidence_current());
    encoder.u8(journal(value.journal()));
    encoder.u8(artifacts(value.artifacts()));
    encoder.u8(projection(value.projection()));
    match value.corruption().detected() {
        Some(target) => {
            encoder.u8(1);
            encoder.u8(corrupt_target(target));
        }
        None => encoder.u8(0),
    }
    encoder.bool(value.corruption().mutation_admitted());
    let ownership = value.ownership();
    encoder.bool(ownership.scan_completed());
    encoder.u16(ownership.discovered());
    encoder.u16(ownership.resumed());
    encoder.u16(ownership.failed());
    encoder.u16(ownership.indeterminate());
    encoder.u16(ownership.unaccounted());
    encoder.u16(ownership.orphan_candidates_detected());
    encoder.u16(ownership.orphans_remaining());
    let retries = value.retries();
    encoder.u16(retries.provider());
    encoder.u16(retries.tool());
    encoder.u16(retries.worker());
    encoder.u16(retries.reconciliation());
    let resources = value.resources();
    encoder.u32(resources.events());
    encoder.u32(resources.evidence_bytes());
    encoder.u16(resources.peak_owned_processes());
    encoder.u16(resources.cleanup_steps());
    encoder.u64(resources.logical_ticks());
    encoder.u16(value.temporary_objects());
    encoder.usize(value.evidence().len());
    for anchor in value.evidence() {
        encoder.u8(evidence_kind(anchor.kind()));
        encoder.text(anchor.id().as_str());
        encoder.digest(anchor.digest());
    }
    encoder.usize(value.milestones().len());
    for milestone in value.milestones() {
        encoder.u16(milestone.sequence());
        encoder.u8(milestone_kind(milestone.kind()));
        encoder.text(milestone.detail().as_str());
    }
}

fn cleanup(encoder: &mut Encoder, value: CleanupObservation) {
    encoder.bool(value.resources_released());
    encoder.u16(value.owned_work_remaining());
    encoder.u16(value.cleanup_steps());
}
