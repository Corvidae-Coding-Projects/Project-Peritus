//! A stage may not consume steering that requires leaving its pinned task.
use peritus_agent::{
    DeveloperActivity, DeveloperControlFlow, DeveloperInput, DeveloperInteraction,
    DeveloperLoopError, DeveloperModelRole, DeveloperRequestAdmission, DeveloperToolEffect,
};
use peritus_model_protocol::{CompletedToolCall, MediaInput, ModelRequest, UsageCounters};

pub(super) struct StageInteraction<'a> {
    pub(super) inner: &'a dyn DeveloperInteraction,
    pub(super) revision: u64,
}
impl DeveloperInteraction for StageInteraction<'_> {
    fn input(&self) -> Result<DeveloperInput, DeveloperLoopError> {
        let input = self.inner.input()?;
        if input.revision != self.revision {
            return Err(DeveloperLoopError::Context(
                "new user input paused the module; steering remains pending for the conversation"
                    .into(),
            ));
        }
        Ok(input)
    }
    fn prepare_request(
        &self,
        revision: u64,
        request: &ModelRequest,
    ) -> Result<DeveloperRequestAdmission, DeveloperLoopError> {
        self.input()?;
        self.inner.prepare_request(revision, request)
    }
    fn prepare_role_request(
        &self,
        role: DeveloperModelRole,
        revision: u64,
        request: &ModelRequest,
    ) -> Result<DeveloperRequestAdmission, DeveloperLoopError> {
        self.input()?;
        self.inner.prepare_role_request(role, revision, request)
    }
    fn complete_role_request(
        &self,
        role: DeveloperModelRole,
        id: &str,
        usage: UsageCounters,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        self.inner.complete_role_request(role, id, usage)
    }
    fn provider(
        &self,
        role: DeveloperModelRole,
    ) -> Result<Option<std::sync::Arc<dyn peritus_provider_core::ModelProvider>>, DeveloperLoopError>
    {
        self.inner.provider(role)
    }
    fn allows_semantic_compaction(&self) -> bool {
        self.inner.allows_semantic_compaction()
    }
    fn admit_tool(
        &self,
        role: DeveloperModelRole,
        invocation: &str,
        sequence: u32,
        revision: u64,
        effect: DeveloperToolEffect,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        self.input()?;
        self.inner.admit_tool(role, invocation, sequence, revision, effect)
    }
    fn complete_tool(
        &self,
        role: DeveloperModelRole,
        invocation: &str,
        sequence: u32,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        self.inner.complete_tool(role, invocation, sequence)
    }
    fn record_observation_images(
        &self,
        call: &CompletedToolCall,
        images: &[MediaInput],
    ) -> Result<(), DeveloperLoopError> {
        self.inner.record_observation_images(call, images)
    }
    fn observe(&self, activity: DeveloperActivity<'_>) -> Result<(), DeveloperLoopError> {
        self.inner.observe(activity)
    }
}
