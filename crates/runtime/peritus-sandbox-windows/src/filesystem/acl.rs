//! Canonical ACL entries and exact save/apply/restore transaction.

use peritus_sandbox::{FileOperation, PathScope, RuleEffect};
use peritus_types::Sha256Digest;

use super::WindowsPath;
use crate::{WindowsError, WindowsOperation, error};

mod reversal;

pub use reversal::AclTransaction;

/// Windows access-mask projection preserving all seven C2 filesystem operations.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AclAccess(u32);

impl AclAccess {
    /// Creates an empty access set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Returns every C2 filesystem operation.
    #[must_use]
    pub const fn all() -> Self {
        Self(0x7f)
    }

    /// Adds one operation.
    pub const fn insert(&mut self, operation: FileOperation) {
        self.0 |= operation_bit(operation);
    }

    /// Reports one operation.
    #[must_use]
    pub const fn contains(self, operation: FileOperation) -> bool {
        self.0 & operation_bit(operation) != 0
    }

    /// Returns the stable seven-bit representation.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum AclTargetDisposition {
    Existing,
    CreateDenyDirectory,
}

/// One exact path/effect/scope ACL projection.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct AclEntry {
    effect: RuleEffect,
    path: WindowsPath,
    scope: PathScope,
    access: AclAccess,
    authority_root: WindowsPath,
    target: AclTargetDisposition,
}

impl AclEntry {
    /// Creates one nonempty exact ACL entry.
    ///
    /// # Errors
    /// Rejects an empty access set.
    pub fn new(
        effect: RuleEffect,
        path: WindowsPath,
        scope: PathScope,
        access: AclAccess,
    ) -> Result<Self, WindowsError> {
        Self::new_authorized(effect, path.clone(), scope, access, path, false)
    }

    pub(super) fn new_authorized(
        effect: RuleEffect,
        path: WindowsPath,
        scope: PathScope,
        mut access: AclAccess,
        authority_root: WindowsPath,
        create_deny_directory: bool,
    ) -> Result<Self, WindowsError> {
        if access.bits() == 0 {
            return Err(error::invalid(WindowsOperation::CompileAcl, "ACL entry is empty"));
        }
        if !authority_root.contains(&path) {
            return Err(error::invalid(
                WindowsOperation::CompileAcl,
                "ACL target is outside its exact authorized root",
            ));
        }
        let target = if create_deny_directory {
            if effect != RuleEffect::Deny || scope != PathScope::Descendants {
                return Err(error::invalid(
                    WindowsOperation::CompileAcl,
                    "only descendant denies may create a temporary ACL anchor",
                ));
            }
            access = access.union(AclAccess(operation_bit(FileOperation::Remove)));
            AclTargetDisposition::CreateDenyDirectory
        } else {
            AclTargetDisposition::Existing
        };
        Ok(Self { effect, path, scope, access, authority_root, target })
    }

    /// Returns allow or deny effect.
    #[must_use]
    pub const fn effect(&self) -> RuleEffect {
        self.effect
    }

    /// Returns normalized path.
    #[must_use]
    pub const fn path(&self) -> &WindowsPath {
        &self.path
    }

    /// Returns exact or inheritable descendant scope.
    #[must_use]
    pub const fn scope(&self) -> PathScope {
        self.scope
    }

    /// Returns operation-preserving access.
    #[must_use]
    pub const fn access(&self) -> AclAccess {
        self.access
    }

    /// Returns the exact configured root that supplies this target's volume authority.
    #[must_use]
    pub const fn authority_root(&self) -> &WindowsPath {
        &self.authority_root
    }

    /// Reports whether an absent descendant-deny target may be created as a temporary directory.
    #[must_use]
    pub const fn creates_deny_directory(&self) -> bool {
        matches!(self.target, AclTargetDisposition::CreateDenyDirectory)
    }

    pub(super) fn subtract_inherited_deny(&mut self, deny: &Self) {
        if deny.effect == RuleEffect::Deny
            && deny.scope == PathScope::Descendants
            && deny.path.contains(&self.path)
            && !deny.path.same_native_path(&self.path)
        {
            self.access = self.access.difference(deny.access);
        }
    }

    fn canonical_cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.path
            .stable_native_cmp(&other.path)
            .then_with(|| self.effect.cmp(&other.effect))
            .then_with(|| self.scope.cmp(&other.scope))
            .then_with(|| self.authority_root.stable_native_cmp(&other.authority_root))
            .then_with(|| self.target.cmp(&other.target))
            .then_with(|| self.access.cmp(&other.access))
    }
}

