//! Explicit preview launch, interaction, capture, and evidence inspection.

use peritus_app_protocol::{
    AppRequestPayload, ControlOperationId, WellKnownProtocolFeature, WorkbenchCaptureConsent,
    WorkbenchCaptureRequest, WorkbenchCaptureState, WorkbenchCaptureTarget, WorkbenchCommand,
    WorkbenchInputText, WorkbenchIntent, WorkbenchLaunchProfile, WorkbenchLaunchSource,
    WorkbenchLaunchSourceKind, WorkbenchLaunchText, WorkbenchPreviewInput, WorkbenchResultPage,
    WorkbenchResultQuery, WorkbenchReviewFeedback,
};

use crate::model::{AppModel, Effect, NoticeLevel, PendingRequest, View};

const PREVIEW_READINESS_MILLIS: u64 = 2_000;
const PREVIEW_WALL_MILLIS: u64 = 600_000;
mod profile;

impl AppModel {
    pub(in crate::model) fn preview_command(&mut self, arguments: &str) -> Vec<Effect> {
        if !self.preview_available() {
            self.notice(
                NoticeLevel::Warning,
                "Preview controls unavailable/offline; /reconnect or upgrade daemon. Draft retained.",
            );
            return Vec::new();
        }
        let (action, rest) = arguments
            .split_once(char::is_whitespace)
            .map_or((arguments, ""), |(action, rest)| (action, rest.trim()));
        if matches!(action, "" | "results") && rest.is_empty() {
            return self.refresh_selected_preview(true);
        }
        if self.chat.workbench.unresolved.is_some() || self.workbench_request_pending() {
            self.notice(
                NoticeLevel::Warning,
                "Resolve the pending Workbench request before another preview action; draft retained.",
            );
            return Vec::new();
        }
        match action {
            "launch" => self.launch_preview(rest),
            "play" => self.interact_preview(rest),
            "capture" => self.capture_preview(rest),
            "stop" if rest.is_empty() => self.stop_preview(),
            "check" => self.check_preview(rest),
            "feedback" => self.preview_feedback(rest),
            _ => {
                self.notice(
                    NoticeLevel::Warning,
                    "Use /preview [results | launch <executable> [args] | play <text> | capture <window-id> | stop | check <exact stdout> | feedback <message>]; draft retained.",
                );
                Vec::new()
            }
        }
    }

    fn refresh_selected_preview(&mut self, clear_command: bool) -> Vec<Effect> {
        let Some(query) = self.selected_preview_query() else { return Vec::new() };
        self.view = View::Preview;
        if clear_command {
            self.clear_chat_command();
        }
        self.refresh_preview(query)
    }

    pub(in crate::model) fn refresh_preview(&mut self, query: WorkbenchResultQuery) -> Vec<Effect> {
        if self
            .pending
            .values()
            .any(|pending| matches!(pending, PendingRequest::WorkbenchResult(_)))
        {
            return Vec::new();
        }
        self.request(
            AppRequestPayload::QueryWorkbenchResult(query),
            PendingRequest::WorkbenchResult(query),
        )
        .into_iter()
        .collect()
    }

    fn selected_preview_query(&mut self) -> Option<WorkbenchResultQuery> {
        let Some(scope) = self.chat.workbench.selected else {
            self.notice(
                NoticeLevel::Warning,
                "Open the exact Workbench conversation with /sessions before using preview controls; draft retained.",
            );
            return None;
        };
        let Some(run) = self
            .product
            .as_ref()
            .and_then(super::ProductUi::selected_run)
            .map(peritus_app_protocol::ProductRunSnapshot::run_id)
        else {
            self.notice(NoticeLevel::Warning, "Select a coding run before using preview controls.");
            return None;
        };
        Some(WorkbenchResultQuery::new(scope, run))
    }

    fn preview_binding(&mut self) -> Option<(WorkbenchResultQuery, u64)> {
        let query = self.selected_preview_query()?;
        let page = self
            .product
            .as_ref()
            .and_then(|product| product.preview.as_ref())
            .filter(|page| page.query() == query);
        let revision = page.map(WorkbenchResultPage::control_revision).or_else(|| {
            self.chat
                .workbench
                .snapshot
                .as_ref()
                .filter(|snapshot| snapshot.query() == query.query())
                .map(peritus_app_protocol::WorkbenchSnapshot::revision)
        });
        let Some(revision) = revision else {
            self.notice(
                NoticeLevel::Warning,
                "Inspect the selected Workbench conversation before changing its preview; draft retained.",
            );
            return None;
        };
        Some((query, revision))
    }

