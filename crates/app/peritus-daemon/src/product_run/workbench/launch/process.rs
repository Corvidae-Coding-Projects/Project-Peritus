//! Owned preview process launch, interaction, observation, and termination.

use super::*;
use crate::product_run::{
    ProductRunServiceError,
    publication::{MutationDisposition, RunIdentitySnapshot, RunMutationKind},
};

#[derive(Clone, Copy)]
pub(super) enum PreviewObservationFence {
    Start { operation: ControlOperationId },
    Stop { operation: ControlOperationId, state: WorkbenchLaunchState },
    Refresh { state: WorkbenchLaunchState },
}

impl ProductRunService {
    pub(super) fn launch_owned_preview(
        &self,
        profile: &WorkbenchLaunchProfile,
        workspace: PathBuf,
        direct: bool,
        operation: ControlOperationId,
    ) -> Result<
        (CommandRuntime, peritus_product_runner::PreviewLaunch, PreviewObservation),
        AppProtocolError,
    > {
        let state = self.preview_state_root().join("commands").join(hex(operation.as_bytes()));
        let run = RunId::new(operation.into_bytes()).map_err(|_| app_error(Code::Internal))?;
        let runtime = if direct {
            CommandRuntime::open_direct(state, &workspace, run, self.inner.processes.clone())
        } else {
            CommandRuntime::open(state, &workspace, run, self.inner.processes.clone())
        }
        .map_err(|_| app_error(Code::Backpressure))?;
        let environment = profile
            .environment()
            .iter()
            .map(|name| {
                std::env::var(name.as_str())
                    .map(|value| (name.as_str().to_owned(), value))
                    .map_err(|_| app_error(Code::MissingRequiredFeature))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let cwd = resolve_workspace_path(&workspace, profile.working_directory().as_str(), true)?;
        let preview = PreviewCommand::new(
            profile.executable().as_str().to_owned(),
            profile.arguments().iter().map(|value| value.as_str().to_owned()).collect(),
            cwd,
            profile.wall_millis().map(Duration::from_millis),
            profile.interactive(),
            24,
            80,
            hex(operation.as_bytes()),
            environment,
        )
        .map_err(|_| app_error(Code::MalformedFrame))?;
        let launch = runtime.launch_preview(&preview).map_err(|_| app_error(Code::Backpressure))?;
        let observation =
            runtime.observe_preview(&launch).map_err(|_| app_error(Code::Backpressure))?;
        Ok((runtime, launch, observation))
    }

    pub(in crate::product_run) fn interact_preview(
        &self,
        run: RunId,
        command: &WorkbenchCommand,
        launch_id: ControlOperationId,
        bytes: &[u8],
    ) -> Result<WorkbenchReceipt, AppProtocolError> {
        let digest = peritus_codec::sha256(bytes);
        let identity = self
            .capture_run_identity(run)
            .map_err(super::aggregate::preview_service_error)?;
        let (receipt, admitted) = self.admit_preview_with_identity(
            command,
            run,
            &identity,
            |preview| {
                mutate_launch(preview, launch_id, |current| {
                    let mut interactions = current.interactions().to_vec();
                    interactions.push(WorkbenchInteractionReceipt::new(
                        command.operation(),
                        digest,
                        false,
                    ));
                    rebuild_launch_with(
                        current,
                        interactions,
                        current.captures().to_vec(),
                        current.feedback().to_vec(),
                        current.behavior_checks(),
                    )
                })
            },
        )?;
        if !admitted {
            return Ok(receipt);
        }
        let active = self.preview_process(launch_id)?;
        if let Ok(observation) = active.runtime.interact_preview(&active.launch, bytes.to_vec()) {
            self.update_preview_interaction(
                &identity,
                launch_id,
                command.operation(),
                digest,
                &active.launch,
                &observation,
            )?;
        }
        Ok(receipt)
    }

    pub(in crate::product_run) fn stop_preview(
        &self,
        run: RunId,
        command: &WorkbenchCommand,
        launch_id: ControlOperationId,
    ) -> Result<WorkbenchReceipt, AppProtocolError> {
        let identity = self
            .capture_run_identity(run)
            .map_err(super::aggregate::preview_service_error)?;
        let (receipt, admitted) = self.admit_preview_with_identity(
            command,
            run,
            &identity,
            |preview| require_launch(preview, launch_id).map(|_| ()),
        )?;
        if !admitted {
            self.qualify_graphical_goal(run, command, launch_id)?;
            return Ok(receipt);
        }
        let prior = self.preview_launch_snapshot(&identity, launch_id)?;
        let active = self.preview_process(launch_id)?;
        if let Ok(observation) = active
            .runtime
            .stop_preview(&active.launch)
            .and_then(|observation| wait_for_preview_terminal(&active, observation))
        {
            self.store_observation(
                &identity,
                launch_id,
                PreviewObservationFence::Stop {
                    operation: command.operation(),
                    state: prior.state(),
                },
                &active.launch,
                &observation,
            )?;
        }
        self.qualify_graphical_goal(run, command, launch_id)?;
        Ok(receipt)
    }

    pub(super) fn store_observation(
        &self,
        identity: &RunIdentitySnapshot,
        launch_id: ControlOperationId,
        fence: PreviewObservationFence,
        launch: &peritus_product_runner::PreviewLaunch,
        observation: &PreviewObservation,
    ) -> Result<(), AppProtocolError> {
        let run = identity.run;
        let input_digest = preview_observation_digest(
            b"peritus-product-run-preview-observation-v1\0",
            run,
            launch_id,
            None,
            None,
            launch,
            observation,
        );
        let (_, ticket) = self
            .mutate_run(
                run,
                Some(&identity.cancelled),
                RunMutationKind::PreviewObservation,
                input_digest,
                MutationDisposition::DurabilityRequired,
                move |record| {
                    if record.request.workspace_id() != identity.workspace
                        || record.interaction.workbench != identity.start
                    {
                        return Err(ProductRunServiceError::InvalidState);
                    }
                    let current = require_launch(&record.preview, launch_id)
                        .map_err(super::aggregate::preview_mutation_error)?;
                    let exact = observation_matches(
                        &record.preview,
                        current,
                        launch_id,
                        launch,
                        observation,
                    );
                    if exact {
                        let admitted = match fence {
                            PreviewObservationFence::Start { operation }
                            | PreviewObservationFence::Stop { operation, .. } => {
                                record.preview.operations.contains_key(&operation)
                            }
                            PreviewObservationFence::Refresh { .. } => true,
                        };
                        return if admitted {
                            Ok(())
                        } else {
                            Err(ProductRunServiceError::InvalidState)
                        };
                    }
                    let fenced = match fence {
                        PreviewObservationFence::Start { operation } => {
                            operation == launch_id
                                && record.preview.operations.contains_key(&operation)
                                && current.state() == WorkbenchLaunchState::Accepted
                                && current.process().is_none()
                                && !current.ready()
                        }
                        PreviewObservationFence::Stop { operation, state } => {
                            record.preview.operations.contains_key(&operation)
                                && current.state() == state
                                && current.process() == Some(launch.process_id())
                        }
                        PreviewObservationFence::Refresh { state } => {
                            state == WorkbenchLaunchState::Running
                                && current.state() == state
                                && current.process() == Some(launch.process_id())
                        }
                    };
                    if !fenced {
                        return Err(ProductRunServiceError::InvalidState);
                    }
                    mutate_launch(&mut record.preview, launch_id, |current| {
                        rebuild_launch(
                            current,
                            Some(launch.process_id()),
                            launch_state(observation.state()),
                            true,
                        )
                        .and_then(|value| {
                            if observation.state() == PreviewProcessState::Running {
                                Ok(value)
                            } else {
                                terminal_launch(&value, observation)
                            }
                        })
                    })
                    .map_err(super::aggregate::preview_mutation_error)?;
                    output::retain_output(&mut record.preview, launch_id, observation);
                    Ok(())
                },
            )
            .map_err(super::aggregate::preview_service_error)?;
        self.await_run_durable(ticket)
            .map_err(super::aggregate::preview_service_error)
    }

    pub(super) fn update_preview_interaction(
        &self,
        identity: &RunIdentitySnapshot,
        launch_id: ControlOperationId,
        operation: ControlOperationId,
        interaction_digest: Sha256Digest,
        launch: &peritus_product_runner::PreviewLaunch,
        observation: &PreviewObservation,
    ) -> Result<(), AppProtocolError> {
        let run = identity.run;
        let input_digest = preview_observation_digest(
            b"peritus-product-run-preview-interaction-v1\0",
            run,
            launch_id,
            Some(operation),
            Some(interaction_digest),
            launch,
            observation,
        );
        let (_, ticket) = self
            .mutate_run(
                run,
                Some(&identity.cancelled),
                RunMutationKind::PreviewObservation,
                input_digest,
                MutationDisposition::DurabilityRequired,
                move |record| {
                    if record.request.workspace_id() != identity.workspace
                        || record.interaction.workbench != identity.start
                    {
                        return Err(ProductRunServiceError::InvalidState);
                    }
                    let (result_sequence, completed_sequence) = record
                        .preview
                        .operations
                        .get(&operation)
                        .map(|value| (value.result_sequence, value.completed_sequence))
                        .ok_or(ProductRunServiceError::InvalidState)?;
                    let current = require_launch(&record.preview, launch_id)
                        .map_err(super::aggregate::preview_mutation_error)?;
                    let interaction = current
                        .interactions()
                        .iter()
                        .find(|value| value.operation() == operation)
                        .ok_or(ProductRunServiceError::InvalidState)?;
                    if interaction.digest() != interaction_digest {
                        return Err(ProductRunServiceError::InvalidState);
                    }
                    if interaction.observed() {
                        if completed_sequence > result_sequence
                            && observation_matches(
                                &record.preview,
                                current,
                                launch_id,
                                launch,
                                observation,
                            )
                        {
                            return Ok(());
                        }
                        return Err(ProductRunServiceError::InvalidState);
                    }
                    if completed_sequence != 0
                        || current.state() != WorkbenchLaunchState::Running
                        || current.process() != Some(launch.process_id())
                    {
                        return Err(ProductRunServiceError::InvalidState);
                    }
                    mutate_launch(&mut record.preview, launch_id, |current| {
                        let interactions = current
                            .interactions()
                            .iter()
                            .map(|value| {
                                if value.operation() == operation {
                                    WorkbenchInteractionReceipt::new(
                                        value.operation(),
                                        value.digest(),
                                        true,
                                    )
                                } else {
                                    *value
                                }
                            })
                            .collect();
                        let value = rebuild_launch_with(
                            current,
                            interactions,
                            current.captures().to_vec(),
                            current.feedback().to_vec(),
                            current.behavior_checks(),
                        )?;
                        let value = rebuild_launch(
                            &value,
                            Some(launch.process_id()),
                            launch_state(observation.state()),
                            true,
                        )?;
                        if observation.state() == PreviewProcessState::Running {
                            Ok(value)
                        } else {
                            terminal_launch(&value, observation)
                        }
                    })
                    .map_err(super::aggregate::preview_mutation_error)?;
                    if let Some(value) = record.preview.operations.get_mut(&operation) {
                        value.completed_sequence = record
                            .preview
                            .page
                            .as_ref()
                            .map_or(0, WorkbenchResultPage::result_revision);
                    }
                    output::retain_output(&mut record.preview, launch_id, observation);
                    Ok(())
                },
            )
            .map_err(super::aggregate::preview_service_error)?;
        self.await_run_durable(ticket)
            .map_err(super::aggregate::preview_service_error)
    }

    pub(super) fn preview_process(
        &self,
        launch: ControlOperationId,
    ) -> Result<super::super::super::PreviewProcess, AppProtocolError> {
        self.inner
            .preview_processes
            .lock()
            .map_err(|_| app_error(Code::Backpressure))?
            .get(&launch)
            .cloned()
            .ok_or_else(|| app_error(Code::InvalidIdentifier))
    }

    fn preview_launch_snapshot(
        &self,
        identity: &RunIdentitySnapshot,
        launch: ControlOperationId,
    ) -> Result<WorkbenchLaunchResult, AppProtocolError> {
        let records = self.inner.records.read().map_err(|_| app_error(Code::Backpressure))?;
        let record = records
            .get(&identity.run)
            .ok_or_else(|| app_error(Code::InvalidIdentifier))?;
        if !std::sync::Arc::ptr_eq(&record.cancelled, &identity.cancelled)
            || record.request.workspace_id() != identity.workspace
            || record.interaction.workbench != identity.start
        {
            return Err(app_error(Code::IdempotencyConflict));
        }
        require_launch(&record.preview, launch).map(Clone::clone)
    }

    pub(super) fn refresh_previews(&self, run: RunId) -> Result<(), AppProtocolError> {
        let identity = self
            .capture_run_identity(run)
            .map_err(super::aggregate::preview_service_error)?;
        let launches = {
            let records = self.inner.records.read().map_err(|_| app_error(Code::Backpressure))?;
            let Some(page) = records.get(&run).and_then(|record| record.preview.page.as_ref())
            else {
                return Ok(());
            };
            page.launches().to_vec()
        };
        for row in launches {
            if row.state() != WorkbenchLaunchState::Running {
                continue;
            }
            let Ok(active) = self.preview_process(row.launch()) else {
                continue;
            };
            if let Ok(observation) = active.runtime.observe_preview(&active.launch) {
                self.store_observation(
                    &identity,
                    row.launch(),
                    PreviewObservationFence::Refresh { state: row.state() },
                    &active.launch,
                    &observation,
                )?;
            }
        }
        Ok(())
    }
}

fn preview_observation_digest(
    domain: &[u8],
    run: RunId,
    launch_id: ControlOperationId,
    operation: Option<ControlOperationId>,
    semantic: Option<Sha256Digest>,
    launch: &peritus_product_runner::PreviewLaunch,
    observation: &PreviewObservation,
) -> Sha256Digest {
    let mut input = domain.to_vec();
    input.extend_from_slice(run.as_bytes());
    input.extend_from_slice(launch_id.as_bytes());
    let operation = operation.map_or([0; 16], ControlOperationId::into_bytes);
    input.extend_from_slice(&operation);
    input.extend_from_slice(
        semantic
            .unwrap_or_else(|| Sha256Digest::new([0; 32]))
            .as_bytes(),
    );
    input.extend_from_slice(launch.process_id().as_bytes());
    input.push(match observation.state() {
        PreviewProcessState::Running => 1,
        PreviewProcessState::Succeeded => 2,
        PreviewProcessState::Failed => 3,
        PreviewProcessState::Cancelled => 4,
        PreviewProcessState::TimedOut => 5,
        PreviewProcessState::Indeterminate => 6,
    });
    input.extend_from_slice(peritus_codec::sha256(observation.stdout().as_bytes()).as_bytes());
    input.extend_from_slice(peritus_codec::sha256(observation.stderr().as_bytes()).as_bytes());
    input.extend_from_slice(&observation.exit_code().unwrap_or(i64::MIN).to_be_bytes());
    peritus_codec::sha256(&input)
}

fn observation_matches(
    preview: &super::super::super::PreviewAggregate,
    current: &WorkbenchLaunchResult,
    launch_id: ControlOperationId,
    launch: &peritus_product_runner::PreviewLaunch,
    observation: &PreviewObservation,
) -> bool {
    if current.process() != Some(launch.process_id())
        || current.state() != launch_state(observation.state())
        || !current.ready()
        || !output::output_matches(preview, launch_id, observation)
    {
        return false;
    }
    observation.state() == PreviewProcessState::Running
        || (current.stdout_digest()
            == Some(peritus_codec::sha256(observation.stdout().as_bytes()))
            && current.exit_code()
                == observation.exit_code().and_then(|value| u32::try_from(value).ok()))
}

#[derive(Clone, Copy)]
pub(super) enum PreviewTarget {
    Launch(ControlOperationId),
    Capture(ControlOperationId),
}

pub(super) fn wait_for_preview_terminal(
    active: &super::super::super::PreviewProcess,
    mut observation: PreviewObservation,
) -> Result<PreviewObservation, peritus_product_runner::ProductRunnerError> {
    let began = std::time::Instant::now();
    while observation.state() == PreviewProcessState::Running
        && began.elapsed() < Duration::from_secs(5)
    {
        std::thread::sleep(Duration::from_millis(10));
        observation = active.runtime.observe_preview(&active.launch)?;
    }
    Ok(observation)
}

pub(super) const fn launch_state(state: PreviewProcessState) -> WorkbenchLaunchState {
    match state {
        PreviewProcessState::Running => WorkbenchLaunchState::Running,
        PreviewProcessState::Succeeded => WorkbenchLaunchState::Exited,
        PreviewProcessState::Cancelled => WorkbenchLaunchState::Stopped,
        PreviewProcessState::Failed
        | PreviewProcessState::TimedOut
        | PreviewProcessState::Indeterminate => WorkbenchLaunchState::Failed,
    }
}

pub(super) fn text(value: &str) -> Result<WorkbenchLaunchText, AppProtocolError> {
    WorkbenchLaunchText::new(value.to_owned())
}

pub(super) const fn app_error(code: Code) -> AppProtocolError {
    AppProtocolError::new(code, None)
}
