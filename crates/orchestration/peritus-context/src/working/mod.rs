//! Task-local, non-authoritative working state and deterministic reducers.
//!
//! Hosts supply verified artifact locators and already redacted content. This module performs
//! no I/O, executes no operations, and never promotes an entry into an approved memory record.

mod binding;
mod delta;
mod delta_model;
mod delta_reducer;
mod entry;
mod error;
mod event;
mod evidence;
mod host;
mod invalidation;
pub use host::WorkingHostBinding;
mod invalidation_environment;
mod invalidation_model;
mod limits;
mod observation_prefix;
pub use observation_prefix::WorkingObservationPage;
mod protocol;
mod staleness;
mod state;
mod state_access;
mod state_model;
mod state_revision;
mod validation;
mod validity;
pub use binding::WorkingBinding;
pub use delta::{WorkingDelta, apply_working_delta};
pub use entry::{WorkingEntry, WorkingEntryKind, WorkingEntryStatus, WorkingLinks};
pub use error::WorkingError;
pub use event::{WorkingEvent, apply_working_event, replay_working_events};
pub use evidence::{ObservationId, ObservationKind, ObservationSource};
pub use limits::WorkingLimits;
pub use protocol::{
    PendingOperationState, WorkingPendingOperation, WorkingProtocol, WorkingProtocolUpdate,
    apply_working_protocol,
};
pub use state::{WorkingState, ingest_working_observation, refresh_working_state};
pub use validity::{WorkingEnvironment, WorkingFileDigest, WorkingValidity};

mod wire;

pub use wire::{
    WorkingCodecError, WorkingStatePageDecoder, WorkingStatePages, decode_working_event,
    decode_working_state, encode_working_event, encode_working_state,
};

mod selection;

pub use selection::{
    WorkingRenderView, render_working_state, render_working_state_complete,
    render_working_state_with_headroom,
};
