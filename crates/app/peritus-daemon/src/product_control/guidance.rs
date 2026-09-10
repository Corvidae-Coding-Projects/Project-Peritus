//! C0 planning and replay for explicit project-local guidance and content-free tombstones.

use super::ControlStoreError as Error;
use peritus_app_protocol::{
    AppErrorCode, AppProtocolError, ControlOperationId, MAX_WORKBENCH_GUIDANCE_RECORDS,
    WorkbenchGuidanceForget, WorkbenchGuidancePin, WorkbenchGuidanceRecord,
    WorkbenchGuidanceRevision, WorkbenchGuidanceSave, WorkbenchGuidanceScopeChange,
    WorkbenchGuidanceTombstone, WorkbenchMemoryRow,
};
use peritus_journal::{DurableStateRecord, StateInstall};
use peritus_product_runner::control::ControlError;
use peritus_types::WorkspaceId;
use serde::Serialize;
use serde::de::DeserializeOwned;

#[cfg(test)]
mod tests;

mod stored;
use stored::{StoredCatalog, StoredSlot, StoredSlotState, StoredTombstone};

pub(super) const GUIDANCE_NAMESPACE: u16 = 3541;
pub(super) const GUIDANCE_CONTROL_NAMESPACE: u16 = 3542;
const STORAGE_SCHEMA: u16 = 1;
const MAX_STORED_BYTES: usize = 256 * 1024;

/// One of the five explicit user-approved guidance mutations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GuidanceMutation {
    Save(WorkbenchGuidanceSave),
    Revise(WorkbenchGuidanceRevision),
    Pin(WorkbenchGuidancePin),
    Scope(WorkbenchGuidanceScopeChange),
    Forget(WorkbenchGuidanceForget),
}

impl GuidanceMutation {
    pub(super) const fn identity(&self, save_operation: ControlOperationId) -> ControlOperationId {
        match self {
            Self::Save(_) => save_operation,
            Self::Revise(value) => value.selection().id(),
            Self::Pin(value) => value.selection().id(),
            Self::Scope(value) => value.selection().id(),
            Self::Forget(value) => value.selection().id(),
        }
    }

    const fn expected_dependency_revision(&self) -> u64 {
        match self {
            Self::Save(value) => value.expected_dependency_revision(),
            Self::Revise(value) => value.expected_dependency_revision(),
            Self::Pin(value) => value.expected_dependency_revision(),
            Self::Scope(value) => value.expected_dependency_revision(),
            Self::Forget(value) => value.expected_dependency_revision(),
        }
    }
}

/// Exact C0 observations used by a pure guidance mutation plan.
#[derive(Clone, Copy)]
pub(super) struct GuidanceObservation<'a> {
    catalog: Option<&'a DurableStateRecord>,
    guidance: Option<&'a DurableStateRecord>,
    tombstone: Option<&'a DurableStateRecord>,
}

impl<'a> GuidanceObservation<'a> {
    pub(super) const fn new(
        catalog: Option<&'a DurableStateRecord>,
        guidance: Option<&'a DurableStateRecord>,
        tombstone: Option<&'a DurableStateRecord>,
    ) -> Self {
        Self { catalog, guidance, tombstone }
    }
}

/// Rebuildable workspace identity catalog and dependent-view revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct GuidanceCatalog {
    workspace: WorkspaceId,
    revision: u64,
    identities: Vec<ControlOperationId>,
}

impl GuidanceCatalog {
    pub(super) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) fn identities(&self) -> &[ControlOperationId] {
        &self.identities
    }

    const fn empty(workspace: WorkspaceId) -> Self {
        Self { workspace, revision: 0, identities: Vec::new() }
    }

    fn advance(&self, add: Option<ControlOperationId>) -> Result<Self, Error> {
        let mut identities = self.identities.clone();
        if let Some(identity) = add {
            match identities.binary_search(&identity) {
                Ok(_) => return Err(ControlError::IdempotencyConflict.into()),
                Err(index) => identities.insert(index, identity),
            }
        }
        if identities.len() > MAX_WORKBENCH_GUIDANCE_RECORDS {
            return Err(ControlError::Capacity.into());
        }
        let revision = self.revision.checked_add(1).ok_or(ControlError::Capacity)?;
        Ok(Self { workspace: self.workspace, revision, identities })
    }
}

/// Checked active or forgotten projection produced by the same state installs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum GuidanceProjection {
    Active(WorkbenchGuidanceRecord),
    Forgotten(WorkbenchGuidanceTombstone),
}

/// Ordered C0 compare-and-swap installs and their exact replay projection.
pub(super) struct GuidanceInstallPlan {
    catalog: GuidanceCatalog,
    projection: GuidanceProjection,
    installs: Vec<StateInstall>,
}

