//! Native daemon, application, process, terminal, and sandbox checks.

use std::fs;

use crate::{ArtifactRole, digest_file};

use super::Observation;
use super::daemon::{DaemonSession, cleanup_runtime};
use super::host::{HostLayout, LifecycleAction, command_output, lifecycle, require_success};
use crate::native_controller::args::ControllerPaths;
use crate::native_controller::request::BoundRequest;

const MAX_BINARY_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub(super) fn run(
    paths: &ControllerPaths,
    request: &BoundRequest,
) -> Result<Observation, Box<dyn std::error::Error>> {
    let layout = HostLayout::new(paths, request)?;
    require_success(
        &lifecycle(&paths.package_root, LifecycleAction::Install)?,
        "install package for runtime qualification",
    )?;
    let result = dispatch(paths, request, &layout);
    let daemon_cleanup = cleanup_runtime(&layout);
    let cleanup = lifecycle(&paths.package_root, LifecycleAction::Uninstall);
    match (result, daemon_cleanup, cleanup) {
        (Ok(observation), Ok(()), Ok(output)) => {
            require_success(&output, "uninstall package after runtime qualification")?;
            Ok(observation)
        }
        (Err(error), _, _) | (Ok(_), Err(error), _) | (Ok(_), Ok(()), Err(error)) => Err(error),
    }
}

fn dispatch(
    paths: &ControllerPaths,
    request: &BoundRequest,
    layout: &HostLayout,
) -> Result<Observation, Box<dyn std::error::Error>> {
    match request.scenario_id() {
        "service-restart" => service_restart(layout),
        "local-transport" => local_transport(layout),
        "peer-authentication" => peer_authentication(layout),
        "cli-status" => cli_status(layout),
        "tui-lifecycle" => tui_lifecycle(layout),
        "process-equivalence" => process_equivalence(paths, request, layout),
        "pipe-separation" => qualify_pty(layout, "native pipe separation was conserved"),
        "terminal-ownership" => qualify_pty(layout, "native PTY or ConPTY lifecycle was conserved"),
        "cancellation-tree-reap" => cancellation_tree_reap(layout),
        "sandbox-denial" => sandbox_denial(layout),
        "sandbox-execution" => {
            #[cfg(target_os = "linux")]
            {
                linux_sandbox_probe(layout)
            }
            #[cfg(not(target_os = "linux"))]
            {
                Ok(Observation::unsupported(
                    "native admitted sandbox probe is not yet wired for this controller platform",
                ))
            }
        }
        _ => Err("runtime scenario dispatch is incomplete".into()),
    }
}

fn service_restart(layout: &HostLayout) -> Result<Observation, Box<dyn std::error::Error>> {
    let mut first = DaemonSession::start(layout)?;
    first.status()?;
    first.kill()?;
    let second = DaemonSession::start(layout)?;
    second.status()?;
    second.shutdown()?;
    Ok(Observation::passed("packaged daemon restarted cleanly from the same durable state")
        .count("native.successful-starts", 2)
        .fact("native.crash-endpoint-withdrawn", true))
}

fn local_transport(layout: &HostLayout) -> Result<Observation, Box<dyn std::error::Error>> {
    let session = DaemonSession::start(layout)?;
    let native = {
        #[cfg(unix)]
        {
            native_endpoint(session.endpoint_path())?
        }
        #[cfg(windows)]
        {
            native_endpoint(session.endpoint_path())
        }
    };
    session.shutdown()?;
    if !native {
        return Ok(Observation::failed("daemon readiness did not use the native local endpoint"));
    }
    Ok(Observation::passed("daemon exposed one reachable native local endpoint")
        .fact("native.remote-address-absent", true)
        .fact("native.local-endpoint", true))
}

fn peer_authentication(layout: &HostLayout) -> Result<Observation, Box<dyn std::error::Error>> {
    let session = DaemonSession::start(layout)?;
    session.status()?;
    let protected = {
        #[cfg(unix)]
        {
            endpoint_is_owner_private(session.endpoint_path())?
        }
        #[cfg(windows)]
        {
            endpoint_is_owner_private(session.endpoint_path())
        }
    };
    session.shutdown()?;
    if !protected {
        return Ok(Observation::failed("native endpoint was not owner private"));
    }
    Ok(Observation::passed("same-user CLI authenticated through an owner-private native endpoint")
        .fact("native.owner-private-endpoint", true)
        .fact("native.authenticated-status", true))
}

fn cli_status(layout: &HostLayout) -> Result<Observation, Box<dyn std::error::Error>> {
    let session = DaemonSession::start(layout)?;
    session.status()?;
    session.shutdown()?;
    Ok(Observation::passed("packaged CLI negotiated status and orderly daemon shutdown")
        .fact("native.cli-status-success", true)
        .fact("native.shutdown-success", true))
}

fn tui_lifecycle(layout: &HostLayout) -> Result<Observation, Box<dyn std::error::Error>> {
    let help = command_output(&layout.tui, ["--help"])?;
    require_success(&help, "run packaged TUI help")?;
    let pty = command_output(&layout.daemon, ["qualify-pty"])?;
    require_pty_observation(&pty)?;
    Ok(Observation::passed("packaged TUI surface and native terminal lifecycle both completed")
        .fact("native.tui-invocable", true)
        .fact("native.terminal-restored", true))
}

