//! Serialized request exchanges with exact response binding.

use peritus_app_protocol::{
    AppMessage, AppRequestEnvelope, AppRequestPayload, AppResponseEnvelope,
};

use crate::{ClientError, RequestIdentity, connection::Client};

impl Client {
    /// Allocates a transport identity that callers can retain before submission.
    ///
    /// # Errors
    /// Returns an error when the operating system cannot generate identifiers.
    pub fn new_request_identity() -> Result<RequestIdentity, ClientError> {
        RequestIdentity::generate()
    }

    /// Sends one request and waits for its exactly matching response.
    ///
    /// Heartbeats are handled while waiting. Other unsolicited frames are errors;
    /// subscriptions belong on separate connections. The timeout includes writing
    /// the request. A failure or cancellation invalidates the connection and does
    /// not establish whether the daemon performed the requested effect.
    ///
    /// # Errors
    /// Returns transport, timeout, or protocol errors. Callers must reconcile the
    /// original operation receipt before retrying an uncertain mutation.
    pub async fn request(
        &mut self,
        identity: RequestIdentity,
        payload: AppRequestPayload,
    ) -> Result<AppResponseEnvelope, ClientError> {
        if let Some(required) = payload.required_workbench_feature()
            && !self.supports(required)
        {
            return Err(ClientError::protocol(
                "encode application request",
                format!(
                    "daemon has not negotiated required capability: {}",
                    required.as_str()
                ),
            ));
        }
        if let AppRequestPayload::WorkbenchCommand(command)
        | AppRequestPayload::QueryWorkbenchReceipt(command) = &payload
        {
            let wide = match command.intent() {
                peritus_app_protocol::WorkbenchIntent::CreateCheckpoint(name) => {
                    name.requires_manifest_feature()
                }
                peritus_app_protocol::WorkbenchIntent::ApplyRewind(preview) => {
                    preview.requires_manifest_feature()
                }
                _ => false,
            };
            if wide
                && !self.supports(
                    peritus_app_protocol::WellKnownProtocolFeature::WorkbenchCheckpointManifests,
                )
            {
                return Err(ClientError::protocol(
                    "encode checkpoint command",
                    "daemon has not negotiated complete checkpoint manifests",
                ));
            }
        }
        let request = AppRequestEnvelope::new(
            self.context,
            identity.request_id,
            identity.correlation_id,
            payload,
        )?;
        self.begin_exchange()?;
        let response = crate::optional_timeout(self.timeout, async {
            self.stream.write(&AppMessage::Request(request)).await?;
            loop {
                match self.stream.read().await? {
                    AppMessage::Response(response) => {
                        validate_response(&response, self.context, identity)?;
                        return Ok(response);
                    }
                    AppMessage::Event(event) => {
                        if !self.reply_heartbeat_inner(&event).await? {
                            return Err(ClientError::protocol(
                                "await daemon response",
                                "received an unsolicited event on the request connection",
                            ));
                        }
                    }
                    _ => {
                        return Err(ClientError::protocol(
                            "await daemon response",
                            "received an unexpected application frame",
                        ));
                    }
                }
            }
        })
        .await
        .map_err(|_| {
            ClientError::connection(
                "await daemon response",
                "request timed out; reconcile its original operation receipt",
            )
        })??;
        self.usable = true;
        Ok(response)
    }
}

fn validate_response(
    response: &AppResponseEnvelope,
    context: peritus_app_protocol::ProtocolContext,
    identity: RequestIdentity,
) -> Result<(), ClientError> {
    if response.context() != context {
        return Err(ClientError::protocol(
            "validate daemon response",
            "response context differs from negotiated context",
        ));
    }
    if response.request_id() != identity.request_id
        || response.correlation_id() != identity.correlation_id
    {
        return Err(ClientError::protocol(
            "validate daemon response",
            "received response for another request",
        ));
    }
    Ok(())
}
