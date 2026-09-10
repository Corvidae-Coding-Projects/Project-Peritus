//! Owned preview process launch, interaction, observation, and termination.

use super::*;

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
            Duration::from_millis(profile.wall_millis()),
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
        let (receipt, admitted) = self.admit_preview(command, run, |preview| {
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
        })?;
        if !admitted {
            return Ok(receipt);
        }
        let active = self.preview_process(launch_id)?;
        if let Ok(observation) = active.runtime.interact_preview(&active.launch, bytes.to_vec()) {
            self.update_preview_interaction(
                run,
                launch_id,
                command.operation(),
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
        let (receipt, admitted) = self.admit_preview(command, run, |preview| {
            require_launch(preview, launch_id).map(|_| ())
        })?;
        if !admitted {
            self.qualify_graphical_goal(run, command, launch_id)?;
            return Ok(receipt);
        }
        let active = self.preview_process(launch_id)?;
        if let Ok(observation) = active
            .runtime
            .stop_preview(&active.launch)
            .and_then(|observation| wait_for_preview_terminal(&active, observation))
        {
            self.store_observation(run, launch_id, &active.launch, &observation)?;
        }
        self.qualify_graphical_goal(run, command, launch_id)?;
        Ok(receipt)
    }

    pub(super) fn store_observation(
        &self,
        run: RunId,
        launch_id: ControlOperationId,
        launch: &peritus_product_runner::PreviewLaunch,
        observation: &PreviewObservation,
    ) -> Result<(), AppProtocolError> {
        let mut records = self.inner.records.write().map_err(|_| app_error(Code::Backpressure))?;
        let record = records.get_mut(&run).ok_or_else(|| app_error(Code::InvalidIdentifier))?;
        let previous = record.preview.clone();
        mutate_launch(&mut record.preview, launch_id, |current| {
            rebuild_launch(
                current,
                Some(launch.process_id()),
                launch_state(observation.state()),
                true,
            )
            .map(|mut value| {
                if observation.state() != PreviewProcessState::Running {
                    value = terminal_launch(&value, observation).expect("valid prior launch");
                }
                value
            })
        })?;
        if observation.state() != PreviewProcessState::Running {
            record.preview.outputs.insert(launch_id, observation.stdout().to_owned());
        }
        if super::super::super::persist_record(&self.inner.directory, record).is_err() {
            record.preview = previous;
            return Err(app_error(Code::Backpressure));
        }
        Ok(())
    }

    pub(super) fn update_preview_interaction(
        &self,
        run: RunId,
        launch_id: ControlOperationId,
        operation: ControlOperationId,
        launch: &peritus_product_runner::PreviewLaunch,
        observation: &PreviewObservation,
    ) -> Result<(), AppProtocolError> {
        let mut records = self.inner.records.write().map_err(|_| app_error(Code::Backpressure))?;
        let record = records.get_mut(&run).ok_or_else(|| app_error(Code::InvalidIdentifier))?;
        let previous = record.preview.clone();
        mutate_launch(&mut record.preview, launch_id, |current| {
            let interactions = current
                .interactions()
                .iter()
                .map(|value| {
                    if value.operation() == operation {
                        WorkbenchInteractionReceipt::new(value.operation(), value.digest(), true)
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
        })?;
        if let Some(value) = record.preview.operations.get_mut(&operation) {
            value.completed_sequence =
                record.preview.page.as_ref().map_or(0, WorkbenchResultPage::result_revision);
        }
        if observation.state() != PreviewProcessState::Running {
            record.preview.outputs.insert(launch_id, observation.stdout().to_owned());
        }
        if super::super::super::persist_record(&self.inner.directory, record).is_err() {
            record.preview = previous;
            return Err(app_error(Code::Backpressure));
        }
        Ok(())
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

    pub(super) fn refresh_previews(&self, run: RunId) -> Result<(), AppProtocolError> {
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
            if let Ok(observation) = active.runtime.observe_preview(&active.launch)
                && observation.state() != PreviewProcessState::Running
            {
                self.store_observation(run, row.launch(), &active.launch, &observation)?;
            }
        }
        Ok(())
    }
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
