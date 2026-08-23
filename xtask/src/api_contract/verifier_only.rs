//! B1 verifier-only source policy.

use crate::source::reference_lexer::{TokenKind, tokenize};

const FORBIDDEN_EXACT_IDENTIFIERS: &[&str] = &[
    "DigestSigner",
    "ExpandedSecretKey",
    "KEYPAIR_LENGTH",
    "KeypairBytes",
    "PrehashSigner",
    "SECRET_KEY_LENGTH",
    "SecretKey",
    "SecretKeyBytes",
    "Signer",
    "SignerMut",
    "SigningKey",
    "from_keypair_bytes",
    "raw_sign",
    "raw_sign_prehashed",
    "sign",
    "sign_prehashed",
    "to_keypair_bytes",
    "try_sign",
    "try_sign_digest",
    "try_sign_prehashed",
];

#[derive(Debug, Eq, PartialEq)]
pub(super) struct Violation {
    line: usize,
    identifier: String,
}

impl Violation {
    pub(super) fn message(&self) -> String {
        format!(
            "line {} uses forbidden signing surface `{}` in verifier-only B1 production source",
            self.line, self.identifier
        )
    }

    pub(super) const fn help() -> &'static str {
        "remove all private/signing-key material and signing APIs; B1 production code may expose only strict signature verification through its audited wrapper"
    }
}

pub(super) fn violations(source: &str) -> Vec<Violation> {
    tokenize(source)
        .into_iter()
        .filter_map(|token| match token.kind {
            TokenKind::Identifier(identifier, _) if forbidden(&identifier) => {
                Some(Violation { line: token.line, identifier })
            }
            TokenKind::Identifier(_, _)
            | TokenKind::Punctuation(_)
            | TokenKind::StringLiteral(_) => None,
        })
        .collect()
}

fn forbidden(identifier: &str) -> bool {
    let normalized: String = identifier
        .chars()
        .filter(|character| *character != '_')
        .map(|character| character.to_ascii_lowercase())
        .collect();
    FORBIDDEN_EXACT_IDENTIFIERS.contains(&identifier)
        || normalized.contains("privatekey")
        || normalized.contains("secretkey")
        || normalized.contains("signingkey")
}

#[cfg(test)]
mod tests {
    use super::violations;

    #[test]
    fn rejects_types_traits_methods_aliases_and_key_material_names() {
        for identifier in [
            "SigningKey",
            "Signer",
            "try_sign",
            "raw_sign_prehashed",
            "workflow_private_key_bytes",
            "secret_key_material",
            "embeddedPrivateKeyMaterial",
            "r#SigningKey",
        ] {
            let source = format!("let {identifier} = ();\n");
            let found = violations(&source);
            assert_eq!(found.len(), 1, "{identifier} must be forbidden: {found:?}");
        }
    }

    #[test]
    fn permits_verification_surface_and_ignores_comments_and_strings() {
        let source = r#"
use ed25519_dalek::{Signature, VerifyingKey};
// SigningKey and secret_key_material are not source tokens here.
let method_name = "try_sign";
key.verify_strict(message, &signature)
"#;
        assert!(violations(source).is_empty());
    }
}
