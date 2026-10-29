//! Direct Responses provider with fresh account credentials and a shared bounded catalog cache.
use super::{ChatGptAccount, auth, network, now, store};
use crate::{OpenAiConfig, OpenAiProvider};
use peritus_model_protocol::{
    ModelName, ModelRequest, ProviderProfile, ReasoningEffort, WireDialect,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, Credential, CredentialReference, CredentialSource, ModelProvider,
    OwnedModelStream, ProviderAvailability, ProviderCoreError,
    catalog::{DiscoveredModel, MAX_CATALOG_MODELS, profile_with_model_limits},
};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use zeroize::{Zeroize as _, Zeroizing};

type CatalogCache = Mutex<Option<(Instant, Vec<DiscoveredModel>)>>;

const TOKEN_REFERENCE: &str = "peritus-chatgpt-current-access";
#[derive(Default)]
struct Access(Mutex<Option<Zeroizing<Vec<u8>>>>);
impl CredentialSource for Access {
    fn resolve(&self, reference: &CredentialReference) -> Result<Credential, ProviderCoreError> {
        if reference.as_str() != TOKEN_REFERENCE {
            return Err(auth("ChatGPT credential reference does not match this route"));
        }
        let token = self.0.lock().map_err(|_| auth("ChatGPT credential owner is unavailable"))?;
        Credential::new(
            token
                .as_ref()
                .ok_or_else(|| auth("ChatGPT request has no admitted access token"))?
                .to_vec(),
        )
    }
}
/// Optional direct ChatGPT-plan provider; the existing Codex provider remains independent.
pub struct ChatGptProvider {
    account: ChatGptAccount,
    inner: Arc<dyn ModelProvider>,
    access: Arc<Access>,
    catalog: Arc<CatalogCache>,
}
impl ChatGptProvider {
    /// Binds one existing protected registration and a stateless advisory-output Responses profile.
    ///
    /// # Errors
    /// Rejects unsafe credentials, a mismatched profile, or unavailable HTTP configuration.
    pub fn new(reference: &str, profile: ProviderProfile) -> Result<Self, ProviderCoreError> {
        let account = ChatGptAccount::from_reference(reference)?;
        let access = Arc::new(Access::default());
        let config =
            OpenAiConfig::chatgpt_plan(CredentialReference::new(TOKEN_REFERENCE.to_owned())?)?;
        let inner = Arc::new(OpenAiProvider::new(config, profile, access.clone())?);
        Ok(Self { account, inner, access, catalog: Arc::new(Mutex::new(None)) })
    }
    async fn ready(&self, cancel: &CancellationToken) -> Result<(), ProviderCoreError> {
        let token = access_token(&self.account.path, cancel).await?;
        *self.access.0.lock().map_err(|_| auth("ChatGPT credential owner is unavailable"))? =
            Some(token);
        Ok(())
    }
}
impl ModelProvider for ChatGptProvider {
    fn profile(&self) -> &ProviderProfile {
        self.inner.profile()
    }
    fn supports_reasoning_effort(&self, effort: ReasoningEffort) -> bool {
        self.inner.supports_reasoning_effort(effort)
    }
    fn availability(&self) -> ProviderAvailability {
        match store::load(&self.account.path) {
            Ok(s) if s.permits_inference() => ProviderAvailability::CredentialPresent,
            _ => ProviderAvailability::Unavailable,
        }
    }
    fn select_model(&self, model: ModelName) -> Result<Arc<dyn ModelProvider>, ProviderCoreError> {
        Ok(Arc::new(Self {
            account: self.account.clone(),
            inner: self.inner.select_model(model)?,
            access: Arc::clone(&self.access),
            catalog: Arc::clone(&self.catalog),
        }))
    }
    fn request_profile(&self, output_tokens: u64) -> Result<ProviderProfile, ProviderCoreError> {
        let saved =
            self.catalog.lock().map_err(|_| auth("ChatGPT model catalog is unavailable"))?;
        saved
            .as_ref()
            .and_then(|(_, models)| models.iter().find(|m| &m.id == self.profile().model()))
            .map_or_else(
                || Ok(self.profile().clone()),
                |model| profile_with_model_limits(self.profile(), model, output_tokens),
            )
    }
    fn prepare_context<'a>(
        &'a self,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<(), ProviderCoreError>> {
        Box::pin(async move {
            self.ready(cancel).await?;
            let fresh = self
                .catalog
                .lock()
                .map_err(|_| auth("ChatGPT catalog owner is unavailable"))?
                .as_ref()
                .is_some_and(|(when, _)| when.elapsed() < Duration::from_mins(5));
            if !fresh {
                let models = self.account.models(cancel).await?;
                *self.catalog.lock().map_err(|_| auth("ChatGPT catalog owner is unavailable"))? =
                    Some((Instant::now(), models));
            }
            Ok(())
        })
    }
    fn discover_models<'a>(
        &'a self,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Vec<DiscoveredModel>, ProviderCoreError>> {
        Box::pin(self.account.models(cancel))
    }
    fn start(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async move {
            self.ready(&cancel).await?;
            self.inner.start(request, cancel).await
        })
    }
}
impl std::fmt::Debug for ChatGptProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatGptProvider").field("profile", self.profile()).finish_non_exhaustive()
    }
}
async fn access_token(
    path: &Path,
    cancel: &CancellationToken,
) -> Result<Zeroizing<Vec<u8>>, ProviderCoreError> {
    let _ownership = store::lock(path, cancel).await?;
    let mut session = store::load(path)?;
    if !session.permits_inference() {
        return Err(auth(
            "ChatGPT plan usage is not authorized for this registration; sign in and grant plan access",
        ));
    }
    let current = now()?;
    if session.expires_at <= current.saturating_add(90) && current >= session.earliest_refresh_at {
        if let Err(error) = network::refresh(&mut session, cancel).await {
            if error.operation() == "chatgpt_reauthorize" {
                session.access_token.zeroize();
                session.refresh_token.zeroize();
                store::save(path, &session)?;
            }
            return Err(error);
        }
        store::save(path, &session)?;
    }
    if session.expires_at <= now()? {
        return Err(auth("ChatGPT token expired before renewal was available; sign in again"));
    }
    if !session.permits_inference() {
        return Err(auth("ChatGPT plan permission is no longer granted"));
    }
    Ok(Zeroizing::new(session.access_token.as_bytes().to_vec()))
}
impl ChatGptAccount {
    /// Clears local tokens, retaining registration identity for future sign-in.
    /// Returns whether remote revocation was confirmed; local sign-out still occurs on failure.
    ///
    /// # Errors
    /// Rejects unsafe storage or interrupted ownership acquisition before local publication.
    pub async fn sign_out(&self, cancel: &CancellationToken) -> Result<bool, ProviderCoreError> {
        let _ownership = store::lock(&self.path, cancel).await?;
        let mut session = store::load(&self.path)?;
        let revoked =
            session.refresh_token.is_empty() || network::revoke(&session, cancel).await.is_ok();
        session.access_token.zeroize();
        session.refresh_token.zeroize();
        session.id_token.zeroize();
        session.scopes.clear();
        session.expires_at = 0;
        session.earliest_refresh_at = 0;
        store::save(&self.path, &session)?;
        Ok(revoked)
    }
    /// Fetches this account's current provider-advertised model choices using a renewed token.
    ///
    /// # Errors
    /// Returns safe authentication, refresh, metadata or cancellation failures; no bundled list.
    pub async fn models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, ProviderCoreError> {
        let token = access_token(&self.path, cancel).await?;
        let token =
            std::str::from_utf8(&token).map_err(|_| auth("ChatGPT access token is not UTF-8"))?;
        let value = network::catalog(token, cancel).await?;
        let rows = value["models"]
            .as_array()
            .ok_or_else(|| auth("ChatGPT account catalog has no model array"))?;
        if rows.len() > MAX_CATALOG_MODELS {
            return Err(auth("ChatGPT account catalog exceeds its model bound"));
        }
        let mut models = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for row in rows.iter().filter(|row| row["visibility"] == "list") {
            let id =
                row["slug"].as_str().ok_or_else(|| auth("ChatGPT catalog model has no slug"))?;
            let label = row["display_name"]
                .as_str()
                .ok_or_else(|| auth("ChatGPT catalog model has no display name"))?;
            if !seen.insert(id.to_owned()) {
                return Err(auth("ChatGPT catalog repeats a model identity"));
            }
            let mut model = DiscoveredModel::new(id.to_owned(), label.to_owned())?;
            model.dialect = Some(WireDialect::OpenAiResponses);
            model.tools = row["supports_tools"].as_bool();
            model.context_tokens = row["context_window"].as_u64();
            model.input_tokens = row["max_input_tokens"].as_u64();
            model.output_tokens = row["max_output_tokens"].as_u64();
            models.push(model);
        }
        if models.is_empty() {
            return Err(auth("No ChatGPT-plan models are advertised for this account"));
        }
        Ok(models)
    }
}
