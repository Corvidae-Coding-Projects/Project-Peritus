//! Checked context-window reservation and input-token accounting.

use crate::{ContextError, ContextErrorKind};

#[doc = " Explicit context-window budget with output and protocol reservations."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenBudget {
    context_window: u64,
    reserved_output: u64,
    reserved_protocol_overhead: u64,
    usable_input: u64,
}
impl TokenBudget {
    #[doc = " Creates a budget using checked addition and subtraction."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed error when the window is zero, addition overflows, or reservations consume"]
    #[doc = " the full context window."]
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
        Ok(Self { context_window, reserved_output, reserved_protocol_overhead, usable_input })
    }
    #[doc = " Total model context capacity."]
    #[must_use]
    pub const fn context_window(self) -> u64 {
        self.context_window
    }
    #[doc = " Reserved model-output capacity."]
    #[must_use]
    pub const fn reserved_output(self) -> u64 {
        self.reserved_output
    }
    #[doc = " Reserved provider-protocol overhead."]
    #[must_use]
    pub const fn reserved_protocol_overhead(self) -> u64 {
        self.reserved_protocol_overhead
    }
    #[doc = " Capacity remaining for selected input nodes."]
    #[must_use]
    pub const fn usable_input(self) -> u64 {
        self.usable_input
    }
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
#[doc = " Exact accounting attached to selection and render plans."]
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
    #[doc = " Total model context capacity."]
    #[must_use]
    pub const fn context_window(self) -> u64 {
        self.context_window
    }
    #[doc = " Reserved model-output capacity."]
    #[must_use]
    pub const fn reserved_output(self) -> u64 {
        self.reserved_output
    }
    #[doc = " Reserved provider-protocol overhead."]
    #[must_use]
    pub const fn reserved_protocol_overhead(self) -> u64 {
        self.reserved_protocol_overhead
    }
    #[doc = " Input capacity after reservations."]
    #[must_use]
    pub const fn usable_input(self) -> u64 {
        self.usable_input
    }
    #[doc = " Input tokens selected by the planner."]
    #[must_use]
    pub const fn used_input(self) -> u64 {
        self.used_input
    }
    #[doc = " Unused input capacity."]
    #[must_use]
    pub const fn remaining_input(self) -> u64 {
        self.remaining_input
    }
}
