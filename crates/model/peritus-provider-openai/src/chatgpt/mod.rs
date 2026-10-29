//! Direct ChatGPT-plan OAuth: independent registration, protected sessions and Responses transport.

mod login;
mod network;
mod provider;
mod store;

use peritus_provider_core::ProviderCoreError;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    path::{Path, PathBuf},
};
use zeroize::Zeroize as _;

pub use login::ChatGptLogin;
pub use provider::ChatGptProvider;

const ISSUER: &str = "https://auth.openai.com";
const RESOURCE: &str = "https://api.openai.com/v1";
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
const REFERENCE: &str = "peritus-chatgpt-oauth-v1:";

/// Non-secret identity of one validated `ChatGPT` registration and its protected credential location.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatGptAccount {
    label: String,
    client_id: String,
    path: PathBuf,
    authorized: bool,
}
impl ChatGptAccount {
    /// Human-readable account label; registrations remain distinct even with identical email.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
    /// Returns the exact registration identity, not a bearer credential.
    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.client_id
    }
    /// Whether the saved grant authorizes the direct ChatGPT-plan inference route.
    #[must_use]
    pub const fn authorized(&self) -> bool {
        self.authorized
    }
    /// Returns a non-secret reference suitable for saved Peritus provider configuration.
    #[must_use]
    pub fn credential_reference(&self) -> String {
        format!("{REFERENCE}{}", self.path.display())
    }
    /// Lists independent saved registrations in this application's OAuth directory.
    ///
    /// # Errors
    /// Rejects unsafe storage and malformed credential records without exposing their contents.
    pub fn list(root: &Path) -> Result<Vec<Self>, ProviderCoreError> {
        store::list(root)
    }
    /// Inspects an existing protected registration without refreshing it or making a request.
    ///
    /// # Errors
    /// Rejects a malformed reference, unsafe storage, or an unavailable registration.
    pub fn from_reference(reference: &str) -> Result<Self, ProviderCoreError> {
        let path = store::reference_path(reference)?;
        let session = store::load(&path)?;
        Ok(Self::from_session(path, &session))
    }
    fn from_session(path: PathBuf, session: &Session) -> Self {
        let name = session
            .email
            .as_deref()
            .filter(|s| s.len() <= 320 && !s.chars().any(char::is_control))
            .unwrap_or("ChatGPT account");
        let suffix = &session.client_id[session.client_id.len().saturating_sub(8)..];
        Self {
            label: format!("{name} · {suffix}"),
            client_id: session.client_id.clone(),
            path,
            authorized: session.permits_inference(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Session {
    format_version: u8,
    issuer: String,
    subject: String,
    email: Option<String>,
    client_id: String,
    ext_agent_host_id: String,
    id_token: String,
    access_token: String,
    refresh_token: String,
    scopes: Vec<String>,
    expires_at: u64,
    earliest_refresh_at: u64,
}
impl Session {
    fn permits_inference(&self) -> bool {
        !self.access_token.is_empty()
            && self.scopes.iter().any(|s| s == "resource.invoke")
            && self.scopes.iter().any(|s| s == "chatgpt.tokens.use.direct")
    }
    fn validate(&self) -> Result<(), ProviderCoreError> {
        if self.format_version != 1
            || self.issuer != ISSUER
            || self.subject.is_empty()
            || self.subject.len() > 2048
            || self
                .email
                .as_ref()
                .is_some_and(|email| email.len() > 1024 || email.chars().any(char::is_control))
            || !valid_client(&self.client_id)
            || !valid_host(&self.ext_agent_host_id)
            || self.scopes.len() > 64
            || self
                .scopes
                .iter()
                .any(|s| s.is_empty() || s.len() > 256 || s.chars().any(char::is_whitespace))
            || self.id_token.len() > 64 * 1024
            || self.access_token.len() > 64 * 1024
            || self.refresh_token.len() > 64 * 1024
        {
            return Err(auth("ChatGPT registration is malformed; sign in again"));
        }
        Ok(())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.id_token.zeroize();
        self.access_token.zeroize();
        self.refresh_token.zeroize();
    }
}
impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChatGptSession([redacted])")
    }
}
fn valid_client(client: &str) -> bool {
    !client.is_empty()
        && client != "dynamic_agent_client"
        && client.len() <= 256
        && client.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
const fn auth(message: &'static str) -> ProviderCoreError {
    ProviderCoreError::credential(message)
}
const fn io_error() -> ProviderCoreError {
    ProviderCoreError::configuration(
        "chatgpt_session",
        "ChatGPT credential storage is unavailable or unsafe",
    )
}
fn now() -> Result<u64, ProviderCoreError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|v| v.as_secs())
        .map_err(|_| auth("system time cannot validate ChatGPT credentials"))
}
fn random() -> Result<String, ProviderCoreError> {
    use base64::Engine as _;
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| auth("secure randomness is unavailable"))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn valid_host(value: &str) -> bool {
    let Some(uuid) = value.strip_prefix("urn:uuid:") else {
        return false;
    };
    uuid.len() == 36
        && uuid.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) { byte == b'-' } else { byte.is_ascii_hexdigit() }
        })
}
fn host_identifier() -> Result<String, ProviderCoreError> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| auth("secure randomness is unavailable"))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let time_low = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let time_mid = u16::from_be_bytes([bytes[4], bytes[5]]);
    let time_high = u16::from_be_bytes([bytes[6], bytes[7]]);
    let clock_sequence = u16::from_be_bytes([bytes[8], bytes[9]]);
    let node = u64::from_be_bytes([
        0, 0, bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    ]);
    Ok(format!(
        "urn:uuid:{time_low:08x}-{time_mid:04x}-{time_high:04x}-{clock_sequence:04x}-{node:012x}"
    ))
}
