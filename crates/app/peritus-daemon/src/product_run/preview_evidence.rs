//! Exact command and output bindings retained with preview admission.

use peritus_app_protocol::{
    AppErrorCode, AppProtocolError, ControlOperationId, WorkbenchCommand, WorkbenchInputText,
    WorkbenchIntent, WorkbenchLaunchResult, WorkbenchLaunchText, WorkbenchQuery,
};
use peritus_product_runner::{PreviewOutputMatch, PreviewOutputStream};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PreviewBehaviorEvidence {
    pub(super) binding_digest: [u8; 32],
    pub(super) launch: [u8; 16],
    pub(super) observed: String,
    pub(super) note: String,
    pub(super) observed_match: PreviewOutputMatch,
    pub(super) note_match: Option<PreviewOutputMatch>,
    pub(super) goal: Option<PreviewGoalBinding>,
}

impl PreviewBehaviorEvidence {
    pub(super) fn new(
        command: &WorkbenchCommand,
        observed_match: PreviewOutputMatch,
        note_match: Option<PreviewOutputMatch>,
        goal: Option<PreviewGoalBinding>,
    ) -> Result<Self, AppProtocolError> {
        let WorkbenchIntent::CheckPreviewBehavior { launch, observed, note } = command.intent()
        else {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        };
        let mut value = Self {
            binding_digest: [0; 32],
            launch: launch.into_bytes(),
            observed: observed.as_str().to_owned(),
            note: note.as_str().to_owned(),
            observed_match,
            note_match,
            goal,
        };
        value.binding_digest = value.digest()?;
        Ok(value)
    }

    fn digest(&self) -> Result<[u8; 32], AppProtocolError> {
        use sha2::Digest as _;
        let mut digest = sha2::Sha256::new();
        digest.update(b"peritus-preview-behavior-evidence-v1\0");
        digest.update(self.launch);
        // Length-prefixed exact text and typed serialized matches make the binding unambiguous.
        let matches = serde_json::to_vec(&(self.observed_match, self.note_match, self.goal))
            .map_err(|_| AppProtocolError::new(AppErrorCode::MalformedFrame, None))?;
        for bytes in [self.observed.as_bytes(), self.note.as_bytes(), matches.as_slice()] {
            let length = u64::try_from(bytes.len())
                .map_err(|_| AppProtocolError::new(AppErrorCode::MalformedFrame, None))?;
            digest.update(length.to_be_bytes());
            digest.update(bytes);
        }
        Ok(digest.finalize().into())
    }

    pub(super) fn validate(
        &self,
        command: &WorkbenchCommand,
        launch: &WorkbenchLaunchResult,
    ) -> Result<(), AppProtocolError> {
        let invalid = || AppProtocolError::new(AppErrorCode::MalformedFrame, None);
        let WorkbenchIntent::CheckPreviewBehavior { launch: launch_id, observed, note } =
            command.intent()
        else {
            return Err(invalid());
        };
        if self.launch != launch_id.into_bytes()
            || *launch_id != launch.launch()
            || self.observed != observed.as_str()
            || self.note != note.as_str()
        {
            return Err(invalid());
        }
        if self.binding_digest != self.digest()? {
            return Err(invalid());
        }
        if let Some(goal) = self.goal {
            goal.validate()?;
            if self.note_match.is_none() {
                return Err(invalid());
            }
        }
        let process = launch.process().ok_or_else(invalid)?;
        for (evidence, needle) in [
            (Some(self.observed_match), self.observed.as_str()),
            (self.note_match, self.note.as_str()),
        ] {
            if let Some(evidence) = evidence {
                evidence.validate_for_needle(process, needle).map_err(|_| invalid())?;
                if evidence.stream() == PreviewOutputStream::Terminal
                    && !launch.profile().interactive()
                {
                    return Err(invalid());
                }
            }
        }
        if let Some(note) = self.note_match
            && (note.stream() == PreviewOutputStream::Terminal)
                != (self.observed_match.stream() == PreviewOutputStream::Terminal)
        {
            return Err(invalid());
        }
        Ok(())
    }

    pub(super) fn command(
        &self,
        operation: ControlOperationId,
        query: WorkbenchQuery,
        revision: u64,
    ) -> Result<WorkbenchCommand, AppProtocolError> {
        Ok(WorkbenchCommand::new(
            operation,
            query,
            revision,
            WorkbenchIntent::CheckPreviewBehavior {
                launch: ControlOperationId::new(self.launch)
                    .map_err(|_| AppProtocolError::new(AppErrorCode::MalformedFrame, None))?,
                observed: WorkbenchLaunchText::new(self.observed.clone())?,
                note: WorkbenchInputText::new(self.note.clone())?,
            },
        ))
    }
}

/// The goal version targeted when output was observed, never selected again during replay.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PreviewGoalBinding {
    pub(super) goal: [u8; 16],
    pub(super) criterion_index: u32,
    pub(super) user_revision: u64,
    pub(super) required_input_generation: u64,
}

impl PreviewGoalBinding {
    pub(super) fn matches(
        self,
        goal: &peritus_product_runner::control::GoalRecord,
        note: &str,
    ) -> bool {
        self.goal == *goal.id().as_bytes()
            && self.user_revision == goal.user_revision()
            && self.required_input_generation == goal.required_input_generation()
            && usize::try_from(self.criterion_index)
                .ok()
                .and_then(|index| goal.criteria().get(index))
                .is_some_and(|criterion| {
                    criterion.kind()
                        == peritus_product_runner::control::GoalCriterionKind::GraphicalPlaytest
                        && criterion.description() == note
                })
    }

    pub(super) const fn validate(self) -> Result<(), AppProtocolError> {
        if ControlOperationId::new(self.goal).is_err()
            || self.user_revision == 0
            || self.required_input_generation == 0
        {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(())
    }
}
