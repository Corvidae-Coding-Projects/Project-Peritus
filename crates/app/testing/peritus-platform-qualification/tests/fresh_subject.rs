//! Fresh-subject runner and ready/not-ready reduction tests.
//! Fixtures retain the concrete home path for the borrowed production-layout contract.

use peritus_platform_qualification::{
    Architecture, ArtifactRole, CleanupObservation, EvidenceEntry, EvidenceKind, EvidenceSet,
    FreshSubjectFactory, FreshSubjectRunner, InstallPath, ManifestArtifact, ObservationOutcome,
    PackageManifest, PackageVersion, Platform, PlatformVersion, QualificationError,
    QualificationReport, QualificationSubject, QualificationTarget, ReadinessVerdict,
    RelativePackagePath, ReleaseLayout, ScenarioId, ScenarioObservation, ScenarioRequest,
    Sha256Digest, digest_bytes,
};

struct Factory {
    next: u16,
    unsupported: Option<ScenarioId>,
}

impl FreshSubjectFactory for Factory {
    fn create(
        &mut self,
        _target: QualificationTarget,
        scenario: ScenarioId,
    ) -> Result<Box<dyn QualificationSubject>, QualificationError> {
        self.next += 1;
        Ok(Box::new(Subject {
            id: format!("fresh-{}", self.next),
            scenario,
            unsupported: self.unsupported == Some(scenario),
        }))
    }
}

struct Subject {
    id: String,
    scenario: ScenarioId,
    unsupported: bool,
}

impl QualificationSubject for Subject {
    fn subject_id(&self) -> &str {
        &self.id
    }

    fn execute(
        &mut self,
        request: ScenarioRequest<'_>,
    ) -> Result<ScenarioObservation, QualificationError> {
        assert_eq!(request.scenario().id(), self.scenario);
        let mut evidence = EvidenceSet::new();
        evidence.insert(EvidenceEntry::new("direct.observation", EvidenceKind::Fact(true))?)?;
        ScenarioObservation::new(
            self.scenario,
            self.id.clone(),
            if self.unsupported {
                ObservationOutcome::Unsupported
            } else {
                ObservationOutcome::Passed
            },
            evidence,
        )
    }

    fn close(self: Box<Self>) -> Result<CleanupObservation, QualificationError> {
        CleanupObservation::new(self.id, true, 0, Sha256Digest::new([0xC2; 32]))
    }
}

#[test]
fn runner_uses_one_subject_per_scenario_and_can_reach_ready() {
    let (target, manifest) = fixture();
    let mut factory = Factory { next: 0, unsupported: None };
    let run = FreshSubjectRunner.run(&mut factory, target, &manifest).expect("run");
    assert_eq!(usize::from(factory.next), ScenarioId::all().len());
    let report = QualificationReport::evaluate(run);
    assert!(matches!(report.verdict(), ReadinessVerdict::Ready(_)));
}

#[test]
fn unsupported_required_scenario_is_not_ready() {
    let (target, manifest) = fixture();
    let mut factory = Factory { next: 0, unsupported: Some(ScenarioId::SandboxExecution) };
    let run = FreshSubjectRunner.run(&mut factory, target, &manifest).expect("run");
    let report = QualificationReport::evaluate(run);
    assert!(matches!(report.verdict(), ReadinessVerdict::NotReady(reasons) if !reasons.is_empty()));
}

fn fixture() -> (QualificationTarget, PackageManifest) {
    let target = QualificationTarget::new(
        Platform::Linux,
        Architecture::X86_64,
        PlatformVersion::new(6, 6, 0, 0),
    );
    let home = InstallPath::new(Platform::Linux, "/home/alice").expect("home");
    let layout = ReleaseLayout::production(Platform::Linux, &home).expect("layout");
    let roles = [
        (ArtifactRole::Daemon, "bin/peritusd", true),
        (ArtifactRole::Cli, "bin/peritus", true),
        (ArtifactRole::Tui, "bin/peritus-tui", true),
        (ArtifactRole::SandboxHelper, "libexec/peritus-linux-sandbox-helper", true),
        (ArtifactRole::ServiceDefinition, "share/peritus/peritus.service", false),
        (ArtifactRole::Installer, "Install-Peritus.sh", true),
        (ArtifactRole::Uninstaller, "Uninstall-Peritus.sh", true),
        (ArtifactRole::Upgrader, "Upgrade-Peritus.sh", true),
    ];
    let artifacts = roles
        .into_iter()
        .map(|(role, path, executable)| {
            ManifestArtifact::new(
                role,
                RelativePackagePath::new(path).expect("path"),
                digest_bytes(path.as_bytes()),
                executable,
            )
            .expect("artifact")
        })
        .collect();
    let manifest = PackageManifest::new(
        PackageVersion::new("0.1.0").expect("version"),
        Platform::Linux,
        Architecture::X86_64,
        layout.digest(),
        artifacts,
    )
    .expect("manifest");
    (target, manifest)
}
