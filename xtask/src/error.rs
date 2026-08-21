use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

/// Stable category for a failure reported by the workspace policy tool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    /// Command-line arguments are invalid.
    Invocation,
    /// A filesystem operation failed.
    Io,
    /// A checked-in policy document is invalid.
    Policy,
    /// Cargo metadata could not be obtained or decoded.
    Metadata,
    /// The crate graph violates architecture policy.
    Architecture,
    /// Rust source violates layout policy.
    SourceLayout,
    /// A trusted Verus construct occurs outside an allowed boundary.
    Trust,
    /// A reproducibility pin or locked-input rule is violated.
    Reproducibility,
}

impl ErrorCode {
    /// Returns the stable printable identifier for this category.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Invocation => "PERITUS-XTASK-CLI-001",
            Self::Io => "PERITUS-XTASK-IO-001",
            Self::Policy => "PERITUS-XTASK-POLICY-001",
            Self::Metadata => "PERITUS-XTASK-METADATA-001",
            Self::Architecture => "PERITUS-XTASK-ARCH-001",
            Self::SourceLayout => "PERITUS-XTASK-SOURCE-001",
            Self::Trust => "PERITUS-XTASK-TRUST-001",
            Self::Reproducibility => "PERITUS-XTASK-REPRO-001",
        }
    }
}

/// One actionable policy violation within a failed check.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    path: Option<PathBuf>,
    message: String,
    help: String,
}

impl Diagnostic {
    pub(crate) fn new(message: impl Into<String>, help: impl Into<String>) -> Self {
        Self { path: None, message: message.into(), help: help.into() }
    }

    pub(crate) fn at(
        path: impl Into<PathBuf>,
        message: impl Into<String>,
        help: impl Into<String>,
    ) -> Self {
        Self { path: Some(path.into()), message: message.into(), help: help.into() }
    }

    /// Returns the repository-relative path associated with the violation.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Returns a concise explanation of the violation.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the concrete recovery guidance for the violation.
    #[must_use]
    pub fn help(&self) -> &str {
        &self.help
    }
}

/// Typed error returned by an `xtask` command.
#[derive(Debug)]
pub struct XtaskError {
    code: ErrorCode,
    message: String,
    diagnostics: Vec<Diagnostic>,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl XtaskError {
    pub(crate) fn invocation(message: impl Into<String>) -> Self {
        Self::plain(ErrorCode::Invocation, message)
    }

    pub(crate) fn io(operation: &str, path: &Path, source: std::io::Error) -> Self {
        Self {
            code: ErrorCode::Io,
            message: format!("could not {operation} {}", path.display()),
            diagnostics: Vec::new(),
            source: Some(Box::new(source)),
        }
    }

    pub(crate) fn parse_policy(path: &Path, source: toml::de::Error) -> Self {
        Self {
            code: ErrorCode::Policy,
            message: format!("could not parse policy file {}", path.display()),
            diagnostics: vec![Diagnostic::at(
                path,
                "the TOML policy is not valid for the current schema",
                "correct the reported TOML error; do not weaken the schema to accept ambiguity",
            )],
            source: Some(Box::new(source)),
        }
    }

    pub(crate) fn metadata(message: impl Into<String>) -> Self {
        Self::plain(ErrorCode::Metadata, message)
    }

    pub(crate) fn metadata_decode(source: serde_json::Error) -> Self {
        Self {
            code: ErrorCode::Metadata,
            message: "Cargo returned metadata that xtask could not decode".to_owned(),
            diagnostics: Vec::new(),
            source: Some(Box::new(source)),
        }
    }

    pub(crate) fn violations(code: ErrorCode, check: &str, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            code,
            message: format!("{check} found {} violation(s)", diagnostics.len()),
            diagnostics,
            source: None,
        }
    }

    fn plain(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), diagnostics: Vec::new(), source: None }
    }

    /// Returns the stable category for programmatic handling.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// Returns every actionable violation collected by the failed check.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Renders a complete human-facing report with stable codes and recovery guidance.
    #[must_use]
    pub fn render(&self) -> String {
        let mut rendered = format!("error[{}]: {}", self.code.as_str(), self.message);
        for diagnostic in &self.diagnostics {
            rendered.push_str("\n  - ");
            if let Some(path) = &diagnostic.path {
                rendered.push_str(&path.display().to_string());
                rendered.push_str(": ");
            }
            rendered.push_str(&diagnostic.message);
            rendered.push_str("\n    help: ");
            rendered.push_str(&diagnostic.help);
        }
        if let Some(source) = &self.source {
            rendered.push_str("\n    caused by: ");
            rendered.push_str(&source.to_string());
        }
        rendered
    }
}

impl Display for XtaskError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.render())
    }
}

impl Error for XtaskError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|source| source as &(dyn Error + 'static))
    }
}
