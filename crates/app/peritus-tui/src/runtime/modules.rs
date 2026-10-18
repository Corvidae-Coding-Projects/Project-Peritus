//! One resource-bounded local catalog operation; rendering and heartbeats stay responsive.
use crate::action::Action;
use peritus_modules::ModuleOperation;
use tokio::task::JoinSet;
#[derive(Default)]
pub(super) struct ModuleOperations {
    jobs: JoinSet<(u64, Result<serde_json::Value, String>)>,
}
impl ModuleOperations {
    pub(super) fn active(&self) -> bool {
        !self.jobs.is_empty()
    }
    pub(super) fn start(&mut self, generation: u64, operation: ModuleOperation) -> bool {
        if self.active() {
            return false;
        }
        self.jobs
            .spawn_blocking(move || (generation, operation.execute().map_err(|e| e.to_string())));
        true
    }
    pub(super) async fn next(&mut self) -> Action {
        match self.jobs.join_next().await {
            Some(Ok((generation, result))) => Action::ModuleResult { generation, result },
            _ => Action::ModuleResult {
                generation: 0,
                result: Err(
                    "Module worker failed. Reopen the market to inspect installed state.".into()
                ),
            },
        }
    }
}
