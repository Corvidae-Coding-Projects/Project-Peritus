#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ViolationKind {
    ExposedRequires,
    PublicTraitContract,
    OpaqueReturn,
    UnsupportedAttribute,
    UnsupportedMacro,
    UnparseableHeader,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct Violation {
    pub(super) line: usize,
    pub(super) function: String,
    pub(super) clause: Option<String>,
    pub(super) kind: ViolationKind,
}

impl Violation {
    pub(super) fn message(&self) -> String {
        match self.kind {
            ViolationKind::ExposedRequires => format!(
                "line {} exposes executable function `{}` with a Verus `requires` clause",
                self.line, self.function
            ),
            ViolationKind::PublicTraitContract => format!(
                "line {} exposes safe trait function `{}` with caller-unenforceable `{}` contract",
                self.line,
                self.function,
                self.clause.as_deref().unwrap_or("unknown")
            ),
            ViolationKind::OpaqueReturn => format!(
                "line {} function `{}` returns an opaque `impl Trait` value across a formal boundary",
                self.line, self.function
            ),
            ViolationKind::UnsupportedAttribute => format!(
                "line {} uses unsupported attribute `{}` in formal-boundary source",
                self.line, self.function
            ),
            ViolationKind::UnsupportedMacro => format!(
                "line {} uses unsupported macro `{}` in formal-boundary source",
                self.line, self.function
            ),
            ViolationKind::UnparseableHeader => format!(
                "line {} executable function `{}` has a header the API checker cannot delimit",
                self.line, self.function
            ),
        }
    }

    pub(super) const fn help(&self) -> &'static str {
        match self.kind {
            ViolationKind::ExposedRequires => {
                "keep the precondition on a private verified helper and expose a total wrapper that validates it at runtime"
            }
            ViolationKind::PublicTraitContract => {
                "seal or make the trait unsafe, or expose an ordinary-safe trait contract that an unverified implementation cannot violate"
            }
            ViolationKind::OpaqueReturn => {
                "return a concrete audited type so a private precondition-bearing function cannot escape through an opaque value"
            }
            ViolationKind::UnsupportedAttribute => {
                "remove the attribute or teach the fail-closed checker its exact expansion and contract semantics before use"
            }
            ViolationKind::UnsupportedMacro => {
                "remove the macro or teach the fail-closed checker its exact expansion semantics before it can generate formal-boundary code"
            }
            ViolationKind::UnparseableHeader => {
                "use an explicit Rust/Verus function declaration that the fail-closed boundary checker can audit"
            }
        }
    }
}
