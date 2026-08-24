//! Non-clone zeroizing secret material.

use core::fmt;
use zeroize::Zeroizing;

use crate::SecretError;

const MAX_SECRET_BYTES: usize = 1024 * 1024;

/// Secret bytes retained only in a zeroizing allocation.
pub struct SecretMaterial(Zeroizing<Vec<u8>>);

impl SecretMaterial {
    /// Creates bounded zeroizing secret material.
    ///
    /// # Errors
    /// Rejects empty or over-limit values.
    pub fn new(bytes: Vec<u8>) -> Result<Self, SecretError> {
        if bytes.is_empty() || bytes.len() > MAX_SECRET_BYTES {
            return Err(crate::error::invalid("secret material is empty or exceeds its bound"));
        }
        Ok(Self(Zeroizing::new(bytes)))
    }

    /// Exposes a borrow only for the supplied scoped operation.
    pub fn expose<R>(&self, operation: impl FnOnce(&[u8]) -> R) -> R {
        operation(&self.0)
    }

    /// Returns the byte length without revealing content.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether no bytes are present. Valid material is never empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for SecretMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretMaterial")
            .field("bytes", &"[REDACTED]")
            .field("length", &self.0.len())
            .finish()
    }
}
