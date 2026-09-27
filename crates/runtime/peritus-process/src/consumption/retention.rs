use std::path::Path;

use super::StoreState;
use crate::{
    LifecyclePhase, ProcessError,
    registry_storage::{hex, retire_execution_record},
};

pub(super) fn execution_record_count(state: &StoreState) -> usize {
    state.claims.len()
        + state.manifests.keys().filter(|process_id| !state.claims.contains_key(process_id)).count()
}

pub(super) fn retire_terminal_records(
    claims: &Path,
    manifests: &Path,
    spools: &Path,
    state: &mut StoreState,
    target: usize,
) -> Result<(), ProcessError> {
    if execution_record_count(state) <= target {
        return Ok(());
    }
    let mut terminal = state
        .manifests
        .iter()
        .filter(|(_, manifest)| manifest.phase == LifecyclePhase::Terminal)
        .map(|(process_id, _)| {
            let path = manifests.join(format!("{}.manifest", hex(process_id.as_bytes())));
            let modified = path
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            (modified, *process_id)
        })
        .collect::<Vec<_>>();
    terminal.sort_unstable_by_key(|(modified, process_id)| (*modified, *process_id));
    for (_, process_id) in terminal {
        if execution_record_count(state) <= target {
            break;
        }
        retire_execution_record(claims, manifests, spools, process_id)?;
        state.claims.remove(&process_id);
        state.manifests.remove(&process_id);
    }
    Ok(())
}