impl GuidanceInstallPlan {
    pub(super) const fn catalog(&self) -> &GuidanceCatalog {
        &self.catalog
    }

    pub(super) const fn projection(&self) -> &GuidanceProjection {
        &self.projection
    }

    pub(super) fn into_installs(self) -> Vec<StateInstall> {
        self.installs
    }
}

/// Plans exact guidance, tombstone, and dependent-view CAS installs without performing I/O.
pub(super) fn plan_guidance(
    operation: ControlOperationId,
    workspace: WorkspaceId,
    mutation: GuidanceMutation,
    observed: GuidanceObservation<'_>,
) -> Result<GuidanceInstallPlan, Error> {
    let catalog = replay_catalog(workspace, observed.catalog)?;
    if catalog.revision() != mutation.expected_dependency_revision() {
        return Err(ControlError::StaleRevision.into());
    }
    let identity = mutation.identity(operation);
    let key = guidance_key(workspace, identity);
    validate_observed_key(observed.guidance, GUIDANCE_NAMESPACE, &key)?;
    validate_observed_key(observed.tombstone, GUIDANCE_CONTROL_NAMESPACE, &key)?;

    let (projection, slot, tombstone, add_identity) = match mutation {
        GuidanceMutation::Save(save) => {
            if observed.guidance.is_some() || observed.tombstone.is_some() {
                return Err(ControlError::IdempotencyConflict.into());
            }
            let record = map_app(WorkbenchGuidanceRecord::save(operation, workspace, save))?;
            let slot = StoredSlot::active(&record);
            (GuidanceProjection::Active(record), slot, None, Some(identity))
        }
        GuidanceMutation::Revise(change) => {
            let current = require_active(workspace, identity, observed)?;
            let record = map_app(current.revise(operation, change))?;
            let slot = StoredSlot::active(&record);
            (GuidanceProjection::Active(record), slot, None, None)
        }
        GuidanceMutation::Pin(change) => {
            let current = require_active(workspace, identity, observed)?;
            let record = map_app(current.set_pinned(change))?;
            let slot = StoredSlot::active(&record);
            (GuidanceProjection::Active(record), slot, None, None)
        }
        GuidanceMutation::Scope(change) => {
            let current = require_active(workspace, identity, observed)?;
            let record = map_app(current.set_scope(change))?;
            let slot = StoredSlot::active(&record);
            (GuidanceProjection::Active(record), slot, None, None)
        }
        GuidanceMutation::Forget(change) => {
            let current = require_active(workspace, identity, observed)?;
            let tombstone = map_app(current.forget(operation, change))?;
            let bytes = encode(&StoredTombstone::from_public(&tombstone))?;
            let slot = StoredSlot::forgotten(&tombstone, peritus_codec::sha256(&bytes))?;
            (GuidanceProjection::Forgotten(tombstone), slot, Some(bytes), None)
        }
    };
    let next_catalog = catalog.advance(add_identity)?;
    let expected_slot_revision = observed.guidance.map(DurableStateRecord::revision);
    let mut installs = vec![StateInstall::new(
        GUIDANCE_NAMESPACE,
        key.clone(),
        expected_slot_revision,
        slot.record_revision,
        encode(&slot)?,
    )?];
    installs.push(StateInstall::new(
        GUIDANCE_CONTROL_NAMESPACE,
        catalog_key(workspace),
        (catalog.revision() != 0).then_some(catalog.revision()),
        next_catalog.revision(),
        encode(&StoredCatalog::from_domain(&next_catalog))?,
    )?);
    if let Some(bytes) = tombstone {
        installs.push(StateInstall::new(GUIDANCE_CONTROL_NAMESPACE, key, None, 1, bytes)?);
    }
    Ok(GuidanceInstallPlan { catalog: next_catalog, projection, installs })
}

