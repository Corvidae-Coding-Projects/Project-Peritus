//! Observe real composed review retries without changing provider responses or tool execution.
use std::sync::Mutex;

use peritus_agent::{
    DeveloperActivity, DeveloperInput, DeveloperInteraction, DeveloperLoopError,
    DeveloperRequestAdmission, DeveloperReviewRetryReason,
};
use peritus_model_protocol::ModelRequest;
use peritus_product_runner::ConversationView;

pub struct ObservedConversation {
    task: String,
    pub(super) retries: Mutex<Vec<(u8, u8, DeveloperReviewRetryReason)>>,
}

impl ObservedConversation {
    pub(super) const fn new(task: String) -> Self {
        Self { task, retries: Mutex::new(Vec::new()) }
    }
}

impl ConversationView for ObservedConversation {
    fn revision(&self) -> u64 {
        1
    }

    fn render(&self) -> String {
        self.task.clone()
    }

    fn interaction(&self) -> Option<&dyn DeveloperInteraction> {
        Some(self)
    }
}

impl DeveloperInteraction for ObservedConversation {
    fn input(&self) -> Result<DeveloperInput, DeveloperLoopError> {
        Ok(DeveloperInput { revision: 1, conversation: self.task.clone(), images: Vec::new() })
    }

    fn prepare_request(
        &self,
        revision: u64,
        _: &ModelRequest,
    ) -> Result<DeveloperRequestAdmission, DeveloperLoopError> {
        assert_eq!(revision, 1);
        Ok(DeveloperRequestAdmission::Accepted)
    }

    fn observe(&self, activity: DeveloperActivity<'_>) -> Result<(), DeveloperLoopError> {
        if let DeveloperActivity::ReviewRetry { next_attempt, max_attempts, reason } = activity {
            self.retries.lock().expect("retry observations").push((
                next_attempt,
                max_attempts,
                reason,
            ));
        }
        Ok(())
    }
}
