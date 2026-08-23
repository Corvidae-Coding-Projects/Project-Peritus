//! Shared exact accounting and identity lookup primitives.

mod charge_safety;
mod fault;
mod fault_safety;
mod lineage;
mod lineage_charge;
mod lookup;
mod receipts;
mod releases;

pub(in crate::transition) use charge_safety::{
    establish_available_charge_safe, establish_observation_charge_safe,
    establish_reserved_charge_safe,
};
#[cfg(verus_only)]
pub(in crate::transition) use charge_safety::{
    known_release_preserves_charge_safety, later_account_update_preserves_safe_fuel,
    lineage_charge_safe, lineage_charge_safe_fuel,
};
pub(in crate::transition) use fault_safety::establish_fault_lineage_safe;
#[cfg(verus_only)]
pub(in crate::transition) use fault_safety::identity_stability_preserves_fault_safety;

pub(super) use fault::fault_lineage;
pub(super) use lineage::{outstanding, outstanding_validated, require_open_lineage};
pub(super) use lineage_charge::charge_lineage;
pub(super) use lookup::{
    find_account, find_reservation, has_live_work, require_account, require_binding,
    require_reference_binding, require_reservation,
};
pub(super) use receipts::{bound_receipt, receipt};
#[allow(
    unused_imports,
    reason = "the exact checked release primitive remains part of the transition-internal contract"
)]
pub(super) use releases::{
    release_full_reservation, release_observation_charge, release_operation_reservation,
    release_operation_reservation_validated,
};
