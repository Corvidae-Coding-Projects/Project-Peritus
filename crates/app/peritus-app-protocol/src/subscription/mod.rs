//! Bounded at-least-once event subscription values and state transitions.

mod error;
mod frame;
mod state;
mod transitions;
mod verified;

pub use error::{SubscriptionError, SubscriptionErrorKind};
pub use frame::RegisteredEventFrame;
pub use state::{
    Acknowledgement, CancellationDisposition, Delivery, DeliveryAdmission, EventCursor,
    PauseReason, SubscriptionCancellation, SubscriptionCancellationSource, SubscriptionFilter,
    SubscriptionGap, SubscriptionPhase, SubscriptionState,
};
pub use verified::{acknowledgement_is_legal, cursor_advances, delivery_window_is_safe};
