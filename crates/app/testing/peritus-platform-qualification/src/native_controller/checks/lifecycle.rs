//! Native upgrade, rollback, and uninstall preservation checks.

use std::fs;
use std::io::Write as _;
use std::path::PathBuf;

use crate::{ArtifactRole, digest_file};

use super::Observation;
use super::host::{HostLayout, LifecycleAction, lifecycle, marker, require_success};
use crate::native_controller::args::ControllerPaths;
use crate::native_controller::request::BoundRequest;

const MAX_BINARY_BYTES: u64 = 2 * 1024 * 1024 * 1024;
type ProtectedMarker = (PathBuf, Vec<u8>);

pub(super) fn run(
    paths: &ControllerPaths,
    request: &BoundRequest,
) -> Result<Observation, Box<dyn std::error::Error>> {
    match request.scenario_id() {
        "upgrade-preservation" => upgrade_preservation(paths, request),
        "upgrade-rollback" => upgrade_rollback(paths, request),
        "uninstall-preservation" => uninstall_preservation(paths, request),
        _ => Err("lifecycle scenario dispatch is incomplete".into()),
    }
}

fn upgrade_preservation(
    paths: &ControllerPaths,
    request: &BoundRequest,
) -> Result<Observation, Box<dyn std::error::Error>> {
    let layout = HostLayout::new(paths, request)?;
    let markers = protected_markers(&layout)?;
    require_success(
        &lifecycle(&paths.package_root, LifecycleAction::Install)?,
        "install package before upgrade",
    )?;
    let before = digest_file(&layout.cli, MAX_BINARY_BYTES)?;
    require_success(
        &lifecycle(&paths.package_root, LifecycleAction::Upgrade)?,
        "upgrade native package",
    )?;
    let after = digest_file(&layout.cli, MAX_BINARY_BYTES)?;
    let preserved =
        markers.iter().all(|(path, expected)| fs::read(path).is_ok_and(|bytes| bytes == *expected));
    let uninstall = lifecycle(&paths.package_root, LifecycleAction::Uninstall)?;
    require_success(&uninstall, "uninstall package after upgrade qualification")?;
    if before != after || !preserved {
        return Ok(Observation::failed(
            "repeat native upgrade changed package identity or protected state",
        ));
    }
    Ok(Observation::passed("native upgrade was atomic and preserved protected roots")
        .count("native.preserved-markers", markers.len() as u64)
        .fact("native.upgrade-identity-exact", true))
}

fn upgrade_rollback(
    paths: &ControllerPaths,
    request: &BoundRequest,
) -> Result<Observation, Box<dyn std::error::Error>> {
    let layout = HostLayout::new(paths, request)?;
    let markers = protected_markers(&layout)?;
    require_success(
        &lifecycle(&paths.package_root, LifecycleAction::Install)?,
        "install package before rollback qualification",
    )?;
    let installed = digest_file(&layout.cli, MAX_BINARY_BYTES)?;
    let cli_artifact = request
        .manifest
        .artifacts()
        .iter()
        .find(|artifact| artifact.role() == ArtifactRole::Cli)
        .ok_or("manifest omitted the CLI artifact")?;
    let corrupt_path = paths.package_root.join(cli_artifact.path().as_str());
    fs::OpenOptions::new().append(true).open(&corrupt_path)?.write_all(b"corrupt")?;
    let failed_upgrade = lifecycle(&paths.package_root, LifecycleAction::Upgrade)?;
    let restored = digest_file(&layout.cli, MAX_BINARY_BYTES)?;
    let preserved =
        markers.iter().all(|(path, expected)| fs::read(path).is_ok_and(|bytes| bytes == *expected));
    let uninstall = lifecycle(&paths.package_root, LifecycleAction::Uninstall)?;
    require_success(&uninstall, "uninstall package after rollback qualification")?;
    if failed_upgrade.status.success() || installed != restored || !preserved {
        return Ok(Observation::failed(
            "failed native upgrade did not restore package and protected state exactly",
        ));
    }
    Ok(Observation::passed("checksum failure triggered exact native package rollback")
        .fact("native.failure-injected", true)
        .fact("native.rollback-exact", true)
        .count("native.preserved-markers", markers.len() as u64))
}

fn uninstall_preservation(
    paths: &ControllerPaths,
    request: &BoundRequest,
) -> Result<Observation, Box<dyn std::error::Error>> {
    let layout = HostLayout::new(paths, request)?;
    let markers = protected_markers(&layout)?;
    require_success(
        &lifecycle(&paths.package_root, LifecycleAction::Install)?,
        "install package before uninstall qualification",
    )?;
    require_success(
        &lifecycle(&paths.package_root, LifecycleAction::Uninstall)?,
        "uninstall native package",
    )?;
    let removed = layout.package_files().iter().all(|path| !path.exists());
    let preserved =
        markers.iter().all(|(path, expected)| fs::read(path).is_ok_and(|bytes| bytes == *expected));
    if !removed || !preserved {
        return Ok(Observation::failed("native uninstall crossed its package ownership boundary"));
    }
    Ok(Observation::passed("native uninstall removed package files and preserved user state")
        .count("native.removed-package-files", layout.package_files().len() as u64)
        .count("native.preserved-markers", markers.len() as u64))
}

fn protected_markers(
    layout: &HostLayout,
) -> Result<Vec<ProtectedMarker>, Box<dyn std::error::Error>> {
    let markers = vec![
        (layout.config.clone(), b"configuration-preserve\n".to_vec()),
        (layout.state.join("qualification-state.txt"), b"state-preserve\n".to_vec()),
        (layout.logs.join("qualification-log.txt"), b"logs-preserve\n".to_vec()),
    ];
    for (path, bytes) in &markers {
        marker(path, bytes)?;
    }
    Ok(markers)
}
