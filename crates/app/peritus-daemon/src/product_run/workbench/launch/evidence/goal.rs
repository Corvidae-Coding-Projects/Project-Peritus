//! Admission-bound graphical goal publication, including exact restart fencing.
use super::*;
use crate::product_run::preview_evidence::PreviewGoalBinding;

impl ProductRunService {
    pub(in crate::product_run::workbench::launch) fn qualify_graphical_goal(
        &self,
        run: RunId,
        command: &WorkbenchCommand,
        launch_id: ControlOperationId,
    ) -> Result<(), AppProtocolError> {
        let WorkbenchIntent::CheckPreviewBehavior { note, .. } = command.intent() else {
            return Ok(());
        };
        let query = command.query();
        let (start, launch, operation, capture) = {
            let records = self.inner.records.read().map_err(|_| app_error(Code::Backpressure))?;
            let record = records.get(&run).ok_or_else(|| app_error(Code::InvalidIdentifier))?;
            let launch = require_launch(&record.preview, launch_id)?;
            let Some(operation) = record.preview.operations.get(&command.operation()) else {
                return Ok(());
            };
            let capture = launch.captures().iter().find(|capture| {
                capture.state() == WorkbenchCaptureState::Captured
                    && capture.artifact().is_some()
                    && capture.image_digest().is_some()
                    && capture.dimensions().is_some_and(|(width, height)| width > 0 && height > 0)
                    && capture.captured_unix_millis().is_some()
                    && ordered_capture(
                        capture.operation(),
                        command.operation(),
                        launch.interactions(),
                        &record.preview.operations,
                    )
            });
            let Some(capture) = capture else { return Ok(()) };
            (
                record.interaction.workbench.clone(),
                launch.clone(),
                operation.clone(),
                capture.clone(),
            )
        };
        if !matches!(start.intent(), ControlIntent::StartGoal { .. })
            || launch.profile().run() != run
            || launch.profile().build().is_none()
            || launch.process().is_none()
            || !launch.ready()
        {
            return Ok(());
        }
        if operation.fingerprint != command.fingerprint()? {
            return Err(app_error(Code::IdempotencyConflict));
        }
        let Some(evidence) = &operation.behavior_evidence else { return Ok(()) };
        evidence.validate(command, &launch)?;
        let Some(note_match) = evidence.note_match else { return Ok(()) };
        let Some(binding) = evidence.goal else { return Ok(()) };
        match self.verify_profile(query, launch.profile()) {
            Ok(_) => {}
            Err(error) if error.code() == Code::StaleRevision => return Ok(()),
            Err(error) => return Err(error),
        }
        self.with_controls(false, |store| {
            let record = store.load(start.conversation())?.ok_or(ControlError::NotFound)?;
            let goal = record
                .goal()
                .filter(|goal| goal.id() == start.id())
                .ok_or(ControlError::NotFound)?;
            if goal.run_bytes() != run.as_bytes() {
                return Ok(());
            }
            if !binding.matches(goal, note.as_str()) {
                return Ok(());
            }
            store.observe_graphical_goal_evidence(
                &start,
                binding.criterion_index,
                binding.user_revision,
                binding.required_input_generation,
                OperationId::new(launch_id.into_bytes())?,
                OperationId::new(capture.operation().into_bytes())?,
                peritus_product_runner::control::GraphicalOutputEvidence::new(
                    OperationId::new(command.operation().into_bytes())?,
                    evidence.observed_match,
                    note_match,
                )?,
            )
        })
        .map_err(error_value)
    }

    pub(super) fn preview_goal_binding(
        &self,
        run: RunId,
        note: &str,
    ) -> Result<Option<PreviewGoalBinding>, AppProtocolError> {
        let start = {
            let records = self.inner.records.read().map_err(|_| app_error(Code::Backpressure))?;
            records
                .get(&run)
                .ok_or_else(|| app_error(Code::InvalidIdentifier))?
                .interaction
                .workbench
                .clone()
        };
        if !matches!(start.intent(), ControlIntent::StartGoal { .. }) {
            return Ok(None);
        }
        self.with_controls(false, |store| {
            let record = store.load(start.conversation())?.ok_or(ControlError::NotFound)?;
            let goal = record
                .goal()
                .filter(|goal| goal.id() == start.id() && goal.run_bytes() == run.as_bytes())
                .ok_or(ControlError::NotFound)?;
            let mut matching = goal.criteria().iter().enumerate().filter(|(_, criterion)| {
                criterion.kind() == GoalCriterionKind::GraphicalPlaytest
                    && criterion.description() == note
            });
            let Some((index, _)) = matching.next() else { return Ok(None) };
            if matching.next().is_some() {
                return Err(ControlError::InvalidInput.into());
            }
            Ok(Some(PreviewGoalBinding {
                goal: *goal.id().as_bytes(),
                criterion_index: u32::try_from(index).map_err(|_| ControlError::Capacity)?,
                user_revision: goal.user_revision(),
                required_input_generation: goal.required_input_generation(),
            }))
        })
        .map_err(error_value)
    }
}
