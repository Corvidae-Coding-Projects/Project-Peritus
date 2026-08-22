//! An explicitly advanced wall and monotonic test clock.

use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// One atomic observation of both test-clock dimensions.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ClockReading {
    wall_time: SystemTime,
    monotonic: Duration,
}

impl ClockReading {
    /// Returns the simulated wall time.
    #[must_use]
    pub const fn wall_time(self) -> SystemTime {
        self.wall_time
    }

    /// Returns elapsed monotonic time since this fake clock was created.
    #[must_use]
    pub const fn monotonic(self) -> Duration {
        self.monotonic
    }
}

/// Identifies the clock dimension whose exact addition overflowed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ClockComponent {
    /// Simulated wall time.
    Wall,
    /// Simulated monotonic elapsed time.
    Monotonic,
}

/// Failure to advance a [`FakeClock`] exactly.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ClockError {
    /// Exact addition overflowed the named clock dimension.
    Overflow {
        /// The dimension that could not represent the requested advance.
        component: ClockComponent,
    },
    /// The shared clock lock was poisoned by an unexpected panic.
    Poisoned,
}

impl ClockError {
    /// Returns a stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Overflow { .. } => "PERITUS-TEST-CLOCK-001",
            Self::Poisoned => "PERITUS-TEST-CLOCK-002",
        }
    }
}

impl fmt::Display for ClockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overflow { component } => {
                write!(formatter, "test clock {component:?} time overflowed")
            }
            Self::Poisoned => formatter.write_str("test clock state was poisoned"),
        }
    }
}

impl Error for ClockError {}

/// A deterministic clock advanced only by explicit calls.
///
/// Clones share one state. Use [`Self::fork`] for an independent clock at the same reading.
#[derive(Clone, Debug)]
pub struct FakeClock {
    state: Arc<Mutex<ClockReading>>,
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new(UNIX_EPOCH)
    }
}

impl FakeClock {
    /// Creates a stopped clock at an exact wall time and zero monotonic elapsed time.
    #[must_use]
    pub fn new(wall_time: SystemTime) -> Self {
        Self::with_reading(wall_time, Duration::ZERO)
    }

    /// Creates a stopped clock at an exact wall and monotonic reading.
    ///
    /// This constructor permits boundary tests to begin near representation limits without
    /// performing an impractical number of advances.
    #[must_use]
    pub fn with_reading(wall_time: SystemTime, monotonic: Duration) -> Self {
        Self { state: Arc::new(Mutex::new(ClockReading { wall_time, monotonic })) }
    }

    /// Reads both dimensions without advancing either one.
    ///
    /// # Errors
    ///
    /// Returns [`ClockError::Poisoned`] after an unexpected panic while the state was locked.
    pub fn reading(&self) -> Result<ClockReading, ClockError> {
        Ok(*self.lock()?)
    }

    /// Advances wall and monotonic time by exactly `elapsed` as one atomic mutation.
    ///
    /// Neither dimension changes if either addition overflows.
    ///
    /// # Errors
    ///
    /// Returns [`ClockError::Overflow`] for an unrepresentable result or
    /// [`ClockError::Poisoned`] for poisoned shared state.
    pub fn advance(&self, elapsed: Duration) -> Result<ClockReading, ClockError> {
        let mut state = self.lock()?;
        let wall_time = state
            .wall_time
            .checked_add(elapsed)
            .ok_or(ClockError::Overflow { component: ClockComponent::Wall })?;
        let monotonic = state
            .monotonic
            .checked_add(elapsed)
            .ok_or(ClockError::Overflow { component: ClockComponent::Monotonic })?;
        let reading = ClockReading { wall_time, monotonic };
        *state = reading;
        drop(state);
        Ok(reading)
    }

    /// Creates an independent stopped clock at the current reading.
    ///
    /// # Errors
    ///
    /// Returns [`ClockError::Poisoned`] for poisoned shared state.
    pub fn fork(&self) -> Result<Self, ClockError> {
        let reading = self.reading()?;
        Ok(Self { state: Arc::new(Mutex::new(reading)) })
    }

    fn lock(&self) -> Result<MutexGuard<'_, ClockReading>, ClockError> {
        self.state.lock().map_err(|_| ClockError::Poisoned)
    }
}
