//! Logical filesystem capability contracts.

use crate::{SandboxError, SandboxFeature};

const MAX_PATH_BYTES: usize = 4_096;
const MAX_RULES: usize = 256;

/// A normalized, platform-neutral absolute path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SandboxPath(String);

impl SandboxPath {
    /// Validates and stores an absolute logical path.
    ///
    /// # Errors
    /// Returns an input error for relative paths, traversal, backslashes, NUL, or excessive size.
    pub fn new(value: impl Into<String>) -> Result<Self, SandboxError> {
        let mut value = value.into();
        if value.is_empty() || value.len() > MAX_PATH_BYTES || value.contains(['\0', '\\']) {
            return Err(crate::error::invalid("invalid sandbox path representation"));
        }
        let drive_absolute = value.len() >= 3
            && value.as_bytes()[0].is_ascii_alphabetic()
            && value.as_bytes()[1] == b':'
            && value.as_bytes()[2] == b'/';
        if !value.starts_with('/') && !drive_absolute {
            return Err(crate::error::invalid("sandbox paths must be absolute"));
        }
        let component_start = if drive_absolute { 3 } else { 1 };
        if value[component_start..]
            .split('/')
            .any(|component| component == "." || component == "..")
        {
            return Err(crate::error::invalid("sandbox paths cannot contain traversal"));
        }
        while value.len() > component_start && value.ends_with('/') {
            value.pop();
        }
        if drive_absolute {
            let drive = char::from(value.as_bytes()[0].to_ascii_uppercase()).to_string();
            value.replace_range(0..1, &drive);
        }
        Ok(Self(value))
    }

    /// Returns the normalized path text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn matches(&self, candidate: &Self, scope: PathScope) -> bool {
        if self == candidate {
            return true;
        }
        scope == PathScope::Descendants
            && candidate.0.starts_with(&self.0)
            && (self.0.ends_with('/') || candidate.0.as_bytes().get(self.0.len()) == Some(&b'/'))
    }
}

/// Whether a matching rule permits or rejects an operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuleEffect {
    /// Permit the matching operation.
    Allow,
    /// Reject the matching operation.
    Deny,
}

impl RuleEffect {
    pub(crate) const fn ordinal(self) -> u8 {
        match self {
            Self::Allow => 0,
            Self::Deny => 1,
        }
    }
}

/// The path extent covered by a rule.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PathScope {
    /// Match only the named path.
    Exact,
    /// Match the named path and all descendants.
    Descendants,
}

impl PathScope {
    pub(crate) const fn ordinal(self) -> u8 {
        match self {
            Self::Exact => 0,
            Self::Descendants => 1,
        }
    }
}

/// A filesystem operation governed by the contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FileOperation {
    /// Discover whether a path exists.
    Discover,
    /// Read metadata.
    Metadata,
    /// Read content.
    Read,
    /// Execute a file.
    Execute,
    /// Create an entry.
    Create,
    /// Write content.
    Write,
    /// Remove an entry.
    Remove,
}

impl FileOperation {
    pub(crate) const ALL: [Self; 7] = [
        Self::Discover,
        Self::Metadata,
        Self::Read,
        Self::Execute,
        Self::Create,
        Self::Write,
        Self::Remove,
    ];

    pub(crate) const fn feature(self) -> SandboxFeature {
        match self {
            Self::Discover => SandboxFeature::FilesystemDiscover,
            Self::Metadata => SandboxFeature::FilesystemMetadata,
            Self::Read => SandboxFeature::FilesystemRead,
            Self::Execute => SandboxFeature::FilesystemExecute,
            Self::Create => SandboxFeature::FilesystemCreate,
            Self::Write => SandboxFeature::FilesystemWrite,
            Self::Remove => SandboxFeature::FilesystemRemove,
        }
    }

    const fn bit(self) -> u8 {
        match self {
            Self::Discover => 1 << 0,
            Self::Metadata => 1 << 1,
            Self::Read => 1 << 2,
            Self::Execute => 1 << 3,
            Self::Create => 1 << 4,
            Self::Write => 1 << 5,
            Self::Remove => 1 << 6,
        }
    }

