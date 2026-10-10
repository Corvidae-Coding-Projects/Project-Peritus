//! Authority classes are independent from the source provenance label.

#[doc = " Authority attached to content without interpreting the content text."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AuthorityClass {
    #[doc = " Non-overridable system policy."]
    SystemPolicy,
    #[doc = " Application-level policy below system policy."]
    ApplicationPolicy,
    #[doc = " Immutable acceptance criteria and specifications."]
    AcceptanceSpecification,
    #[doc = " The active user\'s explicit instruction."]
    UserInstruction,
    #[doc = " Evidence or derived text that carries no instruction authority."]
    NonAuthoritative,
}
impl AuthorityClass {
    pub(crate) const fn precedence(self) -> u8 {
        match self {
            Self::SystemPolicy => 5,
            Self::ApplicationPolicy => 4,
            Self::AcceptanceSpecification => 3,
            Self::UserInstruction => 2,
            Self::NonAuthoritative => 1,
        }
    }
}