    fn launch_preview(&mut self, arguments: &str) -> Vec<Effect> {
        let Some(selection) = profile::Selection::parse(arguments) else {
            self.notice(
                NoticeLevel::Warning,
                "Use /preview launch [--build <observed file>] [--source <observed file>] -- <executable> [literal args]. Observe files with /file first; draft retained.",
            );
            return Vec::new();
        };
        let Some((query, revision)) = self.preview_binding() else { return Vec::new() };
        let profile = (|| {
            let (source, build) = self.preview_source_identity(query, &selection)?;
            let executable = WorkbenchLaunchText::new(selection.executable.to_owned())?;
            let arguments = selection
                .arguments
                .iter()
                .map(|word| WorkbenchLaunchText::new((*word).to_owned()))
                .collect::<Result<Vec<_>, _>>()?;
            let root = WorkbenchLaunchText::new(".".to_owned())?;
            WorkbenchLaunchProfile::new(
                query.run(),
                executable,
                arguments,
                root,
                Vec::new(),
                source,
                build,
                PREVIEW_READINESS_MILLIS,
                PREVIEW_WALL_MILLIS,
                true,
            )
        })();
        let Ok(profile) = profile else {
            self.notice(
                NoticeLevel::Warning,
                "Preview requires exact host-observed source/build files in this conversation (use /file), or the managed candidate identity. Refresh stale files; draft retained.",
            );
            return Vec::new();
        };
        self.submit_preview(query, revision, WorkbenchIntent::StartPreview(profile))
    }

    fn interact_preview(&mut self, text: &str) -> Vec<Effect> {
        if text.is_empty() {
            self.notice(NoticeLevel::Warning, "Use /preview play <text>; draft retained.");
            return Vec::new();
        }
        let Some((query, revision, launch)) = self.latest_preview_launch() else {
            return Vec::new();
        };
        let Ok(input) = WorkbenchPreviewInput::new(format!("{text}\n").into_bytes()) else {
            self.notice(NoticeLevel::Warning, "Preview input exceeds the bounded input limit.");
            return Vec::new();
        };
        self.submit_preview(query, revision, WorkbenchIntent::InteractPreview { launch, input })
    }

    fn capture_preview(&mut self, text: &str) -> Vec<Effect> {
        let Some(window) = parse_window_id(text) else {
            self.notice(
                NoticeLevel::Warning,
                "Use /preview capture <nonzero decimal or 0x-prefixed X11 window ID>; draft retained.",
            );
            return Vec::new();
        };
        let Some((query, revision, launch)) = self.latest_preview_launch() else {
            return Vec::new();
        };
        let Ok(target) = WorkbenchCaptureTarget::x11_window(window) else {
            return Vec::new();
        };
        self.submit_preview(
            query,
            revision,
            WorkbenchIntent::CapturePreview(WorkbenchCaptureRequest::new(
                launch,
                target,
                WorkbenchCaptureConsent::Granted,
            )),
        )
    }

    fn stop_preview(&mut self) -> Vec<Effect> {
        let Some((query, revision, launch)) = self.latest_preview_launch() else {
            return Vec::new();
        };
        self.submit_preview(query, revision, WorkbenchIntent::StopPreview { launch })
    }

    fn check_preview(&mut self, observed: &str) -> Vec<Effect> {
        let Some((criterion, observed)) = observed.split_once(" :: ") else {
            self.notice(
                NoticeLevel::Warning,
                "Use /preview check <exact graphical criterion description> :: <exact stdout observation>; draft retained.",
            );
            return Vec::new();
        };
        let Some((query, revision, launch)) = self.latest_preview_launch() else {
            return Vec::new();
        };
        let (Ok(observed), Ok(note)) = (
            WorkbenchLaunchText::new(observed.to_owned()),
            WorkbenchInputText::new(criterion.trim().to_owned()),
        ) else {
            self.notice(NoticeLevel::Warning, "Behavior observation is invalid or too large.");
            return Vec::new();
        };
        self.submit_preview(
            query,
            revision,
            WorkbenchIntent::CheckPreviewBehavior { launch, observed, note },
        )
    }

