//! Task-local, non-authoritative working state and deterministic reducers.
//!
//! Hosts supply verified artifact locators and already redacted content. This module performs
//! no I/O, executes no operations, and never promotes an entry into an approved memory record.

use vstd::prelude::*;

verus! {
mod binding;
mod delta;
mod delta_model;
mod delta_reducer;
mod entry;
mod error;
mod evidence;
mod event;
mod invalidation;
mod invalidation_environment;
mod invalidation_model;
mod limits;
mod protocol;
mod state;
mod state_access;
mod state_model;
mod state_revision;
mod staleness;
mod validation;
mod validity;

pub use binding::WorkingBinding;
pub use delta::{WorkingDelta, apply_working_delta};
pub use entry::{WorkingEntry, WorkingEntryKind, WorkingEntryStatus, WorkingLinks};
pub use error::WorkingError;
pub use evidence::{ObservationId, ObservationKind, ObservationSource};
pub use event::{
    WorkingEvent, WorkingReplayFrontier, WorkingReplayPageError, apply_working_event,
    replay_working_event_page, replay_working_events,
};
pub use limits::WorkingLimits;
pub use protocol::{PendingOperationState, WorkingPendingOperation, WorkingProtocol, WorkingProtocolUpdate, apply_working_protocol};
pub use state::{WorkingState, ingest_working_observation, refresh_working_state};
pub use validity::{WorkingEnvironment, WorkingFileDigest, WorkingValidity};
}

#[cfg(not(verus_only))]
mod wire;
#[cfg(not(verus_only))]
pub use wire::{
    EncodedWorkingStateDescriptorPage, EncodedWorkingStatePage, EncodedWorkingStatePart,
    EncodedWorkingStateSnapshot, ReusableWorkingStateHistory, WorkingCodecError,
    WorkingStateArtifact, WorkingStatePageKind, WorkingStatePageReference,
    WorkingStateReadError, WorkingStateWriteError, decode_paged_working_state,
    decode_paged_working_state_from, decode_paged_working_state_with_history_from,
    decode_working_event, decode_working_state, decode_working_state_core,
    encode_paged_working_state, encode_paged_working_state_reusing_with,
    encode_paged_working_state_with, encode_working_event, encode_working_state,
    encode_working_state_core,
};

#[cfg(not(verus_only))]
mod selection;
#[cfg(not(verus_only))]
pub use selection::{
    WorkingRenderView, WorkingSelectionReconciliation, render_working_state,
    render_working_state_with_headroom,
};
