//! Deterministic configured registry and sole stateful C4 router owner.

use std::{fmt, sync::Arc};

use peritus_policy::{ActorRole, AuthorityInstant, CapabilityScope, OperationRegistry};
use peritus_tool_protocol::{
    CancellationReason, PreparedToolCall, SemanticVersion, ToolCall, ToolControl, ToolDescriptor,
};
use peritus_tool_router::{
    DispatchOutcome, ExecutionUpdate, ExposedTools, InvocationHandle, RecoveryOutcome,
    RouterLimits, ToolAuthorizationRequest, ToolDispatcher, ToolRegistry, ToolRouter,
};
use peritus_types::CapabilityName;

use super::{
    ToolComponentError, ToolComponentErrorKind, ToolDispatcherRoute,
    catalog::production_catalog,
    selection::{checked_names, operation_registry, select},
};

const MAX_CONFIGURED_TOOLS: usize = 256;

/// Immutable exact descriptor and its production dispatcher constructor route.
#[derive(Clone, Debug)]
pub struct ToolRegistration {
    descriptor: Arc<ToolDescriptor>,
    route: ToolDispatcherRoute,
}

impl ToolRegistration {
    pub(super) const fn new(descriptor: Arc<ToolDescriptor>, route: ToolDispatcherRoute) -> Self {
        Self { descriptor, route }
    }

    /// Borrows the immutable canonical descriptor.
    #[must_use]
    pub fn descriptor(&self) -> &ToolDescriptor {
        self.descriptor.as_ref()
    }

    /// Returns the route used to construct its scoped production dispatcher.
    #[must_use]
    pub const fn route(&self) -> ToolDispatcherRoute {
        self.route
    }
}

/// One concrete, scoped dispatcher paired with its declared construction route.
pub struct DispatcherBinding<'dispatcher> {
    route: ToolDispatcherRoute,
    dispatcher: &'dispatcher mut dyn ToolDispatcher,
}

impl<'dispatcher> DispatcherBinding<'dispatcher> {
    /// Pairs a concrete C4 adapter with the explicit route used to construct it.
    #[must_use]
    pub const fn new(
        route: ToolDispatcherRoute,
        dispatcher: &'dispatcher mut dyn ToolDispatcher,
    ) -> Self {
        Self { route, dispatcher }
    }
}

/// Configured immutable inventory plus bounded stateful C4 routing.
pub struct ToolComponents {
    registrations: Vec<ToolRegistration>,
    operations: OperationRegistry,
    router: Option<ToolRouter>,
}

impl fmt::Debug for ToolComponents {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ToolComponents")
            .field("registrations", &self.registrations)
            .field("operations", &self.operations)
            .field("router_configured", &self.router.is_some())
            .finish_non_exhaustive()
    }
}

impl ToolComponents {
    /// Selects configured names from the closed production catalog and constructs exact C4 state.
    ///
    /// An empty allowlist is valid and exposes no tool. Descriptor catalogs are compiled factories;
    /// this function performs no workspace, executable, plugin, or environment discovery.
    ///
    /// # Errors
    ///
    /// Rejects repeated/unknown names, catalog drift, invalid exact B1 operations, malformed C4
    /// descriptors, or an allowlist larger than the production bound.
    pub fn build(
        allowed: &[String],
        router_limits: RouterLimits,
    ) -> Result<Self, ToolComponentError> {
        if allowed.len() > MAX_CONFIGURED_TOOLS {
            return Err(ToolComponentError::new(
                ToolComponentErrorKind::Capacity,
                "construct configured tool inventory",
                "configured tool count exceeds the production bound",
            ));
        }
        let selected_names = checked_names(allowed)?;
        let catalog = production_catalog()?;
        let registrations = select(catalog, &selected_names)?;
        let operations = operation_registry(&registrations)?;
        let router = if registrations.is_empty() {
            None
        } else {
            let descriptors = registrations
                .iter()
                .map(|registration| Arc::clone(&registration.descriptor))
                .collect();
            let registry = ToolRegistry::new(descriptors, &operations).map_err(|error| {
                ToolComponentError::new(
                    ToolComponentErrorKind::ToolRegistry,
                    "construct configured tool registry",
                    error.to_string(),
                )
            })?;
            Some(ToolRouter::new(registry, router_limits))
        };
        Ok(Self { registrations, operations, router })
    }

