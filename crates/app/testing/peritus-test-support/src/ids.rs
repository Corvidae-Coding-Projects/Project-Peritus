//! Deterministic counter-backed identifier bytes.

use peritus_types::IdentifierError;
use std::error::Error;
use std::fmt;
use std::num::NonZeroU64;

/// Failure to issue or construct a deterministic identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IdSourceError {
    /// Every counter value through [`u64::MAX`] has already been issued.
    Exhausted,
    /// A caller-provided nominal constructor rejected reserved bytes.
    IdentifierRejected(IdentifierError),
}

impl IdSourceError {
    /// Returns a stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Exhausted => "PERITUS-TEST-ID-001",
            Self::IdentifierRejected(_) => "PERITUS-TEST-ID-002",
        }
    }
}

impl fmt::Display for IdSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exhausted => formatter.write_str("deterministic identifier source is exhausted"),
            Self::IdentifierRejected(error) => {
                write!(formatter, "nominal identifier rejected deterministic bytes: {error:?}")
            }
        }
    }
}

impl Error for IdSourceError {}

/// A non-cloneable deterministic sequence of nonzero 16-byte patterns.
///
/// Patterns are unique within one source until exhaustion. Sources created with the same namespace
/// intentionally replay the same sequence; callers obtain disjoint sequences by selecting distinct
/// namespaces. Bytes are the exact eight-byte namespace followed by a big-endian nonzero counter.
#[derive(Debug)]
pub struct DeterministicIdSource {
    namespace: [u8; 8],
    next: Option<NonZeroU64>,
    issued: u64,
}

impl DeterministicIdSource {
    /// Creates a source whose first counter is one.
    #[must_use]
    pub const fn new(namespace: [u8; 8]) -> Self {
        Self { namespace, next: NonZeroU64::new(1), issued: 0 }
    }

    /// Creates a source at an exact nonzero counter.
    #[must_use]
    pub const fn starting_at(namespace: [u8; 8], counter: NonZeroU64) -> Self {
        Self { namespace, next: Some(counter), issued: 0 }
    }

    /// Returns the next byte pattern without reserving it.
    ///
    /// # Errors
    ///
    /// Returns [`IdSourceError::Exhausted`] after the maximum counter was issued.
    pub fn peek_bytes(&self) -> Result<[u8; 16], IdSourceError> {
        self.next.map_or(Err(IdSourceError::Exhausted), |counter| Ok(self.encode(counter)))
    }

    /// Reserves and returns the next byte pattern.
    ///
    /// # Errors
    ///
    /// Returns [`IdSourceError::Exhausted`] after the maximum counter was issued.
    pub fn next_bytes(&mut self) -> Result<[u8; 16], IdSourceError> {
        let counter = self.next.ok_or(IdSourceError::Exhausted)?;
        let bytes = self.encode(counter);
        self.next = counter.get().checked_add(1).and_then(NonZeroU64::new);
        self.issued = self.issued.checked_add(1).ok_or(IdSourceError::Exhausted)?;
        Ok(bytes)
    }

    /// Reserves bytes and passes them to a nominal A1 identifier constructor.
    ///
    /// Rejected bytes remain reserved so a failed construction cannot cause identifier reuse.
    ///
    /// # Errors
    ///
    /// Returns [`IdSourceError::Exhausted`] when no bytes remain or
    /// [`IdSourceError::IdentifierRejected`] when `constructor` rejects the reserved bytes.
    pub fn next<I>(
        &mut self,
        constructor: impl FnOnce([u8; 16]) -> Result<I, IdentifierError>,
    ) -> Result<I, IdSourceError> {
        let bytes = self.next_bytes()?;
        constructor(bytes).map_err(IdSourceError::IdentifierRejected)
    }

    /// Returns how many byte patterns this source has reserved.
    #[must_use]
    pub const fn issued(&self) -> u64 {
        self.issued
    }

    /// Returns the exact namespace prefix.
    #[must_use]
    pub const fn namespace(&self) -> &[u8; 8] {
        &self.namespace
    }

    fn encode(&self, counter: NonZeroU64) -> [u8; 16] {
        let mut bytes = [0_u8; 16];
        bytes[..8].copy_from_slice(&self.namespace);
        bytes[8..].copy_from_slice(&counter.get().to_be_bytes());
        bytes
    }
}
