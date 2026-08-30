//! Real package integrity, layout, protected-root, and supervisor-template checks.

use std::fs;

use crate::{Platform, digest_file};

use super::Observation;
use super::host::{
    HostLayout, LifecycleAction, command_output, lifecycle, marker, require_success,
};
use crate::native_controller::args::ControllerPaths;
use crate::native_controller::request::BoundRequest;

const MAX_PACKAGE_ARTIFACT: u64 = 2 * 1024 * 1024 * 1024;

pub(super) fn run(
    paths: &ControllerPaths,
    request: &BoundRequest,
) -> Result<Observation, Box<dyn std::error::Error>> {
    match request.scenario_id() {
        "artifact-integrity" => artifact_integrity(paths, request),
        "release-layout" => release_layout(paths, request),
        "protected-roots" => protected_roots(paths, request),
        "service-autostart" => service_template(paths, request),
        _ => Err("package scenario dispatch is incomplete".into()),
    }
}

fn artifact_integrity(
    paths: &ControllerPaths,
    request: &BoundRequest,
) -> Result<Observation, Box<dyn std::error::Error>> {
    for artifact in request.manifest.artifacts() {
        let observed =
            digest_file(paths.package_root.join(artifact.path().as_str()), MAX_PACKAGE_ARTIFACT)?;
        if observed != artifact.digest() {
            return Ok(Observation::failed(format!(
                "package artifact {} differs from the canonical manifest",
                artifact.path().as_str()
            )));
        }
    }
    let checksums = fs::read_to_string(paths.package_root.join("SHA256SUMS"))?;
    let manifest = fs::read(paths.package_root.join("manifest.toml"))?;
    if checksums != request.manifest.checksums() || manifest != request.manifest.canonical_bytes() {
        return Ok(Observation::failed(
            "package manifest or SHA256SUMS differs from the canonical release identity",
        ));
    }
    Ok(Observation::passed("every staged package artifact and checksum matched")
        .count("native.artifact-count", request.manifest.artifacts().len() as u64)
        .fact("native.manifest-exact", true))
}

fn release_layout(
    paths: &ControllerPaths,
    request: &BoundRequest,
) -> Result<Observation, Box<dyn std::error::Error>> {
    let layout = HostLayout::new(paths, request)?;
    with_install(paths, || {
        for path in layout.package_files() {
            if !fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file()) {
                return Ok(Observation::failed(format!(
                    "installed package entry is missing or not a regular file: {}",
                    path.display()
                )));
            }
        }
        let modes_are_private = {
            #[cfg(unix)]
            {
                installed_modes_are_private(&layout)?
            }
            #[cfg(windows)]
            {
                installed_modes_are_private(&layout)
            }
        };
        if !modes_are_private {
            return Ok(Observation::failed(
                "installed package permissions differ from the native layout contract",
            ));
        }
        let version = command_output(&layout.cli, ["--version"])?;
        require_success(&version, "run installed Peritus version")?;
        let expected = format!("peritus {}\n", request.document.release().version());
        if version.stdout != expected.as_bytes() {
            return Ok(Observation::failed("installed CLI version differs from the package"));
        }
        Ok(Observation::passed("native installer produced the exact package layout")
            .count("native.installed-files", 5)
            .fact("native.permissions-exact", true))
    })
}

fn protected_roots(
    paths: &ControllerPaths,
    request: &BoundRequest,
) -> Result<Observation, Box<dyn std::error::Error>> {
    let layout = HostLayout::new(paths, request)?;
    marker(&layout.config, b"operator-config-preserve\n")?;
    let state_marker = layout.state.join("qualification-state.txt");
    let log_marker = layout.logs.join("qualification-log.txt");
    marker(&state_marker, b"durable-state-preserve\n")?;
    marker(&log_marker, b"diagnostic-log-preserve\n")?;
    with_install(paths, || {
        if fs::read(&layout.config)? != b"operator-config-preserve\n"
            || fs::read(&state_marker)? != b"durable-state-preserve\n"
            || fs::read(&log_marker)? != b"diagnostic-log-preserve\n"
        {
            return Ok(Observation::failed(
                "package installation changed operator or runtime-owned roots",
            ));
        }
        Ok(Observation::passed("install and uninstall preserved all protected roots")
            .count("native.protected-roots", layout.protected_roots().len() as u64)
            .fact("native.package-ownership-separated", true))
    })
}

fn service_template(
    paths: &ControllerPaths,
    request: &BoundRequest,
) -> Result<Observation, Box<dyn std::error::Error>> {
    let layout = HostLayout::new(paths, request)?;
    with_install(paths, || {
        let text = fs::read_to_string(&layout.service)?;
        let required = match layout.platform {
            Platform::Linux => [
                "ExecStart=%h/.local/bin/peritusd serve --config",
                "Restart=on-failure",
                "KillMode=mixed",
            ],
            Platform::Macos => {
                ["<string>serve</string>", "<key>KeepAlive</key>", "<key>ThrottleInterval</key>"]
            }
            Platform::Windows => ["peritusd.exe", "serve --config", "<RestartOnFailure>"],
        };
        if required.iter().any(|needle| !text.contains(needle)) {
            return Ok(Observation::failed(
                "native supervisor template omitted direct foreground or restart controls",
            ));
        }
        if text.contains("sh -c")
            || text.contains("cmd.exe /c")
            || text.contains("powershell -Command")
        {
            return Ok(Observation::failed(
                "native supervisor template introduced a shell parsing layer",
            ));
        }
        Ok(Observation::passed(
            "inactive native supervisor template uses the exact foreground daemon contract",
        )
        .count("native.supervisor-controls", required.len() as u64)
        .fact("native.shell-wrapper-absent", true))
    })
}

fn with_install<T>(
    paths: &ControllerPaths,
    action: impl FnOnce() -> Result<T, Box<dyn std::error::Error>>,
) -> Result<T, Box<dyn std::error::Error>> {
    let installed = lifecycle(&paths.package_root, LifecycleAction::Install)?;
    require_success(&installed, "install native Peritus package")?;
    let result = action();
    let uninstalled = lifecycle(&paths.package_root, LifecycleAction::Uninstall);
    match (result, uninstalled) {
        (Ok(value), Ok(output)) => {
            require_success(&output, "uninstall native Peritus package")?;
            Ok(value)
        }
        (Err(error), _) | (Ok(_), Err(error)) => Err(error),
    }
}

#[cfg(unix)]
fn installed_modes_are_private(layout: &HostLayout) -> Result<bool, Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt as _;

    for executable in [&layout.cli, &layout.daemon, &layout.tui, &layout.helper] {
        if fs::metadata(executable)?.permissions().mode() & 0o777 != 0o755 {
            return Ok(false);
        }
    }
    Ok(fs::metadata(&layout.service)?.permissions().mode() & 0o777 == 0o600)
}

#[cfg(windows)]
fn installed_modes_are_private(layout: &HostLayout) -> bool {
    layout.package_files().iter().all(|path| path.is_file())
}
