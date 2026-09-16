//! Durable provider request admission and workbench observation helpers.

use super::{
    DeveloperLoopError, DeveloperModelRole, DeveloperRequestAdmission, InteractionOptions,
    LiveConversation, ProductActivityKind, ProductInteractionMode, ProductRunServiceError,
    goal_role, narration, persist_record, port_error,
};

impl LiveConversation {
    pub(super) fn prepare_request_for_role(
        &self,
        role: DeveloperModelRole,
        revision: u64,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<DeveloperRequestAdmission, DeveloperLoopError> {
        if self.service.governed_run(self.run_id).map_err(|_| port_error())?
            && !self
                .service
                .permission_allows(
                    self.run_id,
                    peritus_product_runner::control::PermissionCapability::Network,
                )
                .map_err(|_| port_error())?
        {
            return Err(DeveloperLoopError::Trace(
                "network permission is disabled for this workspace; inspect /permissions"
                    .to_owned(),
            ));
        }
        let mut records = self.service.inner.records.write().map_err(|_| port_error())?;
        let record = records.get_mut(&self.run_id).ok_or_else(port_error)?;
        if self.service.record_input_revision(record).map_err(|_| port_error())? != revision {
            return Ok(DeveloperRequestAdmission::Stale);
        }
        let options = record.interaction.as_ref().ok_or_else(port_error)?;
        if options.persistence_failed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(port_error());
        }
        if let Some(start) = &options.workbench {
            if self.service.refresh_request_files(start, request).map_err(|_| port_error())? {
                return Ok(DeveloperRequestAdmission::Stale);
            }
            let admission = self
                .service
                .with_controls(false, |store| {
                    let admission = store.prepare_execution(start, revision, request)?;
                    if admission.developer_admission() == DeveloperRequestAdmission::Stale {
                        return Ok(DeveloperRequestAdmission::Stale);
                    }
                    let goal = store.reserve_goal_request(
                        start,
                        goal_role(role),
                        request.request_id().expose_for_wire(),
                    )?;
                    Ok(if goal == peritus_product_runner::control::GoalAdmission::Accepted {
                        DeveloperRequestAdmission::Accepted
                    } else {
                        DeveloperRequestAdmission::Stopped
                    })
                })
                .map_err(|_| port_error())?;
            if admission != DeveloperRequestAdmission::Accepted {
                return Ok(admission);
            }
        }
        let mut options = options.clone();
        if revision > options.incorporated {
            options.incorporated = revision;
            options
                .append(
                    ProductActivityKind::Status,
                    narration::starting(options.mode),
                    &format!("Input {revision} incorporated into model request"),
                )
                .map_err(|_| port_error())?;
        }
        let previous = record.interaction.replace(options).ok_or_else(port_error)?;
        if persist_record(&self.service.inner.directory, record).is_err() {
            previous.persistence_failed.store(true, std::sync::atomic::Ordering::Release);
            record.interaction = Some(previous);
            return Err(port_error());
        }
        Ok(DeveloperRequestAdmission::Accepted)
    }

    pub(super) fn workbench_start(
        &self,
    ) -> Result<Option<peritus_product_runner::control::ControlOperation>, DeveloperLoopError> {
        self.workbench_start_record().map_err(|_| port_error())
    }

    pub(super) fn update(
        &self,
        change: impl FnOnce(&mut InteractionOptions) -> Result<(), ProductRunServiceError>,
    ) -> Result<(), DeveloperLoopError> {
        let mut records = self.service.inner.records.write().map_err(|_| port_error())?;
        let record = records.get_mut(&self.run_id).ok_or_else(port_error)?;
        let options = record.interaction.as_mut().ok_or_else(port_error)?;
        change(options).map_err(|_| port_error())?;
        if options.mode != ProductInteractionMode::Build
            && record.snapshot.phase() == peritus_app_protocol::ProductRunPhase::Queued
        {
            record.snapshot = crate::product_run::replace_snapshot(
                &record.snapshot,
                peritus_app_protocol::ProductRunPhase::Writing,
                "Responding to the conversation",
                record.snapshot.summary(),
            )
            .map_err(|_| port_error())?;
        }
        persist_record(&self.service.inner.directory, record).map_err(|_| port_error())
    }
}
