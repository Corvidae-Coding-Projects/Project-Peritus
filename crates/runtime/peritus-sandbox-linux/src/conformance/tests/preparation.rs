//! Canonical inert Linux preparation for the A2 preparation case.

use super::{
    adapter::LinuxConformanceSubject,
    fixture::{FileShape, NetworkShape, PlanShape, TerminalShape, checked_preparation_plan},
};
use peritus_conformance::{
    SandboxFeature as ConformanceFeature, SandboxPreparationFixture, SandboxPreparationObservation,
};
use peritus_sandbox::{
    AdmissionProfile, BackendDescriptor, BackendKind, BackendName, BackendVersion, FeatureSet,
    PathSemantics, ResourceFidelity, SandboxFeature, admit_backend,
};
use peritus_types::Sha256Digest;

pub(super) fn prepare(
    subject: &LinuxConformanceSubject,
    fixture: &SandboxPreparationFixture,
) -> Result<SandboxPreparationObservation, ()> {
    let mut canonical = fixture.required_features().to_vec();
    canonical.sort_unstable();
    canonical.dedup();
    let missing = canonical
        .iter()
        .copied()
        .filter(|feature| !fixture.backend_features().contains(feature))
        .collect::<Vec<_>>();
    let marker = u8::try_from(fixture.authority_marker()).map_err(|_| ())?;
    let plan =
        checked_preparation_plan(subject.workspace(), shape(&canonical), marker).map_err(|_| ())?;
    let missing_runtime = FeatureSet::from_features(missing.iter().copied().map(runtime_feature));
    let supported = FeatureSet::from_features(
        plan.required_features().iter().filter(|feature| !missing_runtime.contains(*feature)),
    );
    let descriptor = BackendDescriptor::new(
        BackendName::new(crate::BACKEND_NAME).map_err(|_| ())?,
        BackendVersion::new(crate::BACKEND_VERSION).map_err(|_| ())?,
        BackendKind::Native,
        PathSemantics::UnixNative,
        ResourceFidelity::Hard,
        supported,
    );
    let admission = admit_backend(&plan, &descriptor, AdmissionProfile::Conformance);
    let (admitted, preparation_digest, canonical_bytes) = match admission {
        Ok(admission) => {
            let (_, manifest) = subject.project_manifest(&plan, &admission)?;
            (true, admission.preparation_digest(), manifest.encode().map_err(|_| ())?)
        }
        Err(_) => (false, Sha256Digest::new([0; 32]), plan.canonical_bytes().to_vec()),
    };
    Ok(SandboxPreparationObservation::new(
        canonical,
        missing,
        *plan.digest().as_bytes(),
        *preparation_digest.as_bytes(),
        admitted,
        canonical_bytes,
        0,
    ))
}

fn shape(features: &[ConformanceFeature]) -> PlanShape {
    let mut shape = PlanShape::baseline(8);
    if features.contains(&ConformanceFeature::FilesystemRead)
        || features.contains(&ConformanceFeature::FilesystemWrite)
    {
        shape.filesystem = FileShape::Allow;
    }
    shape.filesystem_write = features.contains(&ConformanceFeature::FilesystemWrite);
    shape.environment_secret = features.contains(&ConformanceFeature::EnvironmentLiteral)
        || features.contains(&ConformanceFeature::SecretEnvironment);
    if features.contains(&ConformanceFeature::NetworkOutbound) {
        shape.network = NetworkShape::Allow;
    }
    if features.contains(&ConformanceFeature::Descendants) {
        shape.descendant_limit = 1;
        shape.descendant_required = 1;
    }
    shape.terminal = if features.contains(&ConformanceFeature::Resize) {
        TerminalShape::PtyResize
    } else if features.contains(&ConformanceFeature::Pty) {
        TerminalShape::Pty
    } else {
        TerminalShape::Pipes
    };
    shape
}

const fn runtime_feature(feature: ConformanceFeature) -> SandboxFeature {
    match feature {
        ConformanceFeature::FilesystemRead => SandboxFeature::FilesystemRead,
        ConformanceFeature::FilesystemWrite => SandboxFeature::FilesystemWrite,
        ConformanceFeature::Descendants => SandboxFeature::ProcessDescendants,
        ConformanceFeature::EnvironmentLiteral => SandboxFeature::EnvironmentClear,
        ConformanceFeature::NetworkOutbound => SandboxFeature::NetworkEgress,
        ConformanceFeature::SecretEnvironment => SandboxFeature::SecretEnvironment,
        ConformanceFeature::WallTime => SandboxFeature::WallTime,
        ConformanceFeature::OutputBytes => SandboxFeature::Output,
        ConformanceFeature::Pty => SandboxFeature::Pty,
        ConformanceFeature::Resize => SandboxFeature::TerminalResize,
        ConformanceFeature::TreeContainment => SandboxFeature::ProcessTree,
    }
}
