//! Configuration-carrying command-runtime shape for the existing Verus composition API.

use crate::ProductRunnerError;
use peritus_process::ProcessStore;
use peritus_types::RunId;
use std::path::PathBuf;

/// Fully resolved daemon input for one product run.
#[derive(Clone, Debug)]
pub struct CommandRuntime {
    local_context: crate::LocalContextConfig,
}

impl CommandRuntime {
    /// Retains the ordinary explicit direct-folder construction boundary in the Verus API model.
    ///
    /// # Errors
    /// The ordinary implementation reports invalid roots or runtime construction failures.
    pub fn open_direct(
        state_root: impl Into<PathBuf>,
        workspace_root: impl Into<PathBuf>,
        run_id: RunId,
        process_store: ProcessStore,
    ) -> Result<Self, ProductRunnerError> {
        Self::open(state_root, workspace_root, run_id, process_store)
    }
    /// Preserves the ordinary command-runtime construction boundary in the Verus API model.
    ///
    /// The effectful implementation validates the roots and constructs the C4/C2 runtime. The
    /// verified API carries the already-resolved value across the daemon composition boundary.
    ///
    /// # Errors
    ///
    /// The ordinary implementation reports invalid roots or runtime construction failures.
    pub fn open(
        state_root: impl Into<PathBuf>,
        workspace_root: impl Into<PathBuf>,
        run_id: RunId,
        process_store: ProcessStore,
    ) -> Result<Self, ProductRunnerError> {
        let _ = (state_root.into(), workspace_root.into(), run_id, process_store);
        Ok(Self { local_context: crate::LocalContextConfig::default() })
    }

    /// Validates and retains the same local context configuration as the runtime implementation.
    ///
    /// # Errors
    /// Rejects invalid local context policy before execution.
    pub fn with_local_context(
        mut self,
        config: crate::LocalContextConfig,
    ) -> Result<Self, ProductRunnerError> {
        config.validate()?;
        self.local_context = config;
        Ok(self)
    }
}
