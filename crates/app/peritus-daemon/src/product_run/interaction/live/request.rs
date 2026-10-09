//! Durable provider request admission and workbench observation helpers.

use super::{
    DeveloperLoopError, DeveloperModelRole, DeveloperRequestAdmission, InteractionOptions,
    LiveConversation, ProductActivityKind, ProductInteractionMode, ProductRunServiceError,
    goal_role, narration, port_error,
};

impl LiveConversation {
    pub(super) fn prepare_request_for_role(
        &self,
        role: DeveloperModelRole,
        revision: u64,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<DeveloperRequestAdmission, DeveloperLoopError> {
        if !self
            .service
            .permission_allows(
                self.run_id,
                peritus_product_runner::control::PermissionCapability::Network,
            )
            .map_err(|error| port_error("read effective network permission", error.into()))?
        {
            return Err(DeveloperLoopError::Trace(
                "network permission is disabled for this workspace; inspect /permissions"
                    .to_owned(),
            ));
        }
        let mut records = self.service.inner.records.write().map_err(|_| {
            super::port_internal(
                "admit the provider request",
                "the product-run record lock was poisoned",
            )
        })?;
        let record = records.get_mut(&self.run_id).ok_or_else(|| {
            super::port_internal(
                "admit the provider request",
                "the product-run record was not found",
            )
        })?;
        if self
            .service
            .record_input_revision(record)
            .map_err(|error| port_error("read the input revision", error))?
            != revision
        {
            return Ok(DeveloperRequestAdmission::Stale);
        }
        let options = &record.interaction;
        if options.persistence_failed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(super::port_internal(
                "admit the provider request",
                options
                    .persistence_failure()
                    .as_deref()
                    .unwrap_or("the previous persistence operation failed"),
            ));
        }
        let start = &options.workbench;
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
            .map_err(|error| {
                port_error("reserve the provider request in durable control state", error.into())
            })?;
        if admission != DeveloperRequestAdmission::Accepted {
            return Ok(admission);
        }
        self.service
            .finish_file_refresh(start)
            .map_err(|error| port_error("finish selected source snapshot", error))?;
        let mut next = record.clone();
        let mut options = options.clone();
        self.service
            .append_control_inputs(&mut options)
            .map_err(|error| port_error("project accepted user input", error))?;
        if revision > options.incorporated {
            options.incorporated = revision;
            options
                .append(
                    ProductActivityKind::Status,
                    narration::starting(options.mode),
                    &format!("Input {revision} incorporated into model request"),
                )
                .map_err(|error| port_error("record provider request admission", error))?;
        }
        next.interaction = options;
        crate::product_run::persistence::write_record(&self.service.inner.directory, &next)
            .map_err(|error| port_error("persist provider request admission", error))?;
        *record = next;
        Ok(DeveloperRequestAdmission::Accepted)
    }

    pub(super) fn workbench_start(
        &self,
    ) -> Result<peritus_product_runner::control::ControlOperation, DeveloperLoopError> {
        self.workbench_start_record()
            .map_err(|error| port_error("read the active workbench execution", error))
    }

    pub(super) fn update(
        &self,
        change: impl FnOnce(
            &mut InteractionOptions,
            &mut super::super::super::progress::RunProgress,
        ) -> Result<(), ProductRunServiceError>,
    ) -> Result<(), DeveloperLoopError> {
        let mut records = self.service.inner.records.write().map_err(|_| {
            super::port_internal(
                "record conversation activity",
                "the product-run record lock was poisoned",
            )
        })?;
        let record = records.get_mut(&self.run_id).ok_or_else(|| {
            super::port_internal(
                "record conversation activity",
                "the product-run record was not found",
            )
        })?;
        let mut next = record.clone();
        change(&mut next.interaction, &mut next.progress)
            .map_err(|error| port_error("record conversation activity", error))?;
        let options = &mut next.interaction;
        if options.mode != ProductInteractionMode::Build
            && next.snapshot.phase() == peritus_app_protocol::ProductRunPhase::Queued
        {
            next.snapshot = crate::product_run::replace_snapshot(
                &next.snapshot,
                peritus_app_protocol::ProductRunPhase::Writing,
                "Responding to the conversation",
                next.snapshot.summary(),
            )
            .map_err(|error| port_error("project the public run phase", error))?;
        }
        crate::product_run::persistence::write_record(&self.service.inner.directory, &next)
            .map_err(|error| port_error("persist conversation activity", error))?;
        *record = next;
        Ok(())
    }
}
