//! Local commands share durable input admission, without claiming a provider send or usage.
use super::{DeveloperLoopError, LiveConversation, ProductActivityKind, port_error};
use peritus_model_protocol::*;

impl LiveConversation {
    pub(super) fn admit_module_command(&self) -> Result<Option<String>, DeveloperLoopError> {
        self.admit_module_command_for(None)
    }

    pub(super) fn admit_active_module_command(
        &self,
        module_id: &str,
    ) -> Result<Option<String>, DeveloperLoopError> {
        self.admit_module_command_for(Some(module_id))
    }

    fn admit_module_command_for(
        &self,
        active_module: Option<&str>,
    ) -> Result<Option<String>, DeveloperLoopError> {
        let mut records = self
            .service
            .inner
            .records
            .write()
            .map_err(|_| super::port_internal("admit module command", "run lock poisoned"))?;
        let record = records
            .get_mut(&self.run_id)
            .ok_or_else(|| super::port_internal("admit module command", "run missing"))?;
        let result = self
            .service
            .with_controls(false, |store| {
                let control = store.execution_record(&record.interaction.workbench)?;
                // Ordinary chat does not need an extra expansion of attachments and replies.
                let inputs = control.inputs().capture()?;
                let Some(selected) = inputs.included().last() else { return Ok(None) };
                let input = control
                    .inputs()
                    .revisions()
                    .iter()
                    .find(|i| i.selection() == *selected)
                    .ok_or(peritus_product_runner::control::ControlError::NotFound)?;
                let command = input.text().trim();
                if !(command == "/modules"
                    || command == "/module"
                    || command.starts_with("/module "))
                {
                    return Ok(None);
                }
                let admitted = if let Some(module_id) = active_module {
                    let Some(control) = active_control(command, module_id) else {
                        return Ok(None);
                    };
                    control
                } else {
                    command.to_owned()
                };
                let captured = store.capture_execution(&record.interaction.workbench)?;
                if captured.inputs().included().last() != Some(selected) {
                    return Err(peritus_product_runner::control::ControlError::StaleRevision.into());
                }
                // The archive binds a semantic request; admission never asserts a provider send.
                // Keep the host prefix visible and do not reserve a model request or its usage.
                let provider = self
                    .service
                    .select_provider(
                        record.request.providers().writer(),
                        record.interaction.models.writer(),
                    )
                    .map_err(storage_error)?;
                let profile = provider.profile();
                let limits = ProtocolLimits::PRODUCTION;
                let context = captured.conversation_with_guidance()?;
                let request = (|| -> Result<ModelRequest, ProtocolError> {
                    let negotiated = negotiate(
                        profile,
                        RequestedCapabilities::new(
                            &[],
                            &[Capability::ImageInput],
                            profile.limits(),
                        )?,
                    )?;
                    let mut content = vec![ContentBlock::Text(BoundedText::new(context, limits)?)];
                    content.extend(captured.images().iter().cloned().map(ContentBlock::Image));
                    let request = ModelRequest::new(
                        profile,
                        negotiated,
                        RequestId::new(format!(
                            "host-module-command-{}-{}",
                            self.run_id
                                .as_bytes()
                                .iter()
                                .map(|b| format!("{b:02x}"))
                                .collect::<String>(),
                            captured.revision()
                        ))?,
                        vec![Message::new(Role::User, content, limits)?],
                        vec![],
                        ToolChoice::None,
                        ParallelToolPolicy::Disabled,
                        RequestOptions::new(
                            StructuredOutput::Text,
                            ReasoningPolicy::Disabled,
                            GenerationConfig::new(1, vec![], None, None, None)?,
                            CachePolicy::Disabled,
                            PersistencePolicy::LOCAL_FIRST,
                            None,
                            vec![],
                        ),
                        limits,
                    )?;
                    Ok(request)
                })()
                .map_err(storage_error)?;
                let admission = store.prepare_execution(
                    &record.interaction.workbench,
                    captured.inputs().generation(),
                    &request,
                )?;
                if admission.developer_admission()
                    != peritus_agent::DeveloperRequestAdmission::Accepted
                {
                    return Ok(None);
                }
                Ok(Some((admitted, captured.inputs().generation())))
            })
            .map_err(|e| port_error("persist local module command admission", e.into()))?;
        let Some((command, revision)) = result else { return Ok(None) };
        let mut next = record.clone();
        self.service
            .append_control_inputs(&mut next.interaction)
            .map_err(|e| port_error("project local module input", e))?;
        next.interaction.incorporated = revision;
        next.interaction
            .append(
                ProductActivityKind::Status,
                "Module command accepted",
                "Handled locally; no provider inference was requested by command admission.",
            )
            .map_err(|e| port_error("record host command admission", e))?;
        crate::product_run::persistence::write_record(&self.service.inner.directory, &next)
            .map_err(|e| port_error("persist local command activity", e))?;
        *record = next;
        Ok(Some(command))
    }
}

fn active_control(command: &str, module_id: &str) -> Option<String> {
    let arguments = command.strip_prefix("/module")?.trim();
    let (selected, task) = arguments.split_once(char::is_whitespace)?;
    let task = task.trim();
    if selected != module_id
        || !(matches!(task, "pause" | "resume")
            || task.strip_prefix("steer ").is_some_and(|value| !value.trim().is_empty()))
    {
        return None;
    }
    Some(task.to_owned())
}

fn storage_error(error: impl std::fmt::Display) -> crate::product_control::ControlStoreError {
    std::io::Error::other(format!("archive local module command: {error}")).into()
}
