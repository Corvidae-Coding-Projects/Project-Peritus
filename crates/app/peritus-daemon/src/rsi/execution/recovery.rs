//! Recovery of a proved preparation's empty cgroup; never guessed ownership or broad signals.

use std::fmt::Write as _;

use peritus_types::Sha256Digest;

use super::{LocalProcessSandbox, detail};

/// Removes only the exact empty preparation leaf after C2 proved its recorded owner absent.
///
/// The caller must obtain the digest from its durable admitted rollout process receipt and
/// reconcile that exact `(run, action, process)` using C2's native birth-identity probe first.
/// This function never signals a process. A populated, aliased, malformed, or inaccessible leaf
/// is an error, leaving the rollout unsettled rather than inventing successful cleanup.
pub(crate) fn cleanup_empty_preparation(
    native: &LocalProcessSandbox,
    preparation: Sha256Digest,
) -> Result<(), String> {
    let root = native.cgroup_root.canonicalize().map_err(detail)?;
    if root != native.cgroup_root || root == std::path::Path::new("/sys/fs/cgroup") {
        return Err("RSI recovery requires its exact delegated cgroup parent".into());
    }
    let mut leaf_name = "peritus-".to_owned();
    for byte in &preparation.as_bytes()[..12] {
        write!(&mut leaf_name, "{byte:02x}").map_err(detail)?;
    }
    let leaf = root.join(leaf_name);
    let metadata = match std::fs::symlink_metadata(&leaf) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(detail(error)),
    };
    if !metadata.file_type().is_dir() || leaf.canonicalize().map_err(detail)? != leaf {
        return Err("RSI preparation cgroup identity changed before empty cleanup".into());
    }
    let procs = std::fs::read_to_string(leaf.join("cgroup.procs")).map_err(detail)?;
    let events = std::fs::read_to_string(leaf.join("cgroup.events")).map_err(detail)?;
    if procs.split_whitespace().next().is_some()
        || !events.lines().any(|line| line.split_whitespace().eq(["populated", "0"]))
    {
        return Err(
            "RSI preparation retains native processes; exact C2 recovery is required".into()
        );
    }
    // A process entering after observation makes rmdir fail; it never authorizes a kill.
    std::fs::remove_dir(&leaf).map_err(detail)
}
