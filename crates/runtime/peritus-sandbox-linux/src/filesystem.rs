//! Deterministic mount and Landlock projection with protected-metadata dominance.

use crate::{LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery};
use peritus_sandbox::{CheckedSandboxPlan, FileOperation, PathScope, RuleEffect, SandboxPath};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const ALL_FILE_OPERATIONS: u8 = 0x7f;

/// Native mount operation emitted in deterministic order.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MountAction {
    /// Read-only bind of one existing native path.
    ReadOnlyBind {
        /// Existing native source.
        source: PathBuf,
        /// Exact namespace destination.
        target: PathBuf,
    },
    /// Writable bind of one exact existing native path.
    WritableBind {
        /// Existing native source.
        source: PathBuf,
        /// Exact namespace destination.
        target: PathBuf,
    },
    /// Fresh procfs inside the PID namespace.
    Proc {
        /// Namespace procfs destination.
        target: PathBuf,
    },
    /// Fresh minimal `/dev`.
    Dev {
        /// Namespace device destination.
        target: PathBuf,
    },
    /// Fresh temporary filesystem.
    Tmpfs {
        /// Fresh temporary filesystem destination.
        target: PathBuf,
    },
    /// Empty mount hiding protected metadata. Applied after writable binds.
    Mask {
        /// Protected path to hide with an empty read-only mount.
        target: PathBuf,
    },
}

/// Stable compact Landlock access vocabulary.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct LandlockAccess(u16);

impl LandlockAccess {
    const EXECUTE: u16 = 1 << 0;
    const READ_FILE: u16 = 1 << 1;
    const READ_DIR: u16 = 1 << 2;
    const WRITE_FILE: u16 = 1 << 3;
    const CREATE: u16 = 1 << 4;
    const REMOVE: u16 = 1 << 5;
    const REFER: u16 = 1 << 6;
    const TRUNCATE: u16 = 1 << 7;

    /// Returns the stable bit representation used by the helper protocol.
    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }
    /// Reconstructs a validated bit set.
    ///
    /// # Errors
    /// Rejects unknown access bits.
    pub fn from_bits(bits: u16) -> Result<Self, LinuxError> {
        if bits & !0xff != 0 {
            return Err(filesystem_error("Landlock access contains unknown bits"));
        }
        Ok(Self(bits))
    }
    /// Complete read/execute access used for the explicit host view.
    #[must_use]
    pub const fn host_read_only() -> Self {
        Self(Self::EXECUTE | Self::READ_FILE | Self::READ_DIR)
    }
    const fn insert_operation(&mut self, operation: FileOperation) {
        self.0 |= match operation {
            FileOperation::Discover | FileOperation::Metadata => Self::READ_DIR,
            FileOperation::Read => Self::READ_FILE | Self::READ_DIR,
            FileOperation::Execute => Self::EXECUTE,
            FileOperation::Create => Self::CREATE | Self::REFER,
            FileOperation::Write => Self::WRITE_FILE | Self::TRUNCATE,
            FileOperation::Remove => Self::REMOVE | Self::REFER,
        };
    }
}

/// One path-beneath allow rule installed by the helper.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct LandlockRule {
    path: PathBuf,
    access: LandlockAccess,
}

impl LandlockRule {
    /// Creates a nonempty access rule for an absolute path.
    ///
    /// # Errors
    /// Rejects relative paths or empty access.
    pub fn new(path: PathBuf, access: LandlockAccess) -> Result<Self, LinuxError> {
        if !path.is_absolute() || access.bits() == 0 {
            return Err(filesystem_error("Landlock rule must be absolute and nonempty"));
        }
        Ok(Self { path, access })
    }
    /// Returns the native path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Returns access bits.
    #[must_use]
    pub const fn access(&self) -> LandlockAccess {
        self.access
    }
}

