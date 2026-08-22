//! Typed fault planning, control, and verification failures.

use super::FaultExpectation;
use std::error::Error;
use std::fmt;

/// Failure to validate a fault point or label.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FaultNameError {
    /// The name was empty.
    Empty,
    /// The name exceeded 128 ASCII bytes.
    TooLong,
    /// A dot created an empty segment.
    EmptySegment,
    /// A segment did not start with an ASCII lowercase letter.
    InvalidSegmentStart,
    /// A byte was outside lowercase ASCII letters, digits, dots, and hyphens.
    InvalidCharacter,
}

impl FaultNameError {
    /// Returns a stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Empty => "PERITUS-TEST-FAULT-NAME-001",
            Self::TooLong => "PERITUS-TEST-FAULT-NAME-002",
            Self::EmptySegment => "PERITUS-TEST-FAULT-NAME-003",
            Self::InvalidSegmentStart => "PERITUS-TEST-FAULT-NAME-004",
            Self::InvalidCharacter => "PERITUS-TEST-FAULT-NAME-005",
        }
    }
}

impl fmt::Display for FaultNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid fault name ({})", self.code())
    }
}

impl Error for FaultNameError {}

/// Failure to construct an unambiguous fault plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FaultPlanError {
    /// The exact point and occurrence already had a scheduled label.
    Duplicate {
        /// The duplicate expectation.
        expectation: FaultExpectation,
    },
}

impl FaultPlanError {
    /// Returns a stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        "PERITUS-TEST-FAULT-PLAN-001"
    }
}

impl fmt::Display for FaultPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self::Duplicate { expectation } = self;
        write!(
            formatter,
            "fault {} occurrence {} is scheduled more than once",
            expectation.point().as_str(),
            expectation.occurrence()
        )
    }
}

impl Error for FaultPlanError {}

/// Failure to update or inspect shared injector state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FaultControlError {
    /// A one-based per-point call counter overflowed.
    OccurrenceOverflow,
    /// The shared state lock was poisoned by an unexpected panic.
    Poisoned,
}

impl FaultControlError {
    /// Returns a stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::OccurrenceOverflow => "PERITUS-TEST-FAULT-CONTROL-001",
            Self::Poisoned => "PERITUS-TEST-FAULT-CONTROL-002",
        }
    }
}

impl fmt::Display for FaultControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "fault injector control failed ({})", self.code())
    }
}

impl Error for FaultControlError {}

/// Failure to prove that every scheduled fault was exercised.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FaultVerificationError {
    /// The named scheduled faults were never activated.
    Missed {
        /// Deterministically ordered missed expectations.
        expectations: Vec<FaultExpectation>,
    },
    /// Shared state inspection failed.
    Control(FaultControlError),
}

impl FaultVerificationError {
    /// Returns a stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Missed { .. } => "PERITUS-TEST-FAULT-VERIFY-001",
            Self::Control(_) => "PERITUS-TEST-FAULT-VERIFY-002",
        }
    }
}

impl fmt::Display for FaultVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missed { expectations } => {
                write!(formatter, "{} scheduled faults were not activated", expectations.len())
            }
            Self::Control(error) => write!(formatter, "fault verification failed: {error}"),
        }
    }
}

impl Error for FaultVerificationError {}
