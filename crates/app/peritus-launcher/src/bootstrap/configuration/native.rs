//! Conservative discovery of packaged native-isolation components.

use std::path::PathBuf;

/// Complete host paths needed by the Linux native module and candidate runtime.
pub(super) struct NativeRuntime {
    pub(super) bubblewrap: PathBuf,
    pub(super) linux_helper: PathBuf,
    pub(super) cgroup_root: PathBuf,
}

/// Discovers a complete native runtime or advertises none of it.
///
/// The launcher never emits a partial declaration. The daemon's native probe remains the
/// authority for actual platform support; this only connects components installed as one package
/// to the strict generated configuration.
#[cfg(target_os = "linux")]
pub(super) fn discover() -> Option<NativeRuntime> {
    let application = std::fs::canonicalize(std::env::current_exe().ok()?).ok()?;
    let bin = application.parent()?;
    let prefix = bin.parent()?;
    let helper_name = "peritus-linux-sandbox-helper";
    let linux_helper = [bin.join(helper_name), prefix.join("libexec/peritus").join(helper_name)]
        .into_iter()
        .find_map(executable)?;
    let bubblewrap = [PathBuf::from("/usr/bin/bwrap"), PathBuf::from("/bin/bwrap")]
        .into_iter()
        .find_map(executable)?;
    let cgroup_root = delegated_user_cgroup()?;
    Some(NativeRuntime { bubblewrap, linux_helper, cgroup_root })
}

#[cfg(not(target_os = "linux"))]
pub(super) const fn discover() -> Option<NativeRuntime> {
    None
}

#[cfg(target_os = "linux")]
fn executable(path: PathBuf) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt as _;

    let path = std::fs::canonicalize(path).ok()?;
    let metadata = std::fs::metadata(&path).ok()?;
    (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0).then_some(path)
}

#[cfg(target_os = "linux")]
fn delegated_user_cgroup() -> Option<PathBuf> {
    use std::{collections::BTreeSet, fs::OpenOptions, path::Component};

    let membership = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    let relative = membership.lines().find_map(|line| line.strip_prefix("0::"))?;
    let mut root = PathBuf::from("/sys/fs/cgroup");
    let mut found_user_service = false;
    for component in std::path::Path::new(relative).components() {
        match component {
            Component::RootDir => {}
            Component::Normal(component) => {
                root.push(component);
                let component = component.to_str()?;
                if component.starts_with("user@") && component.ends_with(".service") {
                    found_user_service = true;
                    break;
                }
            }
            _ => return None,
        }
    }
    if !found_user_service {
        return None;
    }
    let words = |name: &str| {
        std::fs::read_to_string(root.join(name))
            .ok()
            .map(|value| value.split_whitespace().map(str::to_owned).collect::<BTreeSet<_>>())
    };
    let available = words("cgroup.controllers")?;
    let delegated = words("cgroup.subtree_control")?;
    if !["cpu", "memory", "pids"]
        .iter()
        .all(|name| available.contains(*name) && delegated.contains(*name))
        || OpenOptions::new().write(true).open(root.join("cgroup.procs")).is_err()
        || OpenOptions::new().write(true).open(root.join("cgroup.subtree_control")).is_err()
    {
        return None;
    }
    std::fs::canonicalize(root).ok()
}
