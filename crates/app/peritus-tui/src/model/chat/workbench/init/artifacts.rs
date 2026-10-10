//! Bounded immutable review paging and incremental initialization choices.
use super::{
    AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest, WellKnownProtocolFeature,
    WorkbenchIntent, WorkbenchMode,
};
use peritus_app_protocol::{
    InitArtifactDiscovery, InitArtifactPage, InitArtifactPageRequest, InitArtifactProposal,
    InitSourceKind, InitSourceSelection,
};

impl AppModel {
    pub(in crate::model) fn init_artifacts_available(&self) -> bool {
        self.features.iter().any(|feature| {
            feature.as_str() == WellKnownProtocolFeature::WorkbenchInitArtifacts.as_str()
        })
    }
    pub(in crate::model::chat::workbench) fn init_artifact_binding(
        &mut self,
        proposal: InitArtifactProposal,
        workspace: peritus_types::WorkspaceId,
    ) -> Option<(peritus_app_protocol::WorkbenchQuery, u64)> {
        if self.chat.workbench.mode != WorkbenchMode::Init
            || self.chat.workbench.init_artifact != Some(proposal)
            || self.chat.workbench.selected != Some(proposal.query())
            || workspace != proposal.query().workspace()
            || self.chat.workbench.init_artifact_page.is_none()
        {
            self.notice(
                NoticeLevel::Warning,
                "Inspect /init before applying its complete exact proposal; draft retained.",
            );
            return None;
        }
        Some((proposal.query(), proposal.request().revision()))
    }
    pub(super) fn request_init_artifacts(&mut self, request: InitArtifactDiscovery) -> Vec<Effect> {
        self.request(
            AppRequestPayload::DiscoverInitArtifacts(request.clone()),
            PendingRequest::WorkbenchInitArtifacts(request),
        )
        .into_iter()
        .collect()
    }
    fn request_init_page(&mut self, proposal: InitArtifactProposal, offset: u64) -> Vec<Effect> {
        let Ok(request) = InitArtifactPageRequest::new(proposal, offset, 32 * 1024) else {
            return Vec::new();
        };
        self.request(
            AppRequestPayload::QueryInitArtifactPage(request),
            PendingRequest::WorkbenchInitArtifactPage(request),
        )
        .into_iter()
        .collect()
    }
    pub(in crate::model) fn accept_init_artifact(
        &mut self,
        request: &InitArtifactDiscovery,
        proposal: InitArtifactProposal,
    ) -> Vec<Effect> {
        if proposal.request() != request.request()
            || self.chat.workbench.selected != Some(proposal.query())
            || self.chat.workbench.mode != WorkbenchMode::Init
        {
            return Vec::new();
        }
        self.chat.workbench.init = None;
        self.chat.workbench.init_artifact = Some(proposal);
        self.chat.workbench.init_artifact_page = None;
        self.chat.workbench.scroll = 0;
        self.request_init_page(proposal, 0)
    }
    pub(in crate::model) fn accept_init_artifact_page(
        &mut self,
        request: InitArtifactPageRequest,
        page: InitArtifactPage,
    ) {
        if page.request() != request
            || self.chat.workbench.init_artifact != Some(request.proposal())
            || self.chat.workbench.selected != Some(request.proposal().query())
        {
            return;
        }
        self.chat.workbench.init_artifact_page = Some(page);
        self.chat.workbench.scroll = 0;
        self.chat.workbench.message.clear();
    }
    pub(super) fn init_artifact_command(
        &mut self,
        arguments: &str,
        query: peritus_app_protocol::WorkbenchQuery,
    ) -> Vec<Effect> {
        if arguments == "decline" {
            self.chat.workbench.init = None;
            self.chat.workbench.init_artifact = None;
            self.chat.workbench.init_artifact_page = None;
            self.chat.workbench.mode = WorkbenchMode::Sessions;
            self.clear_chat_command();
            return Vec::new();
        }
        let Some(proposal) =
            self.chat.workbench.init_artifact.filter(|proposal| proposal.query() == query)
        else {
            self.notice(
                NoticeLevel::Warning,
                "Inspect /init before selecting sources or commands; draft retained.",
            );
            return Vec::new();
        };
        match arguments {
            "apply" => {
                return self.submit_workbench(
                    WorkbenchIntent::ApplyInitArtifact(proposal),
                    query.workspace(),
                );
            }
            "next" => {
                if let Some(offset) =
                    self.chat.workbench.init_artifact_page.as_ref().and_then(InitArtifactPage::next)
                {
                    return self.request_init_page(proposal, offset);
                }
                return Vec::new();
            }
            "previous" => {
                let offset = self.chat.workbench.init_artifact_page.as_ref().map_or(0, |page| {
                    page.request().offset().saturating_sub(u64::from(page.request().maximum()))
                });
                return self.request_init_page(proposal, offset);
            }
            _ => {}
        }
        let selection = parse_selection(arguments, proposal);
        if let Some(selection) = selection {
            return self.request_init_artifacts(selection);
        }
        self.notice(NoticeLevel::Warning, "Use /init source <manifest|commands|docs|instructions> <path>, /init command <number>, /init next|previous|apply|decline. Draft retained.");
        Vec::new()
    }
}

fn parse_selection(
    arguments: &str,
    proposal: InitArtifactProposal,
) -> Option<InitArtifactDiscovery> {
    arguments.strip_prefix("source ").map_or_else(
        || {
            arguments
                .strip_prefix("command ")
                .and_then(|number| number.parse::<u64>().ok())
                .and_then(|number| number.checked_sub(1))
                .and_then(|ordinal| {
                    InitArtifactDiscovery::new(
                        proposal.request(),
                        Some(proposal.manifest()),
                        None,
                        Some(ordinal),
                    )
                    .ok()
                })
        },
        |source| {
            let (kind, path) = source.split_once(' ').unwrap_or(("", ""));
            let kind = match kind {
                "manifest" => Some(InitSourceKind::Manifest),
                "commands" => Some(InitSourceKind::CommandConfig),
                "docs" => Some(InitSourceKind::Documentation),
                "instructions" => Some(InitSourceKind::Instructions),
                _ => None,
            };
            kind.and_then(|kind| InitSourceSelection::new(path.to_owned(), kind).ok()).and_then(
                |source| {
                    InitArtifactDiscovery::new(
                        proposal.request(),
                        Some(proposal.manifest()),
                        Some(source),
                        None,
                    )
                    .ok()
                },
            )
        },
    )
}
