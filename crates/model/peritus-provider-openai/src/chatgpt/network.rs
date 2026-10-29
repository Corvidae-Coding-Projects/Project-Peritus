//! Fixed-origin OAuth/JWKS operations; raw vendor bodies and tokens never enter errors.
use super::{ISSUER, RESOURCE, Session, auth, now};
use peritus_provider_core::{CancellationToken, ProviderCoreError, cancel_first};
use serde::Deserialize;
use serde_json::Value;
use zeroize::{Zeroize as _, Zeroizing};

pub(super) fn client() -> Result<reqwest::Client, ProviderCoreError> {
    reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(concat!("Peritus/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| auth("ChatGPT HTTPS transport is unavailable"))
}
async fn json(
    request: reqwest::RequestBuilder,
    cancel: &CancellationToken,
) -> Result<Value, ProviderCoreError> {
    bounded_json(request, cancel, 256 * 1024).await
}
async fn bounded_json(
    request: reqwest::RequestBuilder,
    cancel: &CancellationToken,
    max_bytes: usize,
) -> Result<Value, ProviderCoreError> {
    cancel_first(cancel, async {
        let mut response = request.send().await.map_err(|_| {
            ProviderCoreError::connect(
                "chatgpt_https",
                "ChatGPT authentication service could not be reached",
            )
        })?;
        let status = response.status();
        let mut bytes = Zeroizing::new(Vec::new());
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| auth("ChatGPT authentication response was interrupted"))?
        {
            if bytes.len().saturating_add(chunk.len()) > max_bytes {
                return Err(auth("ChatGPT authentication response exceeds its bound"));
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| auth("ChatGPT authentication response was not valid JSON"))?;
        if !status.is_success() {
            return Err(response_error(&value));
        }
        Ok(value)
    })
    .await
    .unwrap_or_else(|| Err(ProviderCoreError::cancelled("chatgpt_https")))
}
fn response_error(value: &Value) -> ProviderCoreError {
    let code = value["error"].as_str().or_else(|| value["error"]["code"].as_str()).unwrap_or("");
    match code {
        "invalid_grant"
        | "invalid_refresh_token"
        | "token_expired"
        | "refresh_token_expired"
        | "refresh_token_invalidated"
        | "refresh_token_reused" => ProviderCoreError::configuration(
            "chatgpt_reauthorize",
            "ChatGPT authorization expired or was revoked; sign in again with this registration",
        ),
        "invalid_client" => auth("ChatGPT registration was rejected; sign in again"),
        "too_early" => ProviderCoreError::transport(
            "chatgpt_refresh_early",
            "ChatGPT token refresh is not yet allowed; retry near token expiry",
        ),
        _ => auth(
            "ChatGPT authentication was refused; inspect account permissions and sign in again",
        ),
    }
}
#[derive(Deserialize)]
pub(super) struct TokenReply {
    access_token: String,
    refresh_token: String,
    id_token: String,
    token_type: String,
    expires_in: u64,
    scope: String,
    #[serde(default)]
    earliest_refresh_at: Value,
}
impl Drop for TokenReply {
    fn drop(&mut self) {
        self.access_token.zeroize();
        self.refresh_token.zeroize();
        self.id_token.zeroize();
    }
}
impl TokenReply {
    pub(super) fn id_token(&self) -> &str {
        &self.id_token
    }
    pub(super) fn apply(mut self, session: &mut Session) -> Result<(), ProviderCoreError> {
        if !self.token_type.eq_ignore_ascii_case("Bearer")
            || self.access_token.is_empty()
            || self.refresh_token.is_empty()
            || self.expires_in == 0
            || self.expires_in > 86400
        {
            return Err(auth("ChatGPT token response is incomplete or unsupported"));
        }
        let earliest = match &self.earliest_refresh_at {
            Value::Null => 0,
            Value::Number(n) => {
                n.as_u64().ok_or_else(|| auth("ChatGPT refresh time is malformed"))?
            }
            Value::String(s) => s.parse().map_err(|_| auth("ChatGPT refresh time is malformed"))?,
            _ => return Err(auth("ChatGPT refresh time is malformed")),
        };
        session.access_token.zeroize();
        session.refresh_token.zeroize();
        session.id_token.zeroize();
        session.access_token = std::mem::take(&mut self.access_token);
        session.refresh_token = std::mem::take(&mut self.refresh_token);
        session.id_token = std::mem::take(&mut self.id_token);
        session.expires_at =
            now()?.checked_add(self.expires_in).ok_or_else(|| auth("ChatGPT expiry overflows"))?;
        session.earliest_refresh_at = earliest;
        session.scopes = self.scope.split_whitespace().map(str::to_owned).collect();
        session.validate()
    }
}
#[derive(Deserialize)]
pub(super) struct Claims {
    pub(super) sub: String,
    pub(super) email: Option<String>,
    nonce: Option<String>,
    aud: Value,
    azp: Option<String>,
}
pub(super) async fn verify_id(
    token: &str,
    client_id: &str,
    nonce: Option<&str>,
    cancel: &CancellationToken,
) -> Result<Claims, ProviderCoreError> {
    use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
    use sha2::{Digest as _, Sha256};
    use subtle::ConstantTimeEq as _;
    let header = decode_header(token).map_err(|_| auth("ChatGPT ID token is malformed"))?;
    if !matches!(header.alg, Algorithm::RS256 | Algorithm::ES256) {
        return Err(auth("ChatGPT ID token uses an unsupported signing algorithm"));
    }
    let kid = header.kid.ok_or_else(|| auth("ChatGPT ID token has no signing key identity"))?;
    let keys: JwkSet = serde_json::from_value(
        json(client()?.get(format!("{ISSUER}/.well-known/jwks.json")), cancel).await?,
    )
    .map_err(|_| auth("OpenAI signing keys are malformed"))?;
    let jwk =
        keys.find(&kid).ok_or_else(|| auth("OpenAI signing key is unavailable; retry sign-in"))?;
    let key = DecodingKey::from_jwk(jwk).map_err(|_| auth("OpenAI signing key is unsupported"))?;
    let mut validation = Validation::new(header.alg);
    validation.set_audience(&[client_id]);
    validation.set_issuer(&[ISSUER]);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    validation.validate_nbf = true;
    validation.leeway = 30;
    let claims = decode::<Claims>(token, &key, &validation)
        .map_err(|_| auth("ChatGPT ID token identity, signature or expiry could not be verified"))?
        .claims;
    if claims.sub.is_empty()
        || claims.sub.len() > 2048
        || claims.azp.as_deref().is_some_and(|azp| azp != client_id)
        || claims.aud.as_array().is_some_and(|a| a.len() > 1)
            && claims.azp.as_deref() != Some(client_id)
    {
        return Err(auth("ChatGPT ID token does not identify this registration"));
    }
    if let Some(expected) = nonce {
        let found =
            claims.nonce.as_deref().ok_or_else(|| auth("ChatGPT ID token has no login nonce"))?;
        if !bool::from(Sha256::digest(found.as_bytes()).ct_eq(&Sha256::digest(expected.as_bytes())))
        {
            return Err(auth("ChatGPT login nonce did not match"));
        }
    }
    Ok(claims)
}
pub(super) async fn exchange(
    client_id: &str,
    code: &str,
    verifier: &str,
    redirect: &str,
    cancel: &CancellationToken,
) -> Result<TokenReply, ProviderCoreError> {
    let value = json(
        client()?.post(format!("{ISSUER}/api/accounts/oauth/token")).form(&[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", redirect),
            ("resource", RESOURCE),
        ]),
        cancel,
    )
    .await?;
    serde_json::from_value(value)
        .map_err(|_| auth("ChatGPT token exchange returned an incomplete grant"))
}
pub(super) async fn refresh(
    session: &mut Session,
    cancel: &CancellationToken,
) -> Result<(), ProviderCoreError> {
    let value = json(
        client()?.post(format!("{ISSUER}/api/accounts/oauth/token")).form(&[
            ("grant_type", "refresh_token"),
            ("client_id", session.client_id.as_str()),
            ("refresh_token", session.refresh_token.as_str()),
            ("resource", RESOURCE),
        ]),
        cancel,
    )
    .await?;
    let reply: TokenReply = serde_json::from_value(value)
        .map_err(|_| auth("ChatGPT refresh returned an incomplete grant"))?;
    let claims = verify_id(&reply.id_token, &session.client_id, None, cancel).await?;
    if claims.sub != session.subject {
        return Err(auth(
            "ChatGPT refresh changed account identity; credentials were not replaced",
        ));
    }
    reply.apply(session)
}
pub(super) async fn catalog(
    token: &str,
    cancel: &CancellationToken,
) -> Result<Value, ProviderCoreError> {
    bounded_json(
        client()?.get(format!("{RESOURCE}/models")).bearer_auth(token),
        cancel,
        4 * 1024 * 1024,
    )
    .await
}

/// Revoke through the issuer's discovered HTTPS endpoint; an empty successful body is valid.
pub(super) async fn revoke(
    session: &Session,
    cancel: &CancellationToken,
) -> Result<(), ProviderCoreError> {
    let discovery =
        json(client()?.get(format!("{ISSUER}/.well-known/openid-configuration")), cancel).await?;
    let endpoint = discovery["revocation_endpoint"]
        .as_str()
        .ok_or_else(|| auth("ChatGPT revocation is unavailable"))?;
    let url =
        url::Url::parse(endpoint).map_err(|_| auth("ChatGPT revocation endpoint is malformed"))?;
    if url.scheme() != "https"
        || url.host_str() != Some("auth.openai.com")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(auth("ChatGPT revocation endpoint is not trusted"));
    }
    let response = cancel_first(
        cancel,
        client()?
            .post(url)
            .form(&[
                ("token", session.refresh_token.as_str()),
                ("token_type_hint", "refresh_token"),
                ("client_id", session.client_id.as_str()),
            ])
            .send(),
    )
    .await
    .ok_or_else(|| ProviderCoreError::cancelled("chatgpt_sign_out"))?
    .map_err(|_| auth("ChatGPT remote revocation could not be confirmed"))?;
    if !response.status().is_success() {
        return Err(auth("ChatGPT remote revocation could not be confirmed"));
    }
    Ok(())
}
