//! Exact workspace-bound patch plans and stable identities.

use std::fmt;

use peritus_types::{Generation, RevisionNumber, Sha256Digest, WorkspaceId};

use crate::{PatchOperation, PatchSet};

/// SHA-256 identity of one canonical patch set.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PatchIdentity(Sha256Digest);

impl PatchIdentity {
    pub(crate) const fn new(digest: Sha256Digest) -> Self {
        Self(digest)
    }

    /// Returns the exact identity digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.0
    }

    /// Borrows the exact 32 identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }

    /// Returns the lowercase hexadecimal representation used for transaction names.
    #[must_use]
    pub fn to_hex(self) -> String {
        use std::fmt::Write as _;
        let mut text = String::with_capacity(64);
        for byte in self.0.as_bytes() {
            write!(&mut text, "{byte:02x}").expect("writing into String cannot fail");
        }
        text
    }
}

impl fmt::Display for PatchIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

/// Canonical patch data proven to match one observed workspace version.
///
/// Plans cannot be directly constructed; call [`PatchSet::plan`] after inspection.
///
/// ```compile_fail
/// use peritus_patch::PatchPlan;
/// let _unchecked = PatchPlan {};
/// ```
#[derive(Debug)]
pub struct PatchPlan {
    pub(crate) patch: PatchSet,
}

impl PatchPlan {
    /// Returns the canonical patch identity.
    #[must_use]
    pub const fn identity(&self) -> PatchIdentity {
        self.patch.identity()
    }

    /// Returns the bound workspace identity.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.patch.workspace_id()
    }

    /// Returns the exactly matched generation.
    #[must_use]
    pub const fn expected_generation(&self) -> Generation {
        self.patch.expected_generation()
    }

    /// Returns the exactly matched revision.
    #[must_use]
    pub const fn expected_revision(&self) -> RevisionNumber {
        self.patch.expected_revision()
    }

    /// Borrows canonical path-sorted operations.
    #[must_use]
    pub fn operations(&self) -> &[PatchOperation] {
        self.patch.operations()
    }
}