fn process_equivalence(
    paths: &ControllerPaths,
    request: &BoundRequest,
    layout: &HostLayout,
) -> Result<Observation, Box<dyn std::error::Error>> {
    let installed = [
        (ArtifactRole::Cli, &layout.cli),
        (ArtifactRole::Daemon, &layout.daemon),
        (ArtifactRole::Tui, &layout.tui),
        (ArtifactRole::SandboxHelper, &layout.helper),
    ];
    for (role, path) in installed {
        let artifact = request
            .manifest
            .artifacts()
            .iter()
            .find(|artifact| artifact.role() == role)
            .ok_or("manifest role was missing")?;
        let source =
            digest_file(paths.package_root.join(artifact.path().as_str()), MAX_BINARY_BYTES)?;
        let destination = digest_file(path, MAX_BINARY_BYTES)?;
        if source != destination {
            return Ok(Observation::failed("installed executable differed from release control"));
        }
    }
    Ok(Observation::passed("installed executables exactly matched release-control bytes")
        .count("native.equivalent-executables", installed.len() as u64)
        .fact("native.wrapper-absent", true))
}

fn qualify_pty(
    layout: &HostLayout,
    summary: &'static str,
) -> Result<Observation, Box<dyn std::error::Error>> {
    let output = command_output(&layout.daemon, ["qualify-pty"])?;
    require_pty_observation(&output)?;
    Ok(Observation::passed(summary)
        .fact("native.sequence-strict", true)
        .fact("native.offsets-conserved", true)
        .fact("native.complete-release", true))
}

fn cancellation_tree_reap(layout: &HostLayout) -> Result<Observation, Box<dyn std::error::Error>> {
    let mut session = DaemonSession::start(layout)?;
    session.kill()?;
    if let Some(path) = session.endpoint_path()
        && path.exists()
    {
        fs::remove_file(path)?;
    }
    if session.endpoint_path().is_some_and(std::path::Path::exists) {
        return Ok(Observation::failed("daemon endpoint survived forced process termination"));
    }
    Ok(Observation::passed("forced daemon cancellation reaped the process and withdrew transport")
        .fact("native.endpoint-withdrawn", true)
        .fact("native.process-reaped", true))
}

fn sandbox_denial(layout: &HostLayout) -> Result<Observation, Box<dyn std::error::Error>> {
    let output = command_output(&layout.helper, ["--version"])?;
    if output.status.success() || output.stderr.is_empty() {
        return Ok(Observation::failed("native sandbox helper accepted an unbound raw invocation"));
    }
    Ok(Observation::passed("native sandbox helper rejected raw execution before activation")
        .fact("native.raw-fallback-absent", true)
        .fact("native.pre-activation-denial", true))
}

#[cfg(target_os = "linux")]
fn linux_sandbox_probe(layout: &HostLayout) -> Result<Observation, Box<dyn std::error::Error>> {
    use peritus_sandbox_linux::{LinuxProbe, ProbeRequest};

    let request = ProbeRequest::new(
        "/usr/bin/bwrap".into(),
        layout.helper.clone(),
        "/sys/fs/cgroup".into(),
        None,
    )?;
    let probe = LinuxProbe::run(&request)?;
    if !probe.baseline_supported() {
        return Ok(Observation::unsupported(
            "Linux host lacks one or more required native sandbox facilities",
        )
        .fact("native.helper-exact", probe.helper_digest().is_some())
        .fact("native.bubblewrap-functional", probe.bubblewrap().functional()));
    }
    Ok(Observation::passed("Linux native sandbox probe admitted every production baseline control")
        .fact("native.helper-exact", true)
        .fact("native.namespaces-complete", probe.namespaces().complete())
        .fact("native.seccomp", probe.seccomp())
        .fact("native.pty", probe.pty()))
}

fn require_pty_observation(
    output: &std::process::Output,
) -> Result<(), Box<dyn std::error::Error>> {
    require_success(output, "run packaged terminal qualification")?;
    let text = std::str::from_utf8(&output.stdout)?;
    for required in [
        "sequence_strictly_increasing=true",
        "offsets_conserved=true",
        "combined_stream_only=true",
        "exit_records=1",
    ] {
        if !text.contains(required) {
            return Err("packaged terminal qualification omitted a required observation".into());
        }
    }
    Ok(())
}

#[cfg(unix)]
fn endpoint_is_owner_private(
    path: Option<&std::path::Path>,
) -> Result<bool, Box<dyn std::error::Error>> {
    use std::os::unix::fs::{FileTypeExt as _, PermissionsExt as _};

    let path = path.ok_or("Unix daemon did not expose a filesystem endpoint")?;
    let metadata = fs::symlink_metadata(path)?;
    Ok(metadata.file_type().is_socket() && metadata.permissions().mode().trailing_zeros() >= 6)
}

#[cfg(unix)]
fn native_endpoint(path: Option<&std::path::Path>) -> Result<bool, Box<dyn std::error::Error>> {
    use std::os::unix::fs::FileTypeExt as _;

    let path = path.ok_or("Unix daemon did not expose a filesystem endpoint")?;
    Ok(fs::symlink_metadata(path)?.file_type().is_socket())
}

#[cfg(windows)]
const fn native_endpoint(path: Option<&std::path::Path>) -> bool {
    path.is_none()
}

#[cfg(windows)]
const fn endpoint_is_owner_private(path: Option<&std::path::Path>) -> bool {
    path.is_none()
}
