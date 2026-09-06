//! Conservative rejection of recognizable credential material in derived entries.
//!
//! This is accidental-disclosure defense, not a complete secret classifier. The archive retains
//! authorized source bytes under the existing tool-access policy and never reads credential stores.

pub(super) fn contains_credential(value: &str) -> bool {
    if [
        "-----BEGIN PRIVATE KEY-----",
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----BEGIN EC PRIVATE KEY-----",
        "-----BEGIN OPENSSH PRIVATE KEY-----",
    ]
    .iter()
    .any(|marker| value.contains(marker))
    {
        return true;
    }
    let lower = value.to_ascii_lowercase();
    ["sk-", "ghp_", "github_pat_", "authorization: bearer ", "authorization: basic "].iter().any(
        |prefix| {
            lower.match_indices(prefix).any(|(index, _)| {
                lower[index + prefix.len()..]
                    .bytes()
                    .take_while(|byte| {
                        byte.is_ascii_alphanumeric()
                            || matches!(byte, b'_' | b'-' | b'.' | b'/' | b'+' | b'=')
                    })
                    .count()
                    >= 20
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_shapes_are_rejected_without_rejecting_security_guidance() {
        for value in [
            "-----BEGIN OPENSSH PRIVATE KEY-----",
            "Authorization: Bearer fixture000000000000000000000000",
            "ghp_fixture000000000000000000000000",
        ] {
            assert!(contains_credential(value));
        }
        for value in [
            "Do not record credentials.",
            "Use a bearer token from the credential store.",
            "Authorization: Bearer [redacted]",
            "A sk- prefix is not itself a credential.",
        ] {
            assert!(!contains_credential(value));
        }
    }
}
