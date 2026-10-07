//! Durable provider request admission and workbench observation helpers.

use super::{
    DeveloperLoopError, DeveloperModelRole, DeveloperRequestAdmission, InteractionOptions,
    LiveConversation, ProductActivityKind, ProductInteractionMode, ProductRunServiceError,
    goal_role, narration, port_error,
};
use crate::product_run::publication::{MutationDisposition, RunMutationKind};

impl LiveConversation {
    pub(super) fn prepare_request_for_role(
        &self,
        role: DeveloperModelRole,
        revision: u64,
        selection_provenance: Option<peritus_types::Sha256Digest>,
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
        let identity = self
            .service
            .capture_run_identity(self.run_id)
            .map_err(|error| port_error("capture provider request identity", error))?;
        if !std::sync::Arc::ptr_eq(&identity.cancelled, &self.attempt_cancelled) {
            return Ok(DeveloperRequestAdmission::Stopped);
        }
        if let Some(expected) = selection_provenance {
            let providers = identity.request.providers();
            let (profile, choice) = match role {
                DeveloperModelRole::Writer => {
                    (providers.writer(), identity.interaction.models.writer())
                }
                DeveloperModelRole::Reviewer => {
                    (providers.reviewer(), identity.interaction.models.reviewer())
                }
                DeveloperModelRole::Fixer => {
                    (providers.fixer(), identity.interaction.models.fixer())
                }
            };
            if super::provider_selection_provenance(profile, choice) != expected {
                return Ok(DeveloperRequestAdmission::Stale);
            }
            let current = self
                .service
                .select_provider(profile, choice)
                .map_err(|error| super::port_error("verify selected run provider", error))?;
            if current.profile().profile_id() != request.profile_id() {
                return Err(DeveloperLoopError::RecoveryRequired(
                    "the resolved provider profile changed without an explicit selection change"
                        .to_owned(),
                ));
            }
        }
        let options = &identity.interaction;
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
        let continuation = identity
            .continuation_sources
            .iter()
            .rev()
            .find(|source| !source.settled)
            .copied();
        let continuation_incorporated = continuation.is_some_and(|source| {
            identity.interaction.incorporated >= source.generation
        });
        if continuation.is_none()
            && self
                .service
                .refresh_request_files(start, request)
                .map_err(|error| {
                    port_error("refresh files attached to the provider request", error)
                })?
        {
            return Ok(DeveloperRequestAdmission::Stale);
        }
        let (admission, request_sources, control_inputs) = self
            .service
            .with_control_conversation(start.conversation(), |store| {
                let captured = match continuation {
                    None => store.capture_execution(start)?,
                    Some(source) if continuation_incorporated => store
                        .capture_execution_revision_incorporated(start, source.revision)?,
                    Some(source) => {
                        store.capture_execution_revision(start, source.revision)?
                    }
                };
                if captured.inputs().generation() != revision {
                    return if continuation.is_none() {
                        Ok((DeveloperRequestAdmission::Stale, None, None))
                    } else {
                        Err(crate::product_control::ControlStoreError::Corrupt(
                            "continuation source generation changed",
                        ))
                    };
                }
                let request_sources = store.request_source_snapshot(&captured)?;
                let control_inputs = captured
                    .inputs()
                    .revisions()
                    .iter()
                    .map(|input| input.text().to_owned())
                    .collect::<Vec<_>>();
                let admission = match continuation {
                    None => store.prepare_execution(start, revision, request),
                    Some(source) if continuation_incorporated => {
                        store.prepare_execution_revision_incorporated(
                            start,
                            source.revision,
                            source.generation,
                            request,
                        )
                    }
                    Some(source) => store.prepare_execution_revision(
                        start,
                        source.revision,
                        source.generation,
                        request,
                    ),
                }?;
                if admission.developer_admission() == DeveloperRequestAdmission::Stale {
                    return Ok((DeveloperRequestAdmission::Stale, None, None));
                }
                let goal = store.reserve_goal_request(
                    start,
                    goal_role(role),
                    request.request_id().expose_for_wire(),
                )?;
                Ok(if goal == peritus_product_runner::control::GoalAdmission::Accepted {
                    (
                        DeveloperRequestAdmission::Accepted,
                        Some(request_sources),
                        Some(control_inputs),
                    )
                } else {
                    (DeveloperRequestAdmission::Stopped, None, None)
                })
            })
            .map_err(|error| {
                port_error("reserve the provider request in durable control state", error.into())
            })?;
        if admission != DeveloperRequestAdmission::Accepted {
            return Ok(admission);
        }
        let request_sources = request_sources.ok_or_else(|| {
            super::port_internal(
                "bind admitted conversation sources",
                "accepted provider request has no exact source snapshot",
            )
        })?;
        let control_inputs = control_inputs.ok_or_else(|| {
            super::port_internal(
                "project accepted user input",
                "accepted provider request has no exact input projection",
            )
        })?;
        let mut input = b"peritus-product-run-provider-request-admission-v1\0".to_vec();
        input.extend_from_slice(self.run_id.as_bytes());
        input.extend_from_slice(&revision.to_be_bytes());
        input.push(match role {
            DeveloperModelRole::Writer => 1,
            DeveloperModelRole::Reviewer => 2,
            DeveloperModelRole::Fixer => 3,
        });
        input.extend_from_slice(request.request_id().expose_for_wire().as_bytes());
        input.extend_from_slice(request.profile_id().as_bytes());
        input.extend_from_slice(
            selection_provenance
                .unwrap_or_else(|| peritus_types::Sha256Digest::new([0; 32]))
                .as_bytes(),
        );
        let expected_attempt = std::sync::Arc::clone(&identity.cancelled);
        let (_, ticket) = self
            .service
            .mutate_run(
                self.run_id,
                Some(&expected_attempt),
                RunMutationKind::InteractionActivity,
                peritus_codec::sha256(&input),
                MutationDisposition::DurabilityRequired,
                move |record| {
                    if record.request.workspace_id() != identity.workspace
                        || record.request.providers() != identity.request.providers()
                        || record.interaction.workbench != identity.start
                        || record.interaction.models != identity.interaction.models
                        || ProductRunService::active_continuation_source(record) != continuation
                    {
                        return Err(ProductRunServiceError::InvalidState);
                    }
                    if let Some(expected) = selection_provenance {
                        let providers = record.request.providers();
                        let (profile, choice) = match role {
                            DeveloperModelRole::Writer => {
                                (providers.writer(), record.interaction.models.writer())
                            }
                            DeveloperModelRole::Reviewer => {
                                (providers.reviewer(), record.interaction.models.reviewer())
                            }
                            DeveloperModelRole::Fixer => {
                                (providers.fixer(), record.interaction.models.fixer())
                            }
                        };
                        if super::provider_selection_provenance(profile, choice) != expected {
                            return Err(ProductRunServiceError::InvalidState);
                        }
                    }
                    ProductRunService::append_control_input_texts(
                        &mut record.interaction,
                        &control_inputs,
                    )?;
                    if revision > record.interaction.incorporated {
                        record.interaction.incorporated = revision;
                        let mode = record.interaction.mode;
                        record.interaction.append(
                            ProductActivityKind::Status,
                            narration::starting(mode),
                            &format!("Input {revision} incorporated into model request"),
                        )?;
                    }
                    Ok(())
                },
            )
            .map_err(|error| port_error("retain provider request admission", error))?;
        self.service
            .await_run_durable(ticket)
            .map_err(|error| port_error("persist provider request admission", error))?;
        self.bind_request_source_snapshot(revision, request_sources)
            .map_err(|error| port_error("bind admitted conversation sources", error))?;
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
        let mut input = b"peritus-product-run-interaction-activity-v1\0".to_vec();
        input.extend_from_slice(self.run_id.as_bytes());
        let (_, ticket) = self
            .service
            .mutate_run(
                self.run_id,
                Some(&self.attempt_cancelled),
                RunMutationKind::InteractionActivity,
                peritus_codec::sha256(&input),
                MutationDisposition::DurabilityRequired,
                move |record| {
                    change(&mut record.interaction, &mut record.progress)?;
                    if record.interaction.mode != ProductInteractionMode::Build
                        && record.snapshot.phase()
                            == peritus_app_protocol::ProductRunPhase::Queued
                    {
                        record.snapshot = crate::product_run::replace_snapshot(
                            &record.snapshot,
                            peritus_app_protocol::ProductRunPhase::Writing,
                            "Responding to the conversation",
                            record.snapshot.summary(),
                        )?;
                    }
                    Ok(())
                },
            )
            .map_err(|error| port_error("record conversation activity", error))?;
        self.service
            .await_run_durable(ticket)
            .map_err(|error| port_error("persist conversation activity", error))
    }
}