/// Inputs controlling protected metadata projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MountPolicy {
    workspace_root: PathBuf,
    protected_roots: Vec<PathBuf>,
}

impl MountPolicy {
    /// Resolves the workspace and canonicalizes exact protected roots.
    ///
    /// The `.git`, `.peritus`, and `.crosslink` roots are always protected even when omitted.
    ///
    /// # Errors
    /// Rejects an absent/non-directory workspace or protected roots outside it.
    pub fn new(
        workspace_root: &Path,
        additional_protected_roots: Vec<PathBuf>,
    ) -> Result<Self, LinuxError> {
        let workspace_root = fs::canonicalize(workspace_root).map_err(|error| {
            LinuxError::io(LinuxOperation::Project, "resolve workspace root", &error)
        })?;
        if !workspace_root.is_dir() {
            return Err(filesystem_error("workspace root is not a directory"));
        }
        let mut protected_roots = vec![
            workspace_root.join(".git"),
            workspace_root.join(".peritus"),
            workspace_root.join(".crosslink"),
        ];
        for path in additional_protected_roots {
            let absolute = if path.is_absolute() { path } else { workspace_root.join(path) };
            if !absolute.starts_with(&workspace_root) {
                return Err(filesystem_error("protected root is outside the workspace"));
            }
            protected_roots.push(absolute);
        }
        for protected in &mut protected_roots {
            if protected.exists() {
                let resolved = fs::canonicalize(&*protected).map_err(|error| {
                    LinuxError::io(LinuxOperation::Project, "resolve protected root", &error)
                })?;
                if !resolved.starts_with(&workspace_root) {
                    return Err(filesystem_error(
                        "protected root aliases a path outside the workspace",
                    ));
                }
                *protected = resolved;
            }
        }
        protected_roots.sort();
        protected_roots.dedup();
        Ok(Self { workspace_root, protected_roots })
    }
    /// Returns the resolved workspace root.
    #[must_use]
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }
    /// Returns canonical protected metadata roots.
    #[must_use]
    pub fn protected_roots(&self) -> &[PathBuf] {
        &self.protected_roots
    }
}

/// Deterministic namespace mount and second-layer filesystem policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MountPlan {
    actions: Vec<MountAction>,
    landlock_rules: Vec<LandlockRule>,
}

