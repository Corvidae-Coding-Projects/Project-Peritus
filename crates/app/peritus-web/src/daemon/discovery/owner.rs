//! Durable native target and session ownership.

use super::{Discovery, profile_for_workspace};
use crate::{
    daemon::{AppRequestPayload, AppResponsePayload, Client, bytes, hex},
    error::{Result, problem},
    state::{App, Project},
};
use peritus_app_protocol::WellKnownProtocolFeature;
use peritus_types::{SessionId, WorkspaceId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeTarget {
    pub(super) generation: u64,
    pub(super) store: String,
    pub(super) config: PathBuf,
    pub(super) config_sha256: String,
    pub(super) product_state: PathBuf,
    pub(super) product_state_sha256: String,
    pub(super) endpoint: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeOwner {
    #[serde(flatten)]
    target: NativeTarget,
    workspace: String,
    native_session: String,
}

pub(crate) struct NativeConnection {
    target: NativeTarget,
    native_session: String,
    state: peritus_product_state::ProductState,
}

#[derive(Clone)]
pub(crate) struct RetainedProject {
    pub(crate) root: PathBuf,
}

impl NativeTarget {
    pub(crate) fn endpoint(&self) -> &Path {
        &self.endpoint
    }

    pub(crate) fn store(&self) -> &str {
        &self.store
    }

    pub(super) fn verify(&self) -> Result<Discovery> {
        super::persistence::verify_target(self)
    }

    pub(crate) fn retained_project(&self, workspace: &str) -> Result<RetainedProject> {
        let discovery = self.verify()?;
        let profile = profile_for_workspace(&discovery.state, workspace).ok_or_else(|| {
            problem("The retained native workspace is absent from its product generation")
        })?;
        Ok(RetainedProject {root: PathBuf::from(profile.repository_root())})
    }

    pub(crate) async fn retained_project_async(&self, workspace: &str) -> Result<RetainedProject> {
        let target = self.clone();
        let workspace = workspace.to_owned();
        tokio::task::spawn_blocking(move || target.retained_project(&workspace)).await.map_err(problem)?
    }

    fn project_facts(&self, project: &Project) -> Result<Value> {
        let discovery = self.verify()?;
        super::facts_from_discovery(&discovery, project)
    }
}

impl NativeOwner {
    pub(crate) fn decode(value: &Value) -> Result<Self> {
        let owner: Self = serde_json::from_value(value.clone()).map_err(problem)?;
        owner.validate_shape()?;
        Ok(owner)
    }

    pub(crate) fn retained(&self) -> Result<Value> {
        serde_json::to_value(self).map_err(Into::into)
    }

    pub(crate) fn target(&self) -> &NativeTarget {
        &self.target
    }

    pub(crate) fn workspace(&self) -> &str {
        &self.workspace
    }

    pub(crate) fn same_connection(&self, other: &Self) -> bool {
        self.target == other.target && self.native_session == other.native_session
    }

    pub(crate) fn session_id(&self) -> Result<SessionId> {
        SessionId::new(bytes(&self.native_session)?)
            .map_err(|error| problem(format!("{error:?}")))
    }

    pub(crate) fn validate(&self) -> Result<()> {
        self.validate_shape()?;
        let state = self.target.verify()?.state;
        if profile_for_workspace(&state, &self.workspace).is_none() {
            return Err(problem(
                "The native owner workspace is absent from its product generation",
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_shape(&self) -> Result<()> {
        let store = bytes(&self.target.store)?;
        let workspace = WorkspaceId::new(bytes(&self.workspace)?)
            .map_err(|error| problem(format!("{error:?}")))?;
        let session = self.session_id()?;
        if self.target.generation == 0
            || self.target.config.as_os_str().is_empty()
            || self.target.product_state.as_os_str().is_empty()
            || self.target.endpoint.as_os_str().is_empty()
            || !canonical_digest(&self.target.config_sha256)
            || !canonical_digest(&self.target.product_state_sha256)
            || store == [0; 16]
            || hex(&store) != self.target.store
            || hex(workspace.as_bytes()) != self.workspace
            || hex(session.as_bytes()) != self.native_session
        {
            return Err(problem("The retained native owner binding is malformed"));
        }
        Ok(())
    }

    async fn validate_async(&self) -> Result<()> {
        let owner = self.clone();
        tokio::task::spawn_blocking(move || owner.validate()).await.map_err(problem)?
    }

    pub(crate) fn facts(&self, project: &Project) -> Result<Value> {
        let facts = self.target.project_facts(project)?;
        if facts["workspace"]["id"] != self.workspace {
            return Err(problem(
                "The browser project does not belong to its retained native owner",
            ));
        }
        Ok(facts)
    }

    pub(crate) async fn facts_async(&self, project: &Project) -> Result<Value> {
        let owner = self.clone();
        let project = project.clone();
        tokio::task::spawn_blocking(move || owner.facts(&project)).await.map_err(problem)?
    }
}

impl NativeConnection {
    pub(super) fn new(target: NativeTarget, native_session: String, state: peritus_product_state::ProductState) -> Self {
        Self {target, native_session, state}
    }

    pub(crate) fn bind(self, workspace: WorkspaceId) -> Result<NativeOwner> {
        if profile_for_workspace(&self.state, &hex(workspace.as_bytes())).is_none() {
            return Err(problem("The native workspace is absent from its retained product generation"));
        }
        let owner = NativeOwner {
            target: self.target,
            workspace: hex(workspace.as_bytes()),
            native_session: self.native_session,
        };
        owner.validate_shape()?;
        Ok(owner)
    }
}

pub(crate) async fn raw_request_target(
    app: &App,
    target: &NativeTarget,
    payload: AppRequestPayload,
) -> Result<(AppResponsePayload, NativeConnection)> {
    let retained = target.clone();
    let discovery = tokio::task::spawn_blocking(move || retained.verify()).await.map_err(problem)??;
    let required = payload
        .required_workbench_feature()
        .into_iter()
        .collect::<Vec<_>>();
    let mut client =
        super::super::connection::connect(app, target.endpoint(), &required, None).await?;
    let connection = NativeConnection::new(
        target.clone(),
        hex(client.context().session_id().as_bytes()),
        discovery.state,
    );
    let identity = Client::new_request_identity().map_err(problem)?;
    let response = client.request(identity, payload).await.map_err(problem)?;
    Ok((response.payload().clone(), connection))
}

pub(crate) async fn raw_request_owned(
    app: &App,
    owner: &NativeOwner,
    payload: AppRequestPayload,
) -> Result<AppResponsePayload> {
    let required = payload
        .required_workbench_feature()
        .into_iter()
        .collect::<Vec<_>>();
    let mut client = connect_owned(app, owner, &required).await?;
    let identity = Client::new_request_identity().map_err(problem)?;
    let response = client.request(identity, payload).await.map_err(problem)?;
    Ok(response.payload().clone())
}

pub(crate) async fn connect_owned(
    app: &App,
    owner: &NativeOwner,
    required: &[WellKnownProtocolFeature],
) -> Result<Client> {
    owner.validate_async().await?;
    let expected = owner.session_id()?;
    let client = super::super::connection::connect(
        app,
        owner.target.endpoint(),
        required,
        Some(expected),
    )
    .await?;
    if client.context().session_id() != expected {
        return Err(problem(
            "The daemon did not reconnect the retained native session",
        ));
    }
    Ok(client)
}

fn canonical_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

#[cfg(test)]
mod tests;