    /// Returns the number of configured exact tool descriptors.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.registrations.len()
    }

    /// Returns whether the explicit allowlist is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.registrations.is_empty()
    }

    /// Reports whether one canonical capability name is explicitly configured.
    #[must_use]
    pub fn contains_name(&self, name: &str) -> bool {
        self.registrations
            .binary_search_by(|registration| registration.descriptor.name().as_str().cmp(name))
            .is_ok()
    }

    /// Returns configured names in strict canonical order.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.registrations
            .iter()
            .map(|registration| registration.descriptor.name().as_str())
            .collect()
    }

    /// Borrows configured registrations in strict name/version order.
    #[must_use]
    pub fn registrations(&self) -> &[ToolRegistration] {
        &self.registrations
    }

    /// Borrows the exact canonical B1 operations backing the C4 registry.
    #[must_use]
    pub const fn operations(&self) -> &OperationRegistry {
        &self.operations
    }

    /// Borrows the immutable C4 registry, or `None` for an empty allowlist.
    #[must_use]
    pub fn registry(&self) -> Option<&ToolRegistry> {
        self.router.as_ref().map(ToolRouter::registry)
    }

    /// Looks up one exact configured name and semantic version.
    #[must_use]
    pub fn registration(
        &self,
        name: &CapabilityName,
        version: SemanticVersion,
    ) -> Option<&ToolRegistration> {
        self.registrations
            .binary_search_by(|registration| {
                (registration.descriptor.name().as_str(), registration.descriptor.version())
                    .cmp(&(name.as_str(), version))
            })
            .ok()
            .map(|index| &self.registrations[index])
    }

    /// Returns the exact dispatcher route for a registered prepared call.
    ///
    /// # Errors
    ///
    /// Rejects calls not prepared from the configured descriptor digest and implementation.
    pub fn route_for(
        &self,
        prepared: &PreparedToolCall,
    ) -> Result<ToolDispatcherRoute, ToolComponentError> {
        self.exact_registration(prepared).map(ToolRegistration::route)
    }

    /// Computes role/capability exposure, returning `None` for an empty configured registry.
    ///
    /// # Errors
    ///
    /// Preserves the C4 router's exact exposure rejection.
    pub fn exposed(
        &self,
        role: ActorRole,
        scope: &CapabilityScope,
    ) -> Result<Option<ExposedTools>, ToolComponentError> {
        self.router
            .as_ref()
            .map(|router| router.exposed(role, scope).map_err(ToolComponentError::router))
            .transpose()
    }

    /// Performs effect-free lookup, schema validation, and exact descriptor-bound preparation.
    ///
    /// # Errors
    ///
    /// Rejects empty configuration or preserves the C4 preparation rejection.
    pub fn prepare(&self, call: ToolCall) -> Result<PreparedToolCall, ToolComponentError> {
        self.router()?.prepare(call).map_err(ToolComponentError::router)
    }

    /// Dispatches after checking the concrete adapter's configured route and descriptor digest.
    ///
    /// # Errors
    ///
    /// Rejects unregistered prepared calls and route/implementation mismatches before forwarding
    /// the move-only invocation to the authoritative C4 router.
    pub fn dispatch(
        &mut self,
        prepared: PreparedToolCall,
        request: &ToolAuthorizationRequest<'_>,
        binding: DispatcherBinding<'_>,
    ) -> Result<DispatchOutcome, ToolComponentError> {
        let registration = self.exact_registration(&prepared)?;
        if registration.route != binding.route
            || registration.descriptor.implementation_identity()
                != binding.dispatcher.implementation_identity()
            || registration.descriptor.descriptor_digest() != binding.dispatcher.descriptor_digest()
        {
            return Err(ToolComponentError::new(
                ToolComponentErrorKind::DispatcherMismatch,
                "bind configured tool dispatcher",
                "dispatcher route, implementation identity, or descriptor digest differs",
            ));
        }
        self.router_mut()?
            .dispatch(prepared, request, binding.dispatcher)
            .map_err(ToolComponentError::router)
    }

    /// Polls one daemon-owned active tool execution.
    ///
    /// # Errors
    ///
    /// Rejects an empty registry, unknown/mismatched handle, deadline failure, or malformed
    /// execution observation using the underlying C4 classification.
    pub fn poll(
        &mut self,
        handle: InvocationHandle,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, ToolComponentError> {
        self.router_mut()?.poll(handle, observed_at).map_err(ToolComponentError::router)
    }

    /// Applies one descriptor-supported non-cancellation control.
    ///
    /// # Errors
    ///
    /// Rejects an empty registry, unknown/mismatched handle, unsupported control, or malformed
    /// execution observation using the underlying C4 classification.
    pub fn control(
        &mut self,
        handle: InvocationHandle,
        control: ToolControl,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, ToolComponentError> {
        self.router_mut()?.control(handle, control, observed_at).map_err(ToolComponentError::router)
    }

    /// Requests cancellation while retaining router ownership until terminal observation.
    ///
    /// # Errors
    ///
    /// Rejects an empty registry, unknown/mismatched handle, or malformed cancellation observation
    /// using the underlying C4 classification.
    pub fn cancel(
        &mut self,
        handle: InvocationHandle,
        reason: CancellationReason,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, ToolComponentError> {
        self.router_mut()?.cancel(handle, reason, observed_at).map_err(ToolComponentError::router)
    }

    /// Reconciles one active invocation after daemon observation loss.
    ///
    /// # Errors
    ///
    /// Rejects an empty registry, unknown/mismatched handle, or malformed recovery observation
    /// using the underlying C4 classification.
    pub fn recover(
        &mut self,
        handle: InvocationHandle,
        observed_at: AuthorityInstant,
    ) -> Result<RecoveryOutcome, ToolComponentError> {
        self.router_mut()?.recover(handle, observed_at).map_err(ToolComponentError::router)
    }

    fn exact_registration(
        &self,
        prepared: &PreparedToolCall,
    ) -> Result<&ToolRegistration, ToolComponentError> {
        let descriptor = prepared.descriptor();
        let registration = self
            .registration(descriptor.name(), descriptor.version())
            .ok_or_else(unregistered_call)?;
        if registration.descriptor.descriptor_digest() != descriptor.descriptor_digest()
            || registration.descriptor.implementation_identity()
                != descriptor.implementation_identity()
        {
            return Err(unregistered_call());
        }
        Ok(registration)
    }

    fn router(&self) -> Result<&ToolRouter, ToolComponentError> {
        self.router.as_ref().ok_or_else(no_tools)
    }

    fn router_mut(&mut self) -> Result<&mut ToolRouter, ToolComponentError> {
        self.router.as_mut().ok_or_else(no_tools)
    }
}

fn no_tools() -> ToolComponentError {
    ToolComponentError::new(
        ToolComponentErrorKind::NoToolsConfigured,
        "route configured tool",
        "the explicit tool allowlist is empty",
    )
}

fn unregistered_call() -> ToolComponentError {
    ToolComponentError::new(
        ToolComponentErrorKind::UnregisteredCall,
        "route configured tool",
        "prepared call is not bound to an exact configured descriptor",
    )
}