impl MountPlan {
    /// Projects every filesystem contract rule without widening writable access.
    ///
    /// # Errors
    /// Rejects aliases, absent mount sources, exact directory rules that Landlock cannot represent,
    /// and operation-selective denies that cannot be encoded without over-denial.
    #[allow(
        clippy::too_many_lines,
        reason = "the deny-dominant filesystem projection is kept in one auditable ordered pass"
    )]
    pub fn project(plan: &CheckedSandboxPlan, policy: &MountPolicy) -> Result<Self, LinuxError> {
        let mut access_by_path = BTreeMap::<PathBuf, LandlockAccess>::new();
        let mut writable = BTreeSet::<PathBuf>::new();
        let mut creatable = BTreeSet::<PathBuf>::new();
        let mut readable = BTreeSet::<PathBuf>::new();
        let mut masks = BTreeSet::<PathBuf>::new();
        for rule in plan.contract().filesystem().rules() {
            let path = native_path(rule.path())?;
            let canonical = fs::canonicalize(&path).map_err(|error| {
                LinuxError::io(LinuxOperation::Project, "resolve filesystem rule path", &error)
            })?;
            if canonical != path {
                return Err(filesystem_error("filesystem rule contains an alias or symlink"));
            }
            if rule.scope() == PathScope::Exact && canonical.is_dir() {
                return Err(filesystem_error(
                    "exact directory rule cannot be represented by path-beneath enforcement",
                ));
            }
            if rule.scope() == PathScope::Exact
                && (rule.operations().contains(FileOperation::Create)
                    || rule.operations().contains(FileOperation::Remove))
            {
                return Err(filesystem_error(
                    "exact create or remove cannot be represented by Landlock path-beneath",
                ));
            }
            match rule.effect() {
                RuleEffect::Deny => {
                    if rule.operations().bits() != ALL_FILE_OPERATIONS {
                        return Err(filesystem_error(
                            "operation-selective filesystem deny is not exactly representable",
                        ));
                    }
                    masks.insert(canonical);
                }
                RuleEffect::Allow => {
                    let access = access_by_path.entry(canonical.clone()).or_default();
                    for operation in [
                        FileOperation::Discover,
                        FileOperation::Metadata,
                        FileOperation::Read,
                        FileOperation::Execute,
                        FileOperation::Create,
                        FileOperation::Write,
                        FileOperation::Remove,
                    ] {
                        if rule.operations().contains(operation) {
                            access.insert_operation(operation);
                            if operation == FileOperation::Create {
                                creatable.insert(canonical.clone());
                            }
                            if matches!(
                                operation,
                                FileOperation::Create
                                    | FileOperation::Write
                                    | FileOperation::Remove
                            ) {
                                writable.insert(canonical.clone());
                            } else {
                                readable.insert(canonical.clone());
                            }
                        }
                    }
                }
            }
        }
        for requirement in plan.requirements().files() {
            let path = native_path(requirement.path())?;
            if !path.exists() {
                return Err(filesystem_error(
                    "required filesystem path does not exist at preparation",
                ));
            }
            if fs::canonicalize(&path).ok().as_deref() != Some(path.as_path()) {
                return Err(filesystem_error(
                    "required filesystem path changed or aliases another path",
                ));
            }
        }
        for protected in policy.protected_roots() {
            if protected.exists() {
                masks.insert(protected.clone());
            } else if creatable.iter().any(|path| protected.starts_with(path)) {
                return Err(filesystem_error(
                    "an absent protected root overlaps a descendant-creation grant",
                ));
            }
        }
        let mut actions = vec![
            MountAction::ReadOnlyBind { source: PathBuf::from("/"), target: PathBuf::from("/") },
            MountAction::Proc { target: PathBuf::from("/proc") },
            MountAction::Dev { target: PathBuf::from("/dev") },
            MountAction::Tmpfs { target: PathBuf::from("/tmp") },
        ];
        for path in readable.difference(&writable) {
            actions.push(MountAction::ReadOnlyBind { source: path.clone(), target: path.clone() });
        }
        for path in writable {
            if !path.starts_with(policy.workspace_root()) {
                return Err(filesystem_error("writable mount is outside the resolved workspace"));
            }
            actions.push(MountAction::WritableBind { source: path.clone(), target: path });
        }
        for path in masks {
            actions.push(MountAction::Mask { target: path });
        }
        let mut landlock_rules =
            vec![LandlockRule::new(PathBuf::from("/"), LandlockAccess::host_read_only())?];
        for (path, access) in access_by_path {
            landlock_rules.push(LandlockRule::new(path, access)?);
        }
        Ok(Self { actions, landlock_rules })
    }
    /// Returns ordered mount operations. Masks always follow writable binds and dominate them.
    #[must_use]
    pub fn actions(&self) -> &[MountAction] {
        &self.actions
    }
    /// Returns ordered Landlock allow rules.
    #[must_use]
    pub fn landlock_rules(&self) -> &[LandlockRule] {
        &self.landlock_rules
    }
}

fn native_path(path: &SandboxPath) -> Result<PathBuf, LinuxError> {
    let native = PathBuf::from(path.as_str());
    if !native.is_absolute() {
        return Err(filesystem_error("Linux sandbox path is not absolute"));
    }
    Ok(native)
}

fn filesystem_error(detail: &'static str) -> LinuxError {
    LinuxError::new(
        LinuxErrorKind::Filesystem,
        LinuxOperation::Project,
        LinuxRecovery::CorrectRequest,
        detail,
    )
}
