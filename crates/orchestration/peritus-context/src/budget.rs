//! Checked context-window reservation and input-token accounting.

use crate::{ContextError, ContextErrorKind};
use vstd::prelude::*;

verus! {

/// Explicit context-window budget with output and protocol reservations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenBudget {
    context_window: u64,
    reserved_output: u64,
    reserved_protocol_overhead: u64,
    usable_input: u64,
}

impl TokenBudget {
    /// Creates a budget using checked addition and subtraction.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the window is zero, addition overflows, or reservations consume
    /// the full context window.
    pub const fn new(
        context_window: u64,
        reserved_output: u64,
        reserved_protocol_overhead: u64,
    ) -> Result<Self, ContextError> {
        if context_window == 0 {
            return Err(ContextError::plain(ContextErrorKind::InvalidTokenBudget));
        }
        let Some(reserved) = reserved_output.checked_add(reserved_protocol_overhead) else {
            return Err(ContextError::plain(ContextErrorKind::ArithmeticOverflow));
        };
        let Some(usable_input) = context_window.checked_sub(reserved) else {
            return Err(ContextError::with_numbers(
                ContextErrorKind::InvalidTokenBudget,
                context_window,
                reserved,
            ));
        };
        if usable_input == 0 {
            return Err(ContextError::with_numbers(
                ContextErrorKind::InvalidTokenBudget,
                context_window,
                reserved,
            ));
        }
        Ok(Self {
            context_window,
            reserved_output,
            reserved_protocol_overhead,
            usable_input,
        })
    }

    /// Total model context capacity.
    #[must_use]
    pub const fn context_window(self) -> u64 { self.context_window }
    /// Reserved model-output capacity.
    #[must_use]
    pub const fn reserved_output(self) -> u64 { self.reserved_output }
    /// Reserved provider-protocol overhead.
    #[must_use]
    pub const fn reserved_protocol_overhead(self) -> u64 { self.reserved_protocol_overhead }
    /// Capacity remaining for selected input nodes.
    #[must_use]
    pub const fn usable_input(self) -> u64 { self.usable_input }

    pub(crate) const fn accounting(self, used_input: u64) -> Result<TokenAccounting, ContextError> {
        let Some(remaining_input) = self.usable_input.checked_sub(used_input) else {
            return Err(ContextError::with_numbers(
                ContextErrorKind::RequiredTokenBudgetExceeded,
                self.usable_input,
                used_input,
            ));
        };
        Ok(TokenAccounting {
            context_window: self.context_window,
            reserved_output: self.reserved_output,
            reserved_protocol_overhead: self.reserved_protocol_overhead,
            usable_input: self.usable_input,
            used_input,
            remaining_input,
        })
    }
}

/// Exact accounting attached to selection and render plans.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenAccounting {
    context_window: u64,
    reserved_output: u64,
    reserved_protocol_overhead: u64,
    usable_input: u64,
    used_input: u64,
    remaining_input: u64,
}

impl TokenAccounting {
    /// Total model context capacity.
    #[must_use]
    pub const fn context_window(self) -> (result: u64)
        ensures result as int == self.spec_context_window(),
    {
        self.context_window
    }
    /// Reserved model-output capacity.
    #[must_use]
    pub const fn reserved_output(self) -> (result: u64)
        ensures result as int == self.spec_reserved_output(),
    {
        self.reserved_output
    }
    /// Reserved provider-protocol overhead.
    #[must_use]
    pub const fn reserved_protocol_overhead(self) -> (result: u64)
        ensures result as int == self.spec_reserved_protocol_overhead(),
    {
        self.reserved_protocol_overhead
    }
    /// Input capacity after reservations.
    #[must_use]
    pub const fn usable_input(self) -> (result: u64)
        ensures result as int == self.spec_usable_input(),
    {
        self.usable_input
    }
    /// Input tokens selected by the planner.
    #[must_use]
    pub const fn used_input(self) -> (result: u64)
        ensures result as int == self.spec_used_input(),
    {
        self.used_input
    }
    /// Unused input capacity.
    #[must_use]
    pub const fn remaining_input(self) -> (result: u64)
        ensures result as int == self.spec_remaining_input(),
    {
        self.remaining_input
    }

    /// Mathematical context-window capacity.
    pub closed spec fn spec_context_window(self) -> int { self.context_window as int }
    /// Mathematical output reservation.
    pub closed spec fn spec_reserved_output(self) -> int { self.reserved_output as int }
    /// Mathematical protocol-overhead reservation.
    pub closed spec fn spec_reserved_protocol_overhead(self) -> int {
        self.reserved_protocol_overhead as int
    }
    /// Mathematical usable-input capacity.
    pub closed spec fn spec_usable_input(self) -> int { self.usable_input as int }
    /// Mathematical selected-input use.
    pub closed spec fn spec_used_input(self) -> int { self.used_input as int }
    /// Mathematical remaining-input capacity.
    pub closed spec fn spec_remaining_input(self) -> int { self.remaining_input as int }

    /// Exact reservation, use, and remaining-capacity invariant.
    pub open spec fn spec_is_bounded(self) -> bool {
        self.spec_reserved_output() <= self.spec_context_window()
            && self.spec_reserved_protocol_overhead()
                <= self.spec_context_window() - self.spec_reserved_output()
            && self.spec_used_input()
                <= self.spec_context_window()
                    - self.spec_reserved_output()
                    - self.spec_reserved_protocol_overhead()
            && self.spec_used_input() <= self.spec_usable_input()
            && self.spec_remaining_input()
                == self.spec_usable_input() - self.spec_used_input()
    }
}

} // verus!
