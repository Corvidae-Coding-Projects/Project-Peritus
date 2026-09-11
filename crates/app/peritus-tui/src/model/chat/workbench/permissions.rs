//! Effective permission inspection and explicit revision-fenced restriction changes.

use super::{AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest, WorkbenchMode};
use peritus_app_protocol::{
    WellKnownProtocolFeature, WorkbenchIntent, WorkbenchPermissionCapability,
    WorkbenchPermissionChange, WorkbenchPermissions, WorkbenchQuery,
};

impl AppModel {
    pub(super) fn permission_command_binding(
        &mut self,
        workspace: peritus_types::WorkspaceId,
    ) -> Option<(WorkbenchQuery, u64)> {
        let Some(permissions) = self.chat.workbench.permissions.as_ref().filter(|value| {
            self.chat.workbench.mode == WorkbenchMode::Permissions
                && value.query().workspace() == workspace
                && self.chat.workbench.selected == Some(value.query())
        }) else {
            self.notice(
                NoticeLevel::Warning,
                "Inspect /permissions before changing a capability; draft retained.",
            );
            return None;
        };
        Some((permissions.query(), permissions.conversation_revision()))
    }

    pub(in crate::model::chat) fn permissions_command(&mut self, arguments: &str) -> Vec<Effect> {
        if !self.permissions_available() {
            self.notice(
                NoticeLevel::Warning,
                "Permission inspection unavailable/offline; /reconnect or upgrade daemon. Draft retained.",
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
                "Resolve the pending workbench request before changing permissions; draft retained.",
            );
            return Vec::new();
        }
        self.chat.workbench.mode = WorkbenchMode::Permissions;
        self.chat.workbench.images.open = false;
        self.chat.workbench.files.open = false;
        self.chat.workbench.context_mode = None;
        self.chat.workbench.open = true;
        if arguments.is_empty() {
            self.chat.workbench.permissions = None;
            "Reading the currently enforced host-policy intersection; no authority changes."
                .clone_into(&mut self.chat.workbench.message);
            return self.refresh_permissions();
        }
        let Some((action, capability)) = arguments.split_once(char::is_whitespace) else {
            return self.bad_permissions_command();
        };
        let Some(capability) = permission_capability(capability.trim()) else {
            return self.bad_permissions_command();
        };
        let allowed = match action {
            "restrict" => false,
            "grant" => true,
            _ => return self.bad_permissions_command(),
        };
        let Some(permissions) =
            self.chat.workbench.permissions.as_ref().filter(|value| value.query() == query)
        else {
            self.notice(
                NoticeLevel::Warning,
                "Inspect /permissions before changing a capability; draft retained.",
            );
            return Vec::new();
        };
        let Some(entry) =
            permissions.entries().iter().find(|entry| entry.capability() == capability)
        else {
            return Vec::new();
        };
        if allowed && !entry.host_allowed() {
            self.notice(
                NoticeLevel::Warning,
                "The immutable host policy denies this capability; /permissions cannot broaden it.",
            );
            return Vec::new();
        }
        if entry.effective_allowed() == allowed {
            self.notice(
                NoticeLevel::Info,
                "The effective capability already has that state; no change was submitted.",
            );
            return Vec::new();
        }
        let change =
            WorkbenchPermissionChange::new(permissions.authority_revision(), capability, allowed);
        self.submit_workbench(WorkbenchIntent::SetPermissions(change), query.workspace())
    }

    fn bad_permissions_command(&mut self) -> Vec<Effect> {
        self.notice(
            NoticeLevel::Warning,
            "Use /permissions [restrict | grant] <read | write | process | network>; draft retained.",
        );
        Vec::new()
    }

    pub(super) fn refresh_permissions(&mut self) -> Vec<Effect> {
        if !self.permissions_available() || self.workbench_request_pending() {
            return Vec::new();
        }
        let Some(query) = self.chat.workbench.selected else { return Vec::new() };
        self.request(
            AppRequestPayload::QueryWorkbenchPermissions(query),
            PendingRequest::WorkbenchPermissions(query),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model) fn accept_workbench_permissions(
        &mut self,
        query: WorkbenchQuery,
        permissions: WorkbenchPermissions,
    ) {
        if permissions.query() != query
            || self.chat.workbench.selected != Some(query)
            || self.chat.workbench.mode != WorkbenchMode::Permissions
        {
            return;
        }
        self.chat.workbench.permissions = Some(permissions);
        self.chat.workbench.scroll = 0;
        self.chat.workbench.message.clear();
    }

    fn permissions_available(&self) -> bool {
        self.workbench_available()
            && self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::WorkbenchPermissions.as_str()
            })
    }
}

fn permission_capability(value: &str) -> Option<WorkbenchPermissionCapability> {
    match value {
        "read" => Some(WorkbenchPermissionCapability::Read),
        "write" => Some(WorkbenchPermissionCapability::Write),
        "process" => Some(WorkbenchPermissionCapability::Process),
        "network" => Some(WorkbenchPermissionCapability::Network),
        _ => None,
    }
}
