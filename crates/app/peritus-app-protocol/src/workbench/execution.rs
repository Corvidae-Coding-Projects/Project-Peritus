//! Explicit execution settings over an already accepted queue, not an embedded prompt macro.

use crate::{ProductInteractionMode, ProductProviderSelection, ProductRoleModels};
use peritus_types::RunId;

/// Starts the selected durable conversation with immutable initial provider selections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchExecutionSettings {
    run: RunId,
    providers: ProductProviderSelection,
    mode: ProductInteractionMode,
    models: ProductRoleModels,
}
impl WorkbenchExecutionSettings {
    /// Binds checked settings. The host must still admit workspace authority and eligible inputs.
    #[must_use]
    pub const fn new(
        run: RunId,
        providers: ProductProviderSelection,
        mode: ProductInteractionMode,
        models: ProductRoleModels,
    ) -> Self {
        Self { run, providers, mode, models }
    }
    /// Returns the separately identified execution lineage.
    #[must_use]
    pub const fn run(&self) -> RunId {
        self.run
    }
    /// Returns the selected provider routes.
    #[must_use]
    pub const fn providers(&self) -> ProductProviderSelection {
        self.providers
    }
    /// Returns explicit execution semantics; this cannot broaden workspace authority.
    #[must_use]
    pub const fn mode(&self) -> ProductInteractionMode {
        self.mode
    }
    /// Borrows initial role-specific model selections.
    #[must_use]
    pub const fn models(&self) -> &ProductRoleModels {
        &self.models
    }
}
