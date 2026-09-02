//! Collision-free identities scoped to one disposable qualification subject.

use peritus_types::IdentifierError;

use crate::SubjectError;

/// Monotonic identity source for a single isolated subject.
pub struct IdentitySource {
    next: u128,
}

impl IdentitySource {
    pub const fn new(namespace: u64) -> Self {
        Self { next: (namespace as u128) << 64 | 1 }
    }

    pub fn next<T>(
        &mut self,
        constructor: impl FnOnce([u8; 16]) -> Result<T, IdentifierError>,
    ) -> Result<T, SubjectError> {
        let bytes = self.next.to_be_bytes();
        self.next = self.next.checked_add(1).ok_or(SubjectError::IdentityExhausted)?;
        constructor(bytes).map_err(SubjectError::Identifier)
    }

    pub fn key(&mut self) -> Result<Vec<u8>, SubjectError> {
        self.bytes().map(|bytes| bytes.to_vec())
    }

    pub fn bytes(&mut self) -> Result<[u8; 16], SubjectError> {
        let bytes = self.next.to_be_bytes();
        self.next = self.next.checked_add(1).ok_or(SubjectError::IdentityExhausted)?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use peritus_types::RunId;

    use super::IdentitySource;

    #[test]
    fn identities_are_nonzero_and_distinct() {
        let mut source = IdentitySource::new(7);
        let first = source.next(RunId::new).expect("first identity");
        let second = source.next(RunId::new).expect("second identity");
        assert_ne!(first, second);
        assert_ne!(first.as_bytes(), &[0; 16]);
    }
}
