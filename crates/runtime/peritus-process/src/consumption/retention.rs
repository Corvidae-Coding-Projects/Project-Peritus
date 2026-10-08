use std::path::Path;
use peritus_types::ProcessId;

use super::{StoreState, retirable, store_error};
use crate::{
    ProcessError,
    registry_storage::{
        acquire_retained_owner_transaction, persist_tombstone, preserve_consumption_claim,
        retire_execution_record,
    },
};

pub(super) fn retire_settled_records(
    root: &Path,
    claims: &Path,
    manifests: &Path,
    spools: &Path,
    state: &mut StoreState,
    target: usize,
) -> Result<(), ProcessError> {
    let tombstones = manifests.parent()
        .ok_or_else(|| store_error("process manifest directory has no protected parent"))?
        .join("tombstones-v1");
    let retained_owners = manifests
        .parent()
        .ok_or_else(|| store_error("process manifest directory has no protected parent"))?
        .join("retained-owners-v1");
    let mut after = None;
    let mut active = state.index.active_count()?;
    loop {
        let page = state.index.page(after)?;
        if page.is_empty() { break; }
        for record in page {
            after = Some(record.process_id);
            if !record.retired && active <= target { return Ok(()); }
            let _transaction =
                acquire_retained_owner_transaction(&retained_owners, record.process_id)?;
            {
                let StoreState { index, quarantined_records, .. } = &mut *state;
                index.reconcile_identity(root, record.process_id, quarantined_records)?;
            }
            retire_record(claims, manifests, spools, &tombstones, state, record.process_id)?;
            if !record.retired && state.index.get(record.process_id)?.is_some_and(|record| record.retired) {
                active = active.checked_sub(1).ok_or_else(|| store_error("process retirement count underflow"))?;
            }
        }
    }
    Ok(())
}

pub(super) fn retire_record(
    claims: &Path,
    manifests: &Path,
    spools: &Path,
    tombstones: &Path,
    state: &mut StoreState,
    process_id: ProcessId,
) -> Result<(), ProcessError> {
    let record = state.index.get(process_id)?
        .ok_or_else(|| store_error("indexed process record disappeared during retirement"))?;
    let Some((claim, manifest)) = record.claim.zip(record.manifest) else { return Ok(()); };
    if !claim.matches_manifest(&manifest) || !retirable(&manifest) { return Ok(()); }
    if !record.retired {
        preserve_consumption_claim(claims, claim)?;
        persist_tombstone(tombstones, claim, &manifest)?;
        state.index.tombstone(claim, &manifest)?;
    } else if !claims.join(format!("{}.claim", crate::registry_storage::hex(process_id.as_bytes()))).exists() {
        preserve_consumption_claim(claims, claim)?;
    }
    // The immutable result and one-use claim are durable before any active file is removed.
    retire_execution_record(claims, manifests, spools, process_id)
}
