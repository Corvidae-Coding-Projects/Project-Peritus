//! Bounded loopback authorization-code/PKCE flow with verified registration identity.
use super::{
    ChatGptAccount, ISSUER, RESOURCE, SCOPES, Session, auth, network, random, store, valid_client,
};
use base64::Engine as _;
use peritus_provider_core::{CancellationToken, ProviderCoreError, cancel_first};
use sha2::{Digest as _, Sha256};
use std::{
    fmt,
    path::{Path, PathBuf},
    time::Duration,
};
use subtle::ConstantTimeEq as _;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::{TcpListener, TcpStream},
};
use zeroize::Zeroize as _;

/// One pending Peritus OAuth login; owns the callback listener, nonce and PKCE verifier.
pub struct ChatGptLogin {
    root: PathBuf,
    listener: TcpListener,
    url: String,
    redirect: String,
    state: String,
    nonce: String,
    verifier: String,
    host: String,
    existing: Option<PathBuf>,
}
impl fmt::Debug for ChatGptLogin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChatGptLogin([private authorization attempt])")
    }
}
impl Drop for ChatGptLogin {
    fn drop(&mut self) {
        self.verifier.zeroize();
        self.state.zeroize();
        self.nonce.zeroize();
        self.url.zeroize();
    }
}
impl ChatGptLogin {
    /// Starts the loopback listener before producing a browser authorization URL.
    /// `existing` selects an account's saved registration without changing the active provider.
    ///
    /// # Errors
    /// Rejects unsafe storage, unavailable loopback networking, or an invalid selected account.
    pub async fn begin(root: &Path, existing: Option<&str>) -> Result<Self, ProviderCoreError> {
        let host = store::host_id(root)?;
        let existing = existing.map(store::reference_path).transpose()?;
        let prior = existing.as_deref().map(store::load).transpose()?;
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|_| auth("Peritus could not bind the ChatGPT loopback callback"))?;
        let redirect = format!(
            "http://127.0.0.1:{}/auth/callback",
            listener
                .local_addr()
                .map_err(|_| auth("ChatGPT callback address is unavailable"))?
                .port()
        );
        let state = random()?;
        let nonce = random()?;
        let verifier = random()?;
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(Sha256::digest(verifier.as_bytes()));
        let mut url = url::Url::parse(&format!("{ISSUER}/api/accounts/authorize"))
            .map_err(|_| auth("ChatGPT authorization endpoint is invalid"))?;
        {
            let mut query = url.query_pairs_mut();
            query.extend_pairs([
                (
                    "client_id",
                    prior.as_ref().map_or("dynamic_agent_client", |s| s.client_id.as_str()),
                ),
                ("ext_agent_host_id", host.as_str()),
                ("response_type", "code"),
                ("redirect_uri", redirect.as_str()),
                ("scope", SCOPES),
                ("resource", RESOURCE),
                ("state", state.as_str()),
                ("nonce", nonce.as_str()),
                ("code_challenge_method", "S256"),
                ("code_challenge", challenge.as_str()),
            ]);
            if prior.is_none() {
                query.append_pair("agent_name_hint", "Peritus");
            }
            // Omit optional ID-token URL hints; browser account selection stays explicit.
        }
        Ok(Self {
            root: root.to_owned(),
            listener,
            url: url.into(),
            redirect,
            state,
            nonce,
            verifier,
            host,
            existing,
        })
    }
    /// Opens this URL in the user's browser; it contains no access, refresh or ID tokens.
    #[must_use]
    pub fn authorization_url(&self) -> &str {
        &self.url
    }
    /// Completes one login and atomically saves only a verified account-specific grant.
    ///
    /// # Errors
    /// Returns cancellation, timeout, denied consent, token-validation or publication failures.
    pub async fn finish(
        self,
        cancel: &CancellationToken,
    ) -> Result<ChatGptAccount, ProviderCoreError> {
        let result = cancel_first(
            cancel,
            tokio::time::timeout(Duration::from_mins(30), self.complete(cancel)),
        )
        .await
        .ok_or_else(|| ProviderCoreError::cancelled("chatgpt_login"))?;
        result.map_err(|_| auth("ChatGPT sign-in timed out; start a fresh sign-in"))?
    }
    async fn complete(
        &self,
        cancel: &CancellationToken,
    ) -> Result<ChatGptAccount, ProviderCoreError> {
        for _ in 0..256 {
            let (mut socket, peer) = self
                .listener
                .accept()
                .await
                .map_err(|_| auth("ChatGPT callback listener failed"))?;
            if !peer.ip().is_loopback() {
                continue;
            }
            let fields = match callback(&mut socket, &self.redirect, &self.state).await {
                Ok(Some(fields)) => fields,
                Ok(None) => continue,
                Err(error) => return Err(error),
            };
            if fields.iter().any(|(k, _)| k == "error") {
                return Err(auth("ChatGPT consent was declined or plan usage was not authorized"));
            }
            let prior = self.existing.as_deref().map(store::load).transpose()?;
            let client = fields
                .iter()
                .find(|(k, _)| k == "client_id")
                .map(|(_, v)| v.as_str())
                .or_else(|| prior.as_ref().map(|s| s.client_id.as_str()))
                .ok_or_else(|| auth("ChatGPT registration did not return its issued client ID"))?;
            if !valid_client(client) || prior.as_ref().is_some_and(|s| s.client_id != client) {
                return Err(auth("ChatGPT callback changed the selected client registration"));
            }
            let code = fields
                .iter()
                .find(|(k, _)| k == "code")
                .map(|(_, v)| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| auth("ChatGPT callback did not supply an authorization code"))?;
            let reply =
                network::exchange(client, code, &self.verifier, &self.redirect, cancel).await?;
            return self.publish(client, reply, prior.as_ref(), cancel).await;
        }
        Err(auth("ChatGPT callback received too many unrelated requests; retry sign-in"))
    }
    async fn publish(
        &self,
        client: &str,
        reply: network::TokenReply,
        prior: Option<&Session>,
        cancel: &CancellationToken,
    ) -> Result<ChatGptAccount, ProviderCoreError> {
        // Verify the identity before moving any credentials into saved state.
        let claims =
            network::verify_id(reply.id_token(), client, Some(&self.nonce), cancel).await?;
        if prior.is_some_and(|s| s.subject != claims.sub || s.issuer != ISSUER) {
            return Err(auth(
                "ChatGPT sign-in returned a different account; choose Add account instead",
            ));
        }
        let digest = Sha256::digest(format!("{ISSUER}\0{client}\0{}", claims.sub));
        let name = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest) + ".json";
        let path = self.existing.clone().unwrap_or_else(|| self.root.join(name));
        let mut session = Session {
            format_version: 1,
            issuer: ISSUER.to_owned(),
            subject: claims.sub,
            email: claims.email,
            client_id: client.to_owned(),
            ext_agent_host_id: self.host.clone(),
            id_token: String::new(),
            access_token: String::new(),
            refresh_token: String::new(),
            scopes: Vec::new(),
            expires_at: 0,
            earliest_refresh_at: 0,
        };
        reply.apply(&mut session)?;
        let _ownership = store::lock(&path, cancel).await?;
        store::save(&path, &session)?;
        Ok(ChatGptAccount::from_session(path, &session))
    }
}
async fn callback(
    socket: &mut TcpStream,
    redirect: &str,
    state: &str,
) -> Result<Option<Vec<(String, String)>>, ProviderCoreError> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 1024];
    loop {
        let count = tokio::time::timeout(Duration::from_secs(2), socket.read(&mut buffer))
            .await
            .map_err(|_| auth("ChatGPT callback request timed out"))?
            .map_err(|_| auth("ChatGPT callback could not be read"))?;
        if count == 0 {
            return Ok(None);
        }
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() > 16 * 1024 {
            respond(socket, 400).await;
            return Ok(None);
        }
        if bytes.windows(4).any(|s| s == b"\r\n\r\n") {
            break;
        }
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| auth("ChatGPT callback was not UTF-8"))?;
    let mut first = text.lines().next().unwrap_or("").split_whitespace();
    let method = first.next();
    let target = first.next().unwrap_or("");
    let url = url::Url::parse(&format!("{ISSUER}{target}"))
        .map_err(|_| auth("ChatGPT callback path was malformed"))?;
    if method != Some("GET") || url.path() != "/auth/callback" || !target.starts_with('/') {
        respond(socket, 404).await;
        return Ok(None);
    }
    let expected =
        url::Url::parse(redirect).map_err(|_| auth("ChatGPT callback binding is malformed"))?;
    let host = text
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(k, _)| k.eq_ignore_ascii_case("host"))
        .map(|(_, v)| v.trim());
    let expected_host = format!("127.0.0.1:{}", expected.port().unwrap_or(80));
    let fields =
        url.query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect::<Vec<_>>();
    let states = fields.iter().filter(|(k, _)| k == "state").collect::<Vec<_>>();
    if host != Some(expected_host.as_str())
        || states.len() != 1
        || !bool::from(
            Sha256::digest(states[0].1.as_bytes()).ct_eq(&Sha256::digest(state.as_bytes())),
        )
    {
        respond(socket, 400).await;
        return Ok(None);
    }
    for name in ["code", "client_id", "error"] {
        if fields.iter().filter(|(k, _)| k == name).count() > 1 {
            respond(socket, 400).await;
            return Err(auth("ChatGPT callback has duplicate authorization fields"));
        }
    }
    respond(socket, 200).await;
    Ok(Some(fields))
}
async fn respond(socket: &mut TcpStream, status: u16) {
    let message = if status == 200 {
        "Peritus received the sign-in response. Return to Peritus."
    } else {
        "This is not a valid pending Peritus sign-in callback."
    };
    let response = format!(
        "HTTP/1.1 {status} Result\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{message}",
        message.len()
    );
    let _ = socket.write_all(response.as_bytes()).await;
    let _ = socket.shutdown().await;
}
