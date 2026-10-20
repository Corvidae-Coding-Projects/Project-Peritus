//! G0/B1 authority mediation hook for untrusted plugin invocations.

use std::{future::Future, pin::Pin};

use peritus_plugin_sdk::{CapabilityDeclaration, PluginId, PluginOperation, PluginRole};
use peritus_policy::{ActorRole, OperationClass};

use crate::HostError;

/// Sendable borrowed future returned by host integration traits.
pub type HostFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Authenticated invocation subject supplied by G0.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationSubject {
    session_id: String,
    actor_id: String,
    authority_generation: u64,
}

impl InvocationSubject {
    /// Creates an authenticated subject projection.
    #[must_use]
    pub fn new(
        session_id: impl Into<String>,
        actor_id: impl Into<String>,
        authority_generation: u64,
    ) -> Self {
        Self { session_id: session_id.into(), actor_id: actor_id.into(), authority_generation }
    }

    /// Borrows the authenticated session identity.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Borrows the authenticated actor identity.
    #[must_use]
    pub fn actor_id(&self) -> &str {
        &self.actor_id
    }

    /// Returns the current daemon authority generation.
    #[must_use]
    pub const fn authority_generation(&self) -> u64 {
        self.authority_generation
    }
}

/// Exact request presented to the daemon-owned authority mediator.
#[derive(Clone, Copy)]
pub struct AuthorityRequest<'a> {
    plugin_id: &'a PluginId,
    capability: &'a CapabilityDeclaration,
    subject: &'a InvocationSubject,
}

impl<'a> AuthorityRequest<'a> {
    pub(crate) const fn new(
        plugin_id: &'a PluginId,
        capability: &'a CapabilityDeclaration,
        subject: &'a InvocationSubject,
    ) -> Self {
        Self { plugin_id, capability, subject }
    }

    /// Borrows the exact plugin identity.
    #[must_use]
    pub const fn plugin_id(self) -> &'a PluginId {
        self.plugin_id
    }

    /// Borrows the exact declared capability.
    #[must_use]
    pub const fn capability(self) -> &'a CapabilityDeclaration {
        self.capability
    }

    /// Borrows the authenticated invocation subject.
    #[must_use]
    pub const fn subject(self) -> &'a InvocationSubject {
        self.subject
    }

    /// Returns the non-configurable B1 role at this boundary.
    #[must_use]
    pub const fn actor_role(self) -> ActorRole {
        ActorRole::Plugin
    }

    /// Returns the B1 operation projection for the declaration.
    #[must_use]
    pub const fn operation_class(self) -> OperationClass {
        match self.capability.operation() {
            PluginOperation::Inspection => OperationClass::Inspection,
            PluginOperation::WorkspaceMutation => OperationClass::WorkspaceMutation,
            PluginOperation::Execution => OperationClass::Execution,
            PluginOperation::Network => OperationClass::Network,
            PluginOperation::SecretUse => OperationClass::SecretUse,
            PluginOperation::ExternalSideEffect => OperationClass::ExternalSideEffect,
        }
    }
}

/// Current authority facts returned by the daemon-owned mediator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationGrant {
    granted_capabilities: Vec<String>,
    deadline_millis: u64,
}

impl InvocationGrant {
    /// Creates a current authority observation for the host.
    ///
    /// The constructor does not create a B1 capability. Implementations must call it only after
    /// checking current committed authority and should return the exact narrowed capability set.
    #[must_use]
    pub const fn observed(granted_capabilities: Vec<String>, deadline_millis: u64) -> Self {
        Self { granted_capabilities, deadline_millis }
    }

    /// Borrows exact granted capability names.
    #[must_use]
    pub fn granted_capabilities(&self) -> &[String] {
        &self.granted_capabilities
    }

    /// Returns the authority-bounded monotonic deadline.
    #[must_use]
    pub const fn deadline_millis(&self) -> u64 {
        self.deadline_millis
    }

    pub(crate) const fn role() -> PluginRole {
        PluginRole::Plugin
    }
}

/// Authority mediator result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthorityDecision {
    /// Current committed authority permits the exact request.
    Authorized(InvocationGrant),
    /// Current committed authority rejected the exact request.
    Denied {
        /// Stable denial code.
        code: String,
        /// Bounded user-safe explanation.
        detail: String,
    },
}

/// Effect-free adapter to the existing G0/B1 authorization surface.
pub trait AuthorityMediator: Send + Sync {
    /// Checks current authority for one exact plugin/capability/subject tuple.
    fn authorize<'a>(
        &'a self,
        request: AuthorityRequest<'a>,
    ) -> HostFuture<'a, Result<AuthorityDecision, HostError>>;
}
