//! Live authority is rechecked for each host child, outside the memory owner lock.

use super::error;
use crate::{ConversationView, control::PermissionCapability, local_context::memory::LocalMemory};
use peritus_agent::{
    DeveloperControlFlow, DeveloperLoopError, DeveloperModelRole, DeveloperToolEffect,
};
use std::sync::Arc;

pub(super) struct Admission {
    conversation: Arc<dyn ConversationView>,
    role: Option<DeveloperModelRole>,
    invocation: String,
    revision: u64,
    cancellation:
        Option<(Arc<std::sync::atomic::AtomicBool>, peritus_provider_core::CancellationToken)>,
}

pub(in crate::local_context) fn revision(memory: &LocalMemory) -> Result<u64, DeveloperLoopError> {
    let view =
        memory.conversation.as_ref().ok_or_else(|| error("plan has no current input owner"))?;
    match view.interaction() {
        Some(port) => Ok(port.input()?.revision),
        None => Ok(view.revision()),
    }
}

impl Admission {
    pub(super) fn new(
        memory: &LocalMemory,
        role: Option<DeveloperModelRole>,
        parent: &str,
        revision: u64,
    ) -> Result<Self, DeveloperLoopError> {
        let conversation =
            memory.conversation.clone().ok_or_else(|| error("plan has no current input owner"))?;
        if conversation.interaction().is_some() && role.is_none() {
            return Err(error("interactive plan has no host-supplied model role"));
        }
        let identity = format!(
            "inspection-plan-v1:{}:{}:{}",
            memory.transcript.invocation, memory.transcript.request_prefix, parent
        );
        Ok(Self {
            conversation,
            role,
            revision,
            cancellation: memory.cancellation.clone(),
            invocation: format!(
                "inspection-plan-{}",
                crate::local_context::tools::hex(
                    peritus_codec::sha256(identity.as_bytes()).as_bytes()
                )
            ),
        })
    }

    pub(super) fn current(&self) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        if self.cancellation.as_ref().is_some_and(|(run, provider)| {
            run.load(std::sync::atomic::Ordering::Acquire) || provider.is_cancelled()
        }) {
            return Ok(DeveloperControlFlow::Stop);
        }
        if !self.conversation.effective_permissions().allows(PermissionCapability::Read) {
            return Ok(DeveloperControlFlow::Yield);
        }
        if let Some(port) = self.conversation.interaction() {
            return port.tool_control(self.revision);
        }
        let revision = self.conversation.revision();
        Ok(if revision == self.revision {
            DeveloperControlFlow::Continue
        } else {
            DeveloperControlFlow::Yield
        })
    }

    pub(super) fn started(
        &self,
        call: &peritus_model_protocol::CompletedToolCall,
    ) -> Result<(), DeveloperLoopError> {
        if let Some(port) = self.conversation.interaction() {
            port.observe(peritus_agent::DeveloperActivity::ToolStarted {
                name: call.name().as_str(),
                arguments: &call.arguments().to_wire_string(),
            })?;
        }
        Ok(())
    }

    pub(super) fn finished(
        &self,
        call: &peritus_model_protocol::CompletedToolCall,
        output: &peritus_agent::DeveloperToolObservation,
    ) -> Result<(), DeveloperLoopError> {
        if let Some(port) = self.conversation.interaction() {
            port.observe(peritus_agent::DeveloperActivity::ToolFinished {
                name: call.name().as_str(),
                output: &output.output.to_wire_string(),
                is_error: output.is_error,
            })?;
        }
        Ok(())
    }

    pub(super) fn admit(&self, sequence: u32) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        let current = self.current()?;
        if current != DeveloperControlFlow::Continue {
            return Ok(current);
        }
        match (self.conversation.interaction(), self.role) {
            (Some(port), Some(role)) => port.admit_tool(
                role,
                &self.invocation,
                sequence,
                self.revision,
                DeveloperToolEffect::ReadOnly,
            ),
            _ => Ok(DeveloperControlFlow::Continue),
        }
    }

    pub(super) fn complete(
        &self,
        sequence: u32,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        match (self.conversation.interaction(), self.role) {
            (Some(port), Some(role)) => {
                let control = port.complete_tool(role, &self.invocation, sequence)?;
                if control == DeveloperControlFlow::Continue { self.current() } else { Ok(control) }
            }
            _ => self.current(),
        }
    }
}
