//! Subscription-specific rejection vocabulary.

use core::fmt;

/// Stable category for a rejected subscription operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SubscriptionErrorKind {
    /// A negotiated or caller-supplied bound is invalid.
    InvalidLimit,
    /// A topic filter or cancellation value is invalid.
    InvalidInput,
    /// A message names another subscription.
    BindingMismatch,
    /// A new delivery is not the exact cursor successor.
    NonContiguousDelivery,
    /// Delivery-attempt or cursor arithmetic overflowed.
    ArithmeticOverflow,
    /// The cumulative acknowledgement regresses.
    AcknowledgementRegression,
    /// The acknowledgement exceeds delivered data.
    AcknowledgementFuture,
    /// The acknowledgement would cross a declared retention gap.
    AcknowledgementAcrossGap,
    /// The negotiated in-flight window is full.
    Backpressured,
    /// A redelivery target is not currently in flight.
    UnknownDelivery,
    /// The requested state transition is not legal.
    IllegalTransition,
    /// A terminal fact conflicts with the retained terminal fact.
    TerminalConflict,
}

/// Typed subscription failure with inert diagnostic context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionError {
    kind: SubscriptionErrorKind,
    detail: &'static str,
}

impl SubscriptionError {
    pub(crate) const fn new(kind: SubscriptionErrorKind, detail: &'static str) -> Self {
        Self { kind, detail }
    }

    /// Returns the stable rejection category.
    #[must_use]
    pub const fn kind(&self) -> SubscriptionErrorKind {
        self.kind
    }

    /// Returns inert diagnostic text.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl fmt::Display for SubscriptionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for SubscriptionError {}

pub(super) const fn reject(kind: SubscriptionErrorKind, detail: &'static str) -> SubscriptionError {
    SubscriptionError::new(kind, detail)
}