/// Replays and validates the workspace guidance identity catalog from C0.
pub(super) fn replay_catalog(
    workspace: WorkspaceId,
    record: Option<&DurableStateRecord>,
) -> Result<GuidanceCatalog, Error> {
    let Some(record) = record else {
        return Ok(GuidanceCatalog::empty(workspace));
    };
    validate_observed_key(Some(record), GUIDANCE_CONTROL_NAMESPACE, &catalog_key(workspace))?;
    let stored: StoredCatalog = decode(record.bytes())?;
    if stored.schema != STORAGE_SCHEMA
        || stored.workspace != *workspace.as_bytes()
        || stored.revision == 0
        || stored.revision != record.revision()
        || stored.identities.len() > MAX_WORKBENCH_GUIDANCE_RECORDS
        || stored.identities.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(Error::Corrupt("invalid guidance catalog"));
    }
    let identities = stored
        .identities
        .into_iter()
        .map(|bytes| {
            ControlOperationId::new(bytes).map_err(|_| Error::Corrupt("invalid guidance identity"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(GuidanceCatalog { workspace, revision: stored.revision, identities })
}

/// Replays one catalog identity, requiring tombstone dominance over a forgotten slot.
pub(super) fn replay_guidance(
    workspace: WorkspaceId,
    identity: ControlOperationId,
    guidance: &DurableStateRecord,
    tombstone: Option<&DurableStateRecord>,
) -> Result<WorkbenchMemoryRow, Error> {
    let key = guidance_key(workspace, identity);
    validate_observed_key(Some(guidance), GUIDANCE_NAMESPACE, &key)?;
    validate_observed_key(tombstone, GUIDANCE_CONTROL_NAMESPACE, &key)?;
    let slot: StoredSlot = decode(guidance.bytes())?;
    slot.validate_identity(workspace, identity, guidance.revision())?;
    match slot.state {
        StoredSlotState::Active { .. } => {
            if tombstone.is_some() {
                return Err(Error::Corrupt("active guidance has a tombstone"));
            }
            Ok(WorkbenchMemoryRow::Active(slot.into_active()?))
        }
        StoredSlotState::Forgotten { tombstone_digest } => {
            let record = tombstone.ok_or(Error::Corrupt("forgotten guidance tombstone missing"))?;
            if peritus_codec::sha256(record.bytes()).into_bytes() != tombstone_digest {
                return Err(Error::Corrupt("guidance tombstone digest mismatch"));
            }
            let stored: StoredTombstone = decode(record.bytes())?;
            let tombstone = stored.into_public()?;
            let successor = tombstone
                .prior()
                .revision()
                .checked_add(1)
                .ok_or(Error::Corrupt("forgotten guidance revision overflow"))?;
            if successor != guidance.revision()
                || tombstone.identity().id() != identity
                || tombstone.identity().workspace() != workspace
                || tombstone.dependency_revision() != slot.dependency_revision
            {
                return Err(Error::Corrupt("guidance tombstone does not dominate slot"));
            }
            Ok(WorkbenchMemoryRow::Forgotten(tombstone))
        }
    }
}

fn require_active(
    workspace: WorkspaceId,
    identity: ControlOperationId,
    observed: GuidanceObservation<'_>,
) -> Result<WorkbenchGuidanceRecord, Error> {
    if observed.tombstone.is_some() {
        return Err(ControlError::NotFound.into());
    }
    let record = observed.guidance.ok_or(ControlError::NotFound)?;
    match replay_guidance(workspace, identity, record, None)? {
        WorkbenchMemoryRow::Active(record) => Ok(record),
        WorkbenchMemoryRow::Forgotten(_) => Err(ControlError::NotFound.into()),
    }
}

pub(super) fn catalog_key(workspace: WorkspaceId) -> Vec<u8> {
    workspace.as_bytes().to_vec()
}

pub(super) fn guidance_key(workspace: WorkspaceId, identity: ControlOperationId) -> Vec<u8> {
    let mut key = Vec::with_capacity(32);
    key.extend_from_slice(workspace.as_bytes());
    key.extend_from_slice(identity.as_bytes());
    key
}

fn validate_observed_key(
    record: Option<&DurableStateRecord>,
    namespace: u16,
    key: &[u8],
) -> Result<(), Error> {
    if record.is_some_and(|record| record.namespace() != namespace || record.key() != key) {
        return Err(Error::Corrupt("guidance state key or namespace mismatch"));
    }
    Ok(())
}

fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, Error> {
    serde_json::to_vec(value).map_err(|_| Error::Corrupt("cannot encode guidance state"))
}

fn decode<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T, Error> {
    if bytes.len() > MAX_STORED_BYTES {
        return Err(Error::Corrupt("guidance state exceeds its bound"));
    }
    let value: T =
        serde_json::from_slice(bytes).map_err(|_| Error::Corrupt("malformed guidance state"))?;
    if encode(&value)? != bytes {
        return Err(Error::Corrupt("noncanonical guidance state"));
    }
    Ok(value)
}

fn map_app<T>(result: Result<T, AppProtocolError>) -> Result<T, Error> {
    result.map_err(|error| {
        Error::Control(match error.code() {
            AppErrorCode::StaleRevision => ControlError::StaleRevision,
            AppErrorCode::LimitExceeded => ControlError::Capacity,
            _ => ControlError::InvalidInput,
        })
    })
}

fn stored_app<T>(result: Result<T, AppProtocolError>) -> Result<T, Error> {
    result.map_err(|_| Error::Corrupt("invalid stored guidance domain value"))
}