    pub(crate) const fn ordinal(self) -> u8 {
        match self {
            Self::Discover => 0,
            Self::Metadata => 1,
            Self::Read => 2,
            Self::Execute => 3,
            Self::Create => 4,
            Self::Write => 5,
            Self::Remove => 6,
        }
    }
}

/// A compact nonempty-or-empty set of filesystem operations.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FileOperationSet(u8);

impl FileOperationSet {
    /// Creates an empty set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Creates a set from operations.
    #[must_use]
    pub fn from_operations(operations: impl IntoIterator<Item = FileOperation>) -> Self {
        let mut set = Self::empty();
        for operation in operations {
            set.insert(operation);
        }
        set
    }

    /// Adds an operation.
    pub const fn insert(&mut self, operation: FileOperation) {
        self.0 |= operation.bit();
    }

    /// Reports whether the set contains an operation.
    #[must_use]
    pub const fn contains(self, operation: FileOperation) -> bool {
        self.0 & operation.bit() != 0
    }

    /// Returns the stable bit representation.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }
}

/// One path-scoped filesystem rule.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FilesystemRule {
    effect: RuleEffect,
    path: SandboxPath,
    scope: PathScope,
    operations: FileOperationSet,
}

impl FilesystemRule {
    /// Creates a rule. Empty operation sets are rejected.
    ///
    /// # Errors
    /// Returns an input error when `operations` is empty.
    pub fn new(
        effect: RuleEffect,
        path: SandboxPath,
        scope: PathScope,
        operations: FileOperationSet,
    ) -> Result<Self, SandboxError> {
        if operations.bits() == 0 {
            return Err(crate::error::invalid("filesystem rule is empty"));
        }
        Ok(Self { effect, path, scope, operations })
    }

    /// Returns the effect.
    #[must_use]
    pub const fn effect(&self) -> RuleEffect {
        self.effect
    }
    /// Returns the path.
    #[must_use]
    pub const fn path(&self) -> &SandboxPath {
        &self.path
    }
    /// Returns the scope.
    #[must_use]
    pub const fn scope(&self) -> PathScope {
        self.scope
    }
    /// Returns the operations.
    #[must_use]
    pub const fn operations(&self) -> FileOperationSet {
        self.operations
    }
}

/// The result of a filesystem contract evaluation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileDecision {
    /// An allow rule matched and no deny rule matched.
    Allowed,
    /// A deny rule matched.
    DeniedByRule,
    /// No allow rule matched; contracts are deny by default.
    DeniedByDefault,
}

/// Canonically ordered, deny-by-default filesystem rules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemContract {
    rules: Vec<FilesystemRule>,
}

impl FilesystemContract {
    /// Validates, sorts, and deduplicates rules.
    ///
    /// # Errors
    /// Returns a limit error when more than 256 rules are supplied.
    pub fn new(mut rules: Vec<FilesystemRule>) -> Result<Self, SandboxError> {
        if rules.len() > MAX_RULES {
            return Err(crate::error::bound("too many filesystem rules"));
        }
        rules.sort();
        rules.dedup();
        Ok(Self { rules })
    }

    /// Returns an empty, deny-all contract.
    #[must_use]
    pub const fn deny_all() -> Self {
        Self { rules: Vec::new() }
    }

    /// Returns canonical rules.
    #[must_use]
    pub fn rules(&self) -> &[FilesystemRule] {
        &self.rules
    }

    /// Evaluates a logical operation using deny precedence.
    #[must_use]
    pub fn decide(&self, path: &SandboxPath, operation: FileOperation) -> FileDecision {
        let mut allowed = false;
        for rule in &self.rules {
            if rule.operations.contains(operation) && rule.path.matches(path, rule.scope) {
                match rule.effect {
                    RuleEffect::Deny => return FileDecision::DeniedByRule,
                    RuleEffect::Allow => allowed = true,
                }
            }
        }
        if allowed { FileDecision::Allowed } else { FileDecision::DeniedByDefault }
    }
}
