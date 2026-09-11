//! Task-local, non-authoritative working state and deterministic reducers.
//!
//! Hosts supply verified artifact locators and already redacted content. This module performs
//! no I/O, executes no operations, and never promotes an entry into an approved memory record.

use vstd::prelude::*;

verus! {
mod binding;
mod delta;
mod entry;
mod error;
mod evidence;
mod event;
mod limits;
mod protocol;
mod state;
mod validation;
mod validity;

pub use binding::WorkingBinding;
pub use delta::{WorkingDelta, apply_working_delta};
pub use entry::{WorkingEntry, WorkingEntryKind, WorkingEntryStatus, WorkingLinks};
pub use error::WorkingError;
pub use evidence::{ObservationId, ObservationKind, ObservationSource};
pub use event::{WorkingEvent, apply_working_event, replay_working_events};
pub use limits::WorkingLimits;
pub use protocol::{PendingOperationState, WorkingPendingOperation, WorkingProtocol, WorkingProtocolUpdate, apply_working_protocol};
pub use state::{WorkingState, ingest_working_observation, refresh_working_state};
pub use validity::{WorkingEnvironment, WorkingFileDigest, WorkingValidity};
}

#[cfg(not(verus_only))]
mod wire;
#[cfg(not(verus_only))]
pub use wire::{WorkingCodecError, decode_working_event, decode_working_state, encode_working_event, encode_working_state};

#[cfg(not(verus_only))]
mod selection;
#[cfg(not(verus_only))]
pub use selection::{WorkingRenderView, render_working_state, render_working_state_with_headroom};
