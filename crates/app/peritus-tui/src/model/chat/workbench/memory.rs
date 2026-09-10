//! Explicit project-guidance inspection and revision-fenced lifecycle commands.

use super::{AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest, WorkbenchMode};
use crate::model::decode_hex_16;
use peritus_app_protocol::{
    ControlOperationId, MAX_WORKBENCH_GUIDANCE_PAGE, WellKnownProtocolFeature,
    WorkbenchGuidanceContent, WorkbenchGuidanceForget, WorkbenchGuidancePin,
    WorkbenchGuidanceReason, WorkbenchGuidanceRecord, WorkbenchGuidanceRevision,
    WorkbenchGuidanceSave, WorkbenchGuidanceScope, WorkbenchGuidanceScopeChange,
    WorkbenchGuidanceSelection, WorkbenchGuidanceSource, WorkbenchGuidanceText, WorkbenchIntent,
    WorkbenchMemory, WorkbenchMemoryQuery, WorkbenchMemoryRow, WorkbenchQuery,
};

impl AppModel {
    pub(super) fn memory_command_binding(
        &mut self,
        workspace: peritus_types::WorkspaceId,
    ) -> Option<(WorkbenchQuery, u64)> {
        let valid_memory = self.chat.workbench.memory.as_ref().is_some_and(|memory| {
            self.chat.workbench.mode == WorkbenchMode::Memory
                && memory.query().query().workspace() == workspace
                && self.chat.workbench.selected == Some(memory.query().query())
        });
        let Some(snapshot) = self.chat.workbench.snapshot.as_ref().filter(|snapshot| {
            valid_memory
                && snapshot.query().workspace() == workspace
                && self.chat.workbench.selected == Some(snapshot.query())
        }) else {
            self.notice(
                NoticeLevel::Warning,
                "Inspect /memory and the current conversation before changing guidance; draft retained.",
            );
            return None;
        };
        Some((snapshot.query(), snapshot.revision()))
    }

    pub(in crate::model::chat) fn memory_command(&mut self, arguments: &str) -> Vec<Effect> {
        if !self.memory_available() {
            self.notice(
                NoticeLevel::Warning,
                "Project guidance unavailable/offline; /reconnect or upgrade daemon. Draft retained.",
            );
            return Vec::new();
        }
        let Some(query) = self.chat.workbench.selected else {
            self.notice(
                NoticeLevel::Warning,
                "Select a conversation with /sessions first; draft retained.",
            );
            return Vec::new();
        };
        if self.workbench_request_pending() || self.chat.workbench.unresolved.is_some() {
            self.notice(
                NoticeLevel::Warning,
                "Resolve the pending workbench request before changing guidance; draft retained.",
            );
            return Vec::new();
        }
        self.open_memory_panel();
        let (action, value) = arguments
            .split_once(char::is_whitespace)
            .map_or((arguments, ""), |(action, value)| (action, value.trim()));
        match (action, value) {
            ("", "") => self.inspect_memory(false),
            ("history", "") => self.inspect_memory(true),
            ("more", "") => self.memory_page(true),
            ("previous", "") => self.memory_page(false),
            ("save" | "revise" | "pin" | "unpin" | "project" | "conversation" | "forget", _) => {
                let intent = match self.prepare_memory_mutation(action, value) {
                    Ok(intent) => intent,
                    Err(message) => {
                        self.notice(NoticeLevel::Warning, message);
                        return Vec::new();
                    }
                };
                self.submit_workbench(intent, query.workspace())
            }
            _ => self.bad_memory_command(),
        }
    }

    const fn open_memory_panel(&mut self) {
        self.chat.workbench.mode = WorkbenchMode::Memory;
        self.chat.workbench.images.open = false;
        self.chat.workbench.files.open = false;
        self.chat.workbench.context_mode = None;
        self.chat.workbench.open = true;
    }

    fn inspect_memory(&mut self, include_forgotten: bool) -> Vec<Effect> {
        self.chat.workbench.memory = None;
        self.chat.workbench.set_include_forgotten_memory(include_forgotten);
        if include_forgotten {
            "Reading active guidance and content-free forget markers; no historical content recovery."
                .clone_into(&mut self.chat.workbench.message);
        } else {
            "Reading user-approved guidance eligible for this conversation; no inference."
                .clone_into(&mut self.chat.workbench.message);
        }
        self.refresh_selected_snapshot()
    }

    fn memory_page(&mut self, forward: bool) -> Vec<Effect> {
        let Some(memory) = self.chat.workbench.memory.as_ref() else {
            return Vec::new();
        };
        let size = u32::try_from(MAX_WORKBENCH_GUIDANCE_PAGE).unwrap_or(u32::MAX);
        let offset = if forward {
            let next = memory.query().offset().saturating_add(size);
            if next >= memory.total() {
                return Vec::new();
            }
            next
        } else {
            memory.query().offset().saturating_sub(size)
        };
        self.refresh_memory_at(
            memory.dependency_revision(),
            offset,
            memory.query().include_forgotten(),
        )
    }

