//! Algorithm-tagged Git object identifiers.

use core::fmt;

use crate::{ErrorKind, GitError, Operation, RecoveryClass};

/// Git object format reported by the repository.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ObjectFormat {
    /// The traditional 160-bit Git object format.
    Sha1,
    /// The 256-bit Git object format.
    Sha256,
}

impl ObjectFormat {
    /// Parses Git's canonical object-format name.
    ///
    /// # Errors
    ///
    /// Rejects object formats other than canonical `sha1` and `sha256`.
    pub fn parse(value: &str, operation: Operation) -> Result<Self, GitError> {
        match value {
            "sha1" => Ok(Self::Sha1),
            "sha256" => Ok(Self::Sha256),
            _ => Err(GitError::new(
                ErrorKind::UnsupportedRepository,
                operation,
                RecoveryClass::CorrectRequest,
                "Git reported an unsupported object format",
            )),
        }
    }

    /// Returns Git's canonical object-format name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
        }
    }

    /// Returns the exact binary identifier length.
    #[must_use]
    pub const fn byte_len(self) -> usize {
        match self {
            Self::Sha1 => 20,
            Self::Sha256 => 32,
        }
    }

    /// Returns the exact lowercase hexadecimal identifier length.
    #[must_use]
    pub const fn hex_len(self) -> usize {
        self.byte_len() * 2
    }
}

/// Exact Git object identity retaining its hash algorithm.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObjectId {
    format: ObjectFormat,
    bytes: [u8; 32],
}

impl ObjectId {
    /// Parses a canonical lowercase Git object ID for `format`.
    ///
    /// # Errors
    ///
    /// Returns a typed object mismatch for the wrong length or non-lowercase-hex bytes.
    pub fn parse(
        format: ObjectFormat,
        value: &str,
        operation: Operation,
    ) -> Result<Self, GitError> {
        if !crate::verified::supported_object_hex_length(value.len())
            || value.len() != format.hex_len()
            || !value.bytes().all(is_lower_hex)
        {
            return Err(GitError::new(
                ErrorKind::ObjectMismatch,
                operation,
                RecoveryClass::Reobserve,
                "Git returned a malformed object identifier",
            ));
        }
        let mut bytes = [0_u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = decode_nibble(pair[0]) * 16 + decode_nibble(pair[1]);
        }
        Ok(Self { format, bytes })
    }

    /// Returns the identifier's exact object format.
    #[must_use]
    pub const fn format(self) -> ObjectFormat {
        self.format
    }

    /// Returns exact binary identifier bytes without unused storage.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.format.byte_len()]
    }

    /// Returns canonical lowercase hexadecimal text.
    #[must_use]
    pub fn to_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut result = String::with_capacity(self.format.hex_len());
        for &byte in self.as_bytes() {
            result.push(char::from(HEX[usize::from(byte >> 4)]));
            result.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        result
    }

    pub(crate) fn zero_hex(format: ObjectFormat) -> String {
        "0".repeat(format.hex_len())
    }
}

impl fmt::Debug for ObjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("ObjectId").field(&self.to_hex()).finish()
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

/// An object ID verified by Git to denote a commit.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CommitId(ObjectId);

impl CommitId {
    pub(crate) const fn checked(value: ObjectId) -> Self {
        Self(value)
    }

    /// Returns the underlying algorithm-tagged object identity.
    #[must_use]
    pub const fn object_id(self) -> ObjectId {
        self.0
    }
}

impl fmt::Display for CommitId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, formatter)
    }
}

/// An object ID verified by Git to denote a tree.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TreeId(ObjectId);

impl TreeId {
    pub(crate) const fn checked(value: ObjectId) -> Self {
        Self(value)
    }

    /// Returns the underlying algorithm-tagged object identity.
    #[must_use]
    pub const fn object_id(self) -> ObjectId {
        self.0
    }
}

impl fmt::Display for TreeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, formatter)
    }
}

const fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (byte >= b'a' && byte <= b'f')
}

const fn decode_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{ObjectFormat, ObjectId};
    use crate::Operation;

    #[test]
    fn accepts_exact_sha1_and_sha256() {
        let sha1 =
            ObjectId::parse(ObjectFormat::Sha1, &"ab".repeat(20), Operation::Status).expect("sha1");
        let sha256 = ObjectId::parse(ObjectFormat::Sha256, &"cd".repeat(32), Operation::Status)
            .expect("sha256");
        assert_eq!(sha1.to_hex(), "ab".repeat(20));
        assert_eq!(sha256.to_hex(), "cd".repeat(32));
    }

    #[test]
    fn rejects_wrong_algorithm_length_and_uppercase() {
        assert!(ObjectId::parse(ObjectFormat::Sha1, &"ab".repeat(32), Operation::Status).is_err());
        assert!(
            ObjectId::parse(ObjectFormat::Sha256, &"AB".repeat(32), Operation::Status).is_err()
        );
    }
}
