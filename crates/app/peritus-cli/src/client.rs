//! CLI error adaptation for the shared application client.

use std::{ffi::OsStr, time::Duration};

use peritus_app_protocol::{
    AppEventEnvelope, AppProtocolLimits, AppRequestPayload, AppResponseEnvelope, ControlPayload,
    CorrelationId, ProtocolContext, WellKnownProtocolFeature,
};
use peritus_types::SessionId;

use crate::error::CliError;

pub use peritus_app_client::RequestIdentity;

/// CLI presentation adapter; protocol behavior belongs to the shared client.
pub struct Client {
    inner: peritus_app_client::Client,
}

impl Client {
    pub(crate) async fn connect(
        endpoint: &OsStr,
        requested_session: Option<SessionId>,
        timeout: Duration,
        required: &[WellKnownProtocolFeature],
    ) -> Result<Self, CliError> {
        let inner =
            peritus_app_client::Client::connect(endpoint, requested_session, timeout, required)
                .await?;
        Ok(Self { inner })
    }

    pub(crate) const fn context(&self) -> ProtocolContext {
        self.inner.context()
    }

    pub(crate) const fn limits(&self) -> AppProtocolLimits {
        self.inner.limits()
    }

    pub(crate) fn new_request_identity() -> Result<RequestIdentity, CliError> {
        RequestIdentity::generate().map_err(Into::into)
    }

    pub(crate) async fn request(
        &mut self,
        identity: RequestIdentity,
        payload: AppRequestPayload,
    ) -> Result<AppResponseEnvelope, CliError> {
        self.inner.request(identity, payload).await.map_err(Into::into)
    }

    pub(crate) async fn read_event(&mut self) -> Result<AppEventEnvelope, CliError> {
        self.inner.read_event().await.map_err(Into::into)
    }

    pub(crate) async fn write_control(
        &mut self,
        correlation_id: CorrelationId,
        payload: ControlPayload,
    ) -> Result<(), CliError> {
        self.inner.write_control(correlation_id, payload).await.map_err(Into::into)
    }

    pub(crate) async fn reply_heartbeat(
        &mut self,
        event: &AppEventEnvelope,
    ) -> Result<bool, CliError> {
        self.inner.reply_heartbeat(event).await.map_err(Into::into)
    }
}