    fn prepare_memory_mutation(
        &self,
        action: &str,
        value: &str,
    ) -> Result<WorkbenchIntent, &'static str> {
        let memory = self
            .chat
            .workbench
            .memory
            .as_ref()
            .ok_or("Inspect /memory before changing guidance; draft retained.")?;
        let dependency = memory.dependency_revision();
        if action == "save" {
            let content = user_content(value, WorkbenchGuidanceScope::Project)?;
            return Ok(WorkbenchIntent::SaveGuidance(WorkbenchGuidanceSave::new(
                dependency, content, false,
            )));
        }
        let (id, tail) = split_identity(value, matches!(action, "revise" | "forget"))?;
        let record = active_record(memory, id)?;
        let selection = WorkbenchGuidanceSelection::new(id, record.version().record())
            .map_err(|_| "The selected guidance revision is invalid; refresh /memory.")?;
        let intent = match action {
            "revise" => WorkbenchIntent::ReviseGuidance(WorkbenchGuidanceRevision::new(
                selection,
                dependency,
                user_content(tail, record.content().scope())?,
            )),
            "pin" | "unpin" => WorkbenchIntent::PinGuidance(WorkbenchGuidancePin::new(
                selection,
                dependency,
                action == "pin",
            )),
            "project" | "conversation" => {
                let scope = if action == "project" {
                    WorkbenchGuidanceScope::Project
                } else {
                    WorkbenchGuidanceScope::Conversation(memory.query().query().conversation())
                };
                WorkbenchIntent::ScopeGuidance(WorkbenchGuidanceScopeChange::new(
                    selection, dependency, scope,
                ))
            }
            "forget" => WorkbenchIntent::ForgetGuidance(WorkbenchGuidanceForget::new(
                selection,
                dependency,
                WorkbenchGuidanceReason::new(tail.to_owned()).map_err(
                    |_| "Provide a non-empty bounded reason: /memory forget <32-hex ID> <reason>.",
                )?,
            )),
            _ => return Err("Unknown guidance action; draft retained."),
        };
        Ok(intent)
    }

    fn bad_memory_command(&mut self) -> Vec<Effect> {
        self.notice(
            NoticeLevel::Warning,
            "Use /memory [history | more | previous | save <text> | revise <ID> <text> | pin|unpin|project|conversation <ID> | forget <ID> <reason>]; draft retained.",
        );
        Vec::new()
    }

    pub(in crate::model) fn refresh_memory(&mut self) -> Vec<Effect> {
        self.refresh_memory_at(0, 0, self.chat.workbench.include_forgotten_memory())
    }

    fn refresh_memory_at(
        &mut self,
        dependency_revision: u64,
        offset: u32,
        include_forgotten: bool,
    ) -> Vec<Effect> {
        if !self.memory_available() || self.workbench_request_pending() {
            return Vec::new();
        }
        let Some(query) = self.chat.workbench.selected else { return Vec::new() };
        let Ok(request) =
            WorkbenchMemoryQuery::new(query, dependency_revision, offset, include_forgotten)
        else {
            return Vec::new();
        };
        self.request(
            AppRequestPayload::QueryWorkbenchMemory(request),
            PendingRequest::WorkbenchMemory(request),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model) fn accept_workbench_memory(
        &mut self,
        query: WorkbenchMemoryQuery,
        memory: WorkbenchMemory,
    ) {
        if memory.query() != query
            || self.chat.workbench.selected != Some(query.query())
            || self.chat.workbench.mode != WorkbenchMode::Memory
        {
            return;
        }
        self.chat.workbench.set_include_forgotten_memory(query.include_forgotten());
        self.chat.workbench.memory = Some(memory);
        self.chat.workbench.scroll = 0;
        self.chat.workbench.message.clear();
    }

    fn memory_available(&self) -> bool {
        self.workbench_available()
            && self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::WorkbenchMemory.as_str()
            })
    }
}

fn user_content(
    text: &str,
    scope: WorkbenchGuidanceScope,
) -> Result<WorkbenchGuidanceContent, &'static str> {
    let text = WorkbenchGuidanceText::new(text.to_owned())
        .map_err(|_| "Guidance text must be non-empty, bounded, and free of terminal controls.")?;
    WorkbenchGuidanceContent::new(text, WorkbenchGuidanceSource::UserAuthored, scope)
        .map_err(|_| "Guidance content is invalid; draft retained.")
}

fn split_identity(
    value: &str,
    requires_tail: bool,
) -> Result<(ControlOperationId, &str), &'static str> {
    let (id, tail) =
        value.split_once(char::is_whitespace).map_or((value, ""), |(id, tail)| (id, tail.trim()));
    if requires_tail && tail.is_empty() {
        return Err("This guidance action requires both a 32-hex ID and text; draft retained.");
    }
    if !requires_tail && !tail.is_empty() {
        return Err("This guidance action accepts exactly one 32-hex ID; draft retained.");
    }
    let id = decode_hex_16(id)
        .and_then(|bytes| ControlOperationId::new(bytes).ok())
        .ok_or("Select a 32-hex guidance ID from the current /memory page; draft retained.")?;
    Ok((id, tail))
}

fn active_record(
    memory: &WorkbenchMemory,
    id: ControlOperationId,
) -> Result<&WorkbenchGuidanceRecord, &'static str> {
    memory
        .rows()
        .iter()
        .find_map(|row| match row {
            WorkbenchMemoryRow::Active(record) if record.identity().id() == id => Some(record),
            WorkbenchMemoryRow::Active(_) | WorkbenchMemoryRow::Forgotten(_) => None,
        })
        .ok_or("Select an active guidance ID from the current /memory page; draft retained.")
}
