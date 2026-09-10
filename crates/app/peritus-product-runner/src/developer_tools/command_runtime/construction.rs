//! Construction of managed-workspace and explicitly trusted direct-folder command runtimes.

use super::{ARTIFACT_QUOTA_BYTES, CommandRuntime, RuntimeInner, RuntimeState, plan, runtime_open};
use peritus_artifact_store::StoreConfig;
use peritus_policy::{OperationDescriptor, OperationRegistry, RiskSet};
use peritus_process::{ExecutionGateway, ProcessStore};
use peritus_tool_router::{RouterLimits, ToolRegistry, ToolRouter};
use peritus_tools_shell::exec_descriptor;
use peritus_types::RunId;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

impl CommandRuntime {
    /// Creates the run-owned C4 router while reusing the caller's daemon-owned C2 process store.
    ///
    /// # Errors
    /// Returns a product-run failure when the state root overlaps the agent-visible workspace or
    /// the canonical C4 catalog cannot be constructed.
    pub fn open(
        state_root: impl Into<PathBuf>,
        workspace_root: impl Into<PathBuf>,
        run_id: RunId,
        process_store: ProcessStore,
    ) -> Result<Self, crate::ProductRunnerError> {
        Self::open_configured(
            &state_root.into(),
            &workspace_root.into(),
            run_id,
            process_store,
            false,
        )
    }

    /// Opens explicitly trusted in-place folder commands, which retain raw local-user effects.
    ///
    /// Unlike a managed worktree, a home folder can contain the application's private state.
    /// This is not a sandbox or a claim of state isolation; callers must explicitly authorize
    /// raw commands and exclude private paths from their ordinary file-tool surface.
    ///
    /// # Errors
    /// Rejects an unavailable root, a workspace inside the command state, or invalid routing state.
    pub fn open_direct(
        state_root: impl Into<PathBuf>,
        workspace_root: impl Into<PathBuf>,
        run_id: RunId,
        process_store: ProcessStore,
    ) -> Result<Self, crate::ProductRunnerError> {
        Self::open_configured(
            &state_root.into(),
            &workspace_root.into(),
            run_id,
            process_store,
            true,
        )
    }

    fn open_configured(
        state_root: &Path,
        workspace_root: &Path,
        run_id: RunId,
        process_store: ProcessStore,
        direct: bool,
    ) -> Result<Self, crate::ProductRunnerError> {
        std::fs::create_dir_all(state_root).map_err(|error| runtime_open(error.to_string()))?;
        let state_root =
            state_root.canonicalize().map_err(|error| runtime_open(error.to_string()))?;
        let workspace_root =
            workspace_root.canonicalize().map_err(|error| runtime_open(error.to_string()))?;
        if (!direct && state_root.starts_with(&workspace_root))
            || workspace_root.starts_with(&state_root)
        {
            return Err(runtime_open(
                "command state and agent-visible workspace roots overlap".to_owned(),
            ));
        }
        let artifacts = StoreConfig::new(
            state_root.join("artifacts"),
            plan::OUTPUT_BYTES,
            ARTIFACT_QUOTA_BYTES,
        )
        .map_err(|error| runtime_open(error.to_string()))?;
        let descriptor = exec_descriptor().map_err(|error| runtime_open(error.to_string()))?;
        let operation = OperationDescriptor::new(
            descriptor.operation().name().clone(),
            descriptor.operation().operation_class(),
            RiskSet::new(descriptor.operation().risks().as_slice().to_vec())
                .map_err(|error| runtime_open(format!("{error:?}")))?,
        )
        .map_err(|error| runtime_open(format!("{error:?}")))?;
        let operations = OperationRegistry::new(vec![operation])
            .map_err(|error| runtime_open(format!("{error:?}")))?;
        let registry = ToolRegistry::new(vec![Arc::new(descriptor)], &operations)
            .map_err(|error| runtime_open(error.to_string()))?;
        let limits =
            RouterLimits::new(64, 4_096).map_err(|error| runtime_open(error.to_string()))?;
        Ok(Self {
            local_context: crate::LocalContextConfig::default(),
            inner: Arc::new(RuntimeInner {
                run_id,
                workspace_root,
                state_root,
                artifacts,
                gateway: ExecutionGateway::new(process_store),
                state: Mutex::new(RuntimeState {
                    router: ToolRouter::new(registry, limits),
                    next_ordinal: 0,
                    active: BTreeMap::new(),
                    terminal: BTreeMap::new(),
                }),
                #[cfg(test)]
                state_guard: None,
            }),
        })
    }
}