/// Deterministic temporary ACL mutation plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AclPlan {
    principal_sid: String,
    entries: Vec<AclEntry>,
    digest: Sha256Digest,
}

impl AclPlan {
    pub(crate) fn new(
        principal_sid: &str,
        mut entries: Vec<AclEntry>,
    ) -> Result<Self, WindowsError> {
        entries.sort_by(AclEntry::canonical_cmp);
        if entries.windows(2).any(|pair| {
            pair[0].path.same_native_path(&pair[1].path) && pair[0].path != pair[1].path
        }) {
            return Err(error::invalid(
                WindowsOperation::CompileAcl,
                "ACL entries contain a case-fold path alias",
            ));
        }
        let mut merged: Vec<AclEntry> = Vec::new();
        merged.try_reserve_exact(entries.len()).map_err(|_| {
            error::invalid(WindowsOperation::CompileAcl, "ACL merge capacity is unavailable")
        })?;
        for entry in entries {
            if let Some(previous) = merged.last_mut()
                && previous.effect == entry.effect
                && previous.path == entry.path
                && previous.scope == entry.scope
                && previous.authority_root == entry.authority_root
                && previous.target == entry.target
            {
                previous.access = previous.access.union(entry.access);
            } else {
                merged.push(entry);
            }
        }
        let digest = digest_plan(principal_sid, &merged);
        Ok(Self { principal_sid: principal_sid.to_owned(), entries: merged, digest })
    }

    /// Returns exact ACL principal.
    #[must_use]
    pub fn principal_sid(&self) -> &str {
        &self.principal_sid
    }

    /// Returns canonical entries.
    #[must_use]
    pub fn entries(&self) -> &[AclEntry] {
        &self.entries
    }

    /// Returns canonical plan digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Creates an inert transaction for platform-independent planning tests.
    #[must_use]
    pub const fn planned(&self) -> AclTransaction {
        AclTransaction::planned(self.digest)
    }

    /// Saves and applies each exact ACL entry on Windows.
    ///
    /// Reserves every affected volume across cooperating Peritus processes before reading
    /// pristine descriptors. A busy volume returns `Replan` without preparing files. The
    /// movable owner holds reservations through verified restoration and backup removal.
    /// Failed final-owner cleanup quarantines its volumes until process exit. Process death
    /// releases reservations without restoring ACLs; durable crash recovery is not provided.
    /// Out-of-band ACL writers and namespace changes during initial inventory must be quiescent.
    ///
    /// # Errors
    /// Save or mutation failures attempt rollback. Incomplete cleanup ownership stays in
    /// `WindowsError` for `retry_cleanup`; losing its final owner quarantines the live volume.
    #[cfg(target_os = "windows")]
    pub fn install(&self, backup_root: &std::path::Path) -> Result<AclTransaction, WindowsError> {
        AclTransaction::install(self, backup_root)
    }
}

fn digest_plan(principal: &str, entries: &[AclEntry]) -> Sha256Digest {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"PERITUS-WINDOWS-ACL-V2\0");
    bytes.extend_from_slice(principal.as_bytes());
    for entry in entries {
        bytes.push(match entry.effect {
            RuleEffect::Allow => 1,
            RuleEffect::Deny => 2,
        });
        bytes.push(match entry.scope {
            PathScope::Exact => 1,
            PathScope::Descendants => 2,
        });
        bytes.extend_from_slice(entry.path.digest().as_bytes());
        bytes.extend_from_slice(entry.authority_root.digest().as_bytes());
        bytes.push(match entry.target {
            AclTargetDisposition::Existing => 1,
            AclTargetDisposition::CreateDenyDirectory => 2,
        });
        bytes.extend_from_slice(&entry.access.bits().to_be_bytes());
    }
    peritus_codec::sha256(&bytes)
}

const fn operation_bit(operation: FileOperation) -> u32 {
    match operation {
        FileOperation::Discover => 1 << 0,
        FileOperation::Metadata => 1 << 1,
        FileOperation::Read => 1 << 2,
        FileOperation::Execute => 1 << 3,
        FileOperation::Create => 1 << 4,
        FileOperation::Write => 1 << 5,
        FileOperation::Remove => 1 << 6,
    }
}