    fn preview_feedback(&mut self, message: &str) -> Vec<Effect> {
        if message.is_empty() {
            self.notice(NoticeLevel::Warning, "Use /preview feedback <message>; draft retained.");
            return Vec::new();
        }
        let Some((query, revision)) = self.preview_binding() else { return Vec::new() };
        let capture = self
            .product
            .as_ref()
            .and_then(|product| product.preview.as_ref())
            .filter(|page| page.query() == query)
            .and_then(|page| {
                page.launches()
                    .iter()
                    .rev()
                    .flat_map(|launch| launch.captures().iter().rev())
                    .find(|capture| capture.state() == WorkbenchCaptureState::Captured)
                    .map(peritus_app_protocol::WorkbenchCaptureReceipt::operation)
            });
        let Some(capture) = capture else {
            self.notice(
                NoticeLevel::Warning,
                "Capture a selected app window successfully before attaching feedback.",
            );
            return Vec::new();
        };
        let Ok(message) = WorkbenchInputText::new(message.to_owned()) else {
            self.notice(NoticeLevel::Warning, "Feedback is invalid or too large.");
            return Vec::new();
        };
        self.submit_preview(
            query,
            revision,
            WorkbenchIntent::AddArtifactFeedback {
                capture,
                feedback: WorkbenchReviewFeedback::RequestRevision,
                message,
                region: None,
            },
        )
    }

    fn latest_preview_launch(&mut self) -> Option<(WorkbenchResultQuery, u64, ControlOperationId)> {
        let (query, revision) = self.preview_binding()?;
        let launch = self
            .product
            .as_ref()
            .and_then(|product| product.preview.as_ref())
            .filter(|page| page.query() == query)
            .and_then(|page| page.launches().last())
            .map(peritus_app_protocol::WorkbenchLaunchResult::launch);
        let Some(launch) = launch else {
            self.notice(NoticeLevel::Warning, "Launch a preview before using this action.");
            return None;
        };
        Some((query, revision, launch))
    }

    fn submit_preview(
        &mut self,
        query: WorkbenchResultQuery,
        revision: u64,
        intent: WorkbenchIntent,
    ) -> Vec<Effect> {
        let Ok(operation) = ControlOperationId::new(self.ids.bytes(b"workbench-preview-operation"))
        else {
            return Vec::new();
        };
        let command = WorkbenchCommand::new(operation, query.query(), revision, intent);
        let Some(effect) = self.request(
            AppRequestPayload::WorkbenchCommand(command.clone()),
            PendingRequest::WorkbenchControl(command.clone()),
        ) else {
            return Vec::new();
        };
        self.chat.workbench.unresolved = Some((command, self.chat.buffer.clone()));
        self.chat.workbench.open = false;
        self.view = View::Preview;
        self.notice(NoticeLevel::Info, "Awaiting durable preview receipt; no result inferred.");
        vec![effect]
    }

    pub(in crate::model) fn accept_preview_page(
        &mut self,
        query: WorkbenchResultQuery,
        page: WorkbenchResultPage,
    ) {
        if page.query() != query
            || self.chat.workbench.selected != Some(query.query())
            || self
                .product
                .as_ref()
                .and_then(super::ProductUi::selected_run)
                .is_none_or(|run| run.run_id() != query.run())
        {
            return;
        }
        if let Some(product) = &mut self.product {
            product.preview = Some(page);
            product.preview_scroll = 0;
        }
    }

    pub(in crate::model) fn preview_available(&self) -> bool {
        self.context.is_some()
            && self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::WorkbenchPreview.as_str()
            })
    }
}

fn parse_window_id(text: &str) -> Option<u64> {
    let value = text.trim();
    if value.is_empty() || value.contains(char::is_whitespace) {
        return None;
    }
    let parsed = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .map_or_else(|| value.parse().ok(), |hex| u64::from_str_radix(hex, 16).ok())?;
    (parsed != 0).then_some(parsed)
}
