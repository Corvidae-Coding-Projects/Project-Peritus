//! Conservative constraints for the originating submitted view, never a later recapture.

use crate::local_context::{error, memory::LocalMemory};
use peritus_agent::DeveloperLoopError;
use peritus_context::working::WorkingValidity;
use std::{
    collections::BTreeSet,
    io::Write as _,
    process::{Command, Stdio},
};

pub(in crate::local_context) fn capture(
    memory: &mut LocalMemory,
) -> Result<WorkingValidity, DeveloperLoopError> {
    if memory.workspace_scope.direct || !memory.transcript.pending.is_empty() {
        return Ok(WorkingValidity::uncertain());
    }
    let mut index = std::mem::take(&mut memory.decision_index);
    let refreshed = index.refresh(memory);
    memory.decision_index = index;
    refreshed?;
    if !memory.decision_index.dependencies_are_managed() || !reads_are_managed(memory) {
        return Ok(WorkingValidity::uncertain());
    }
    let environment = memory.state.environment();
    WorkingValidity::new(
        Some(environment.binding().conversation_revision()),
        Some(environment.candidate()),
        environment.files().to_vec(),
        memory.limits,
    )
    .map_err(|_| error("bind conservative host dependencies"))
}

fn reads_are_managed(memory: &LocalMemory) -> bool {
    let evidence = memory.decision_index.evidence();
    let paths =
        evidence.iter().filter_map(|source| source.file.as_deref()).collect::<BTreeSet<_>>();
    if paths.is_empty() {
        return true;
    }
    // Candidate capture excludes ignored untracked files. Check current coverage rather
    // than caching ignore policy with an immutable observation. Git also refuses paths
    // through submodules; without a proof of coverage those inputs require recheck.
    let Ok(mut child) = Command::new("git")
        .current_dir(&memory.workspace)
        .args(["check-ignore", "-z", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let written = child.stdin.take().is_some_and(|mut stdin| {
        paths.iter().all(|path| {
            stdin.write_all(path.as_bytes()).and_then(|()| stdin.write_all(&[0])).is_ok()
        })
    });
    // Always reap the child, including an early Git failure while writing its input.
    let status = child.wait();
    written && status.is_ok_and(|status| status.code() == Some(1))
}
