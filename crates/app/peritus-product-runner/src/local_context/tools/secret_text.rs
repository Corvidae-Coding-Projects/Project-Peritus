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
