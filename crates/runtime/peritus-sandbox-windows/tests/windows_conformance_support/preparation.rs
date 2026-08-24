use peritus_conformance::{
    SandboxFeature, SandboxPreparationFixture, SandboxPreparationObservation,
};

use super::{
    plan::{FileShape, NetworkShape, PlanShape, TerminalShape, preparation},
    projection::ProjectedSession,
};

pub fn run(fixture: &SandboxPreparationFixture) -> Result<SandboxPreparationObservation, ()> {
    let mut canonical = fixture.required_features().to_vec();
    canonical.sort_unstable();
    canonical.dedup();
    let missing = canonical
        .iter()
        .copied()
        .filter(|feature| !fixture.backend_features().contains(feature))
        .collect::<Vec<_>>();
    let marker = u8::try_from(fixture.authority_marker()).map_err(|_| ())?;
    let plan = preparation(shape(&canonical), marker).map_err(|_| ())?;
    if !missing.is_empty() {
        return Ok(SandboxPreparationObservation::new(
            canonical,
            missing,
            *plan.digest().as_bytes(),
            [0; 32],
            false,
            plan.canonical_bytes().to_vec(),
            0,
        ));
    }
    let session = ProjectedSession::prepare(&plan).map_err(|_| ())?;
    Ok(SandboxPreparationObservation::new(
        canonical,
        missing,
        *plan.digest().as_bytes(),
        *session.manifest().preparation_digest().as_bytes(),
        true,
        session.manifest().canonical_bytes().to_vec(),
        0,
    ))
}

fn shape(features: &[SandboxFeature]) -> PlanShape {
    let mut shape = PlanShape::baseline(8);
    if features.contains(&SandboxFeature::FilesystemRead)
        || features.contains(&SandboxFeature::FilesystemWrite)
    {
        shape.file = FileShape::Allow;
    }
    shape.file_write = features.contains(&SandboxFeature::FilesystemWrite);
    shape.environment_secret = features.contains(&SandboxFeature::EnvironmentLiteral)
        || features.contains(&SandboxFeature::SecretEnvironment);
    if features.contains(&SandboxFeature::NetworkOutbound) {
        shape.network = NetworkShape::Allow;
    }
    if features.contains(&SandboxFeature::Descendants) {
        shape.descendants = 1;
        shape.required_descendants = 1;
    }
    shape.terminal = if features.contains(&SandboxFeature::Resize) {
        TerminalShape::PtyResize
    } else if features.contains(&SandboxFeature::Pty) {
        TerminalShape::Pty
    } else {
        TerminalShape::Pipes
    };
    shape
}
