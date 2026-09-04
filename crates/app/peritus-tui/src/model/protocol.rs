//! Protocol message admission and daemon-derived state projection.

use super::{
    Acknowledgement, AppEventEnvelope, AppEventPayload, AppMessage, AppModel, AppRequestEnvelope,
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ControlEnvelope, ControlPayload,
    EVENT_CAPACITY, Effect, EventRecord, FAMILIES, HeartbeatReply, NoticeLevel, PendingRequest,
    PromptItem, PromptPhase, ProtocolContext, SubscriptionFilter, SubscriptionRequest,
    TerminalSession, View, inert_preview,
};

impl AppModel {
    pub(super) fn start_session(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        if let Some(effect) = self.request(AppRequestPayload::DaemonStatus, PendingRequest::Status)
        {
            effects.push(effect);
        }
        effects.extend(self.poll_product_runs());
        let Some(subscription_id) = self.ids.subscription() else {
            self.notice(NoticeLevel::Error, "failed to allocate subscription identity");
            return effects;
        };
        let topics = vec!["system.all".to_owned()];
        let filter = match SubscriptionFilter::new(
            topics,
            self.limits.max_topics(),
            self.limits.codec().max_string_bytes,
        ) {
            Ok(filter) => filter,
            Err(error) => {
                self.notice(NoticeLevel::Error, error.to_string());
                return effects;
            }
        };
        let window = u32::try_from(self.limits.max_in_flight_events()).unwrap_or(u32::MAX);
        let subscription =
            match SubscriptionRequest::new(subscription_id, filter, self.last_cursor, window, true)
            {
                Ok(subscription) => subscription,
                Err(error) => {
                    self.notice(NoticeLevel::Error, error.to_string());
                    return effects;
                }
            };
        self.subscription = Some(subscription_id);
        if let Some(effect) =
            self.request(AppRequestPayload::Subscribe(subscription), PendingRequest::Subscribe)
        {
            effects.push(effect);
        }
        effects
    }

    pub(super) fn request(
        &mut self,
        payload: AppRequestPayload,
        kind: PendingRequest,
    ) -> Option<Effect> {
        let context = self.context?;
        let request = self.ids.request()?;
        let correlation = self.ids.correlation()?;
        match AppRequestEnvelope::new(context, request, correlation, payload) {
            Ok(envelope) => {
                self.pending.insert(request, kind);
                Some(Effect::Send(AppMessage::Request(envelope)))
            }
            Err(error) => {
                self.notice(NoticeLevel::Error, error.to_string());
                None
            }
        }
    }

    pub(super) fn handle_message(&mut self, message: AppMessage) -> Vec<Effect> {
        match message {
            AppMessage::Response(response) => self.handle_response(&response),
            AppMessage::Event(event) => self.handle_event(&event),
            AppMessage::ClientHello(_)
            | AppMessage::ServerHello(_)
            | AppMessage::Request(_)
            | AppMessage::Control(_) => {
                self.notice(NoticeLevel::Error, "daemon sent an illegal post-negotiation frame");
                Vec::new()
            }
        }
    }

    fn context_matches(&mut self, context: ProtocolContext) -> bool {
        if self.context == Some(context) {
            true
        } else {
            self.notice(NoticeLevel::Error, "daemon frame used a foreign protocol context");
            false
        }
    }

    fn handle_response(&mut self, response: &AppResponseEnvelope) -> Vec<Effect> {
        if !self.context_matches(response.context()) {
            return Vec::new();
        }
        let pending = self.pending.remove(&response.request_id());
        match response.payload() {
            AppResponsePayload::SubscriptionStarted(started) => {
                self.subscription = Some(started.subscription_id());
                self.notice(
                    NoticeLevel::Info,
                    format!("live event stream resumed after #{}", started.after().get()),
                );
            }
            AppResponsePayload::DaemonStatus(status) => {
                self.daemon_status = Some(status.clone());
            }
            AppResponsePayload::TerminalAttached(binding) => {
                match TerminalSession::new(*binding, self.limits.max_terminal_chunk_bytes()) {
                    Ok(terminal) => {
                        self.terminal = Some(terminal);
                        self.view = View::Terminal;
                        self.notice(
                            NoticeLevel::Info,
                            "terminal attached; Ctrl-] releases keyboard capture",
                        );
                    }
                    Err(error) => self.notice(NoticeLevel::Error, error.to_string()),
                }
            }
            AppResponsePayload::PromptAccepted(prompt_id) => {
                self.set_prompt_phase(*prompt_id, PromptPhase::Accepted);
                self.notice(NoticeLevel::Info, "prompt response accepted as protocol input");
            }
            AppResponsePayload::Acknowledged(_) => match pending {
                Some(PendingRequest::TerminalDetach) => {
                    self.terminal = None;
                    self.notice(NoticeLevel::Info, "terminal detached");
                }
                Some(PendingRequest::TerminalCancel) => {
                    self.notice(NoticeLevel::Warning, "terminal cancellation was acknowledged");
                }
                _ => {}
            },
            AppResponsePayload::Error(error) => {
                if let Some(PendingRequest::Prompt(prompt_id)) = pending {
                    self.set_prompt_phase(prompt_id, PromptPhase::Failed);
                }
                self.notice(
                    NoticeLevel::Error,
                    format!(
                        "{} / {} / retry {}{}",
                        error.subsystem().as_str(),
                        error.code().as_str(),
                        error.retry().as_str(),
                        error
                            .diagnostic()
                            .map(|value| format!(": {}", value.as_str()))
                            .unwrap_or_default()
                    ),
                );
            }
            AppResponsePayload::CommandResult(result) => {
                self.notice(NoticeLevel::Info, format!("command result: {result:?}"));
            }
            AppResponsePayload::ArtifactOpened(metadata) => {
                self.notice(
                    NoticeLevel::Info,
                    format!("artifact transfer opened: {} bytes", metadata.byte_size()),
                );
            }
            AppResponsePayload::ShutdownAccepted(_) => {
                self.notice(NoticeLevel::Warning, "daemon accepted graceful shutdown request");
            }
            AppResponsePayload::ProductRunAccepted(snapshot) => {
                self.accept_product_run(snapshot.clone());
                self.notice(NoticeLevel::Info, format!("coding run: {}", snapshot.status()));
            }
            AppResponsePayload::ProductRuns(snapshots) => {
                self.accept_product_runs(snapshots.clone());
            }
            AppResponsePayload::ProductRunSettled(settled) => {
                self.accept_product_settlement(settled);
                self.notice(
                    NoticeLevel::Info,
                    format!("coding run settled: {:?}", settled.settlement().disposition()),
                );
            }
            AppResponsePayload::ProductRunSettlements(settled) => {
                self.accept_product_settlements(settled);
            }
            AppResponsePayload::ProductRunConversation(conversation) => {
                self.accept_product_conversation(conversation.clone());
            }
        }
        Vec::new()
    }

    fn handle_event(&mut self, event: &AppEventEnvelope) -> Vec<Effect> {
        if !self.context_matches(event.context()) {
            return Vec::new();
        }
        match event.payload() {
            AppEventPayload::DomainEvent(delivery) => self.accept_delivery(delivery),
            AppEventPayload::PromptRequested(binding) => {
                let prompt_id = binding.correlation().prompt_id();
                if !self
                    .prompts
                    .iter()
                    .any(|item| item.binding.correlation().prompt_id() == prompt_id)
                {
                    self.prompts
                        .push(PromptItem { binding: binding.clone(), phase: PromptPhase::Pending });
                }
                self.view = View::Approvals;
                self.selected_prompt = self.prompts.len().saturating_sub(1);
                self.notice(NoticeLevel::Warning, "daemon is waiting for human input");
                Vec::new()
            }
            AppEventPayload::TerminalOutput(output) => {
                if let Some(terminal) = &mut self.terminal
                    && let Err(error) = terminal.accept_output(output)
                {
                    self.notice(NoticeLevel::Error, format!("terminal ordering error: {error}"));
                }
                Vec::new()
            }
            AppEventPayload::TerminalExited(exit) => {
                if let Some(terminal) = &mut self.terminal {
                    if let Err(error) = terminal.accept_exit(*exit) {
                        self.notice(NoticeLevel::Error, format!("terminal exit error: {error}"));
                    } else {
                        self.notice(
                            NoticeLevel::Warning,
                            format!("terminal exited: {:?}", exit.disposition()),
                        );
                    }
                }
                Vec::new()
            }
            payload => self.handle_system_event(payload),
        }
    }

    fn handle_system_event(&mut self, payload: &AppEventPayload) -> Vec<Effect> {
        match payload {
            AppEventPayload::ReadinessChanged(status) => {
                self.daemon_status = Some(status.clone());
                self.notice(
                    NoticeLevel::Info,
                    format!("daemon readiness changed: {:?}", status.readiness()),
                );
                Vec::new()
            }
            AppEventPayload::Diagnostic(diagnostic) => {
                self.notice_effect(NoticeLevel::Warning, diagnostic.as_str())
            }
            AppEventPayload::Heartbeat(heartbeat) => {
                self.daemon_status = Some(heartbeat.status().clone());
                let Some(context) = self.context else {
                    return Vec::new();
                };
                let Some(correlation) = self.ids.correlation() else {
                    return Vec::new();
                };
                vec![Effect::Send(AppMessage::Control(ControlEnvelope::new(
                    context,
                    correlation,
                    ControlPayload::HeartbeatReply(HeartbeatReply::new(
                        heartbeat.heartbeat_id(),
                        heartbeat.sequence(),
                    )),
                )))]
            }
            AppEventPayload::SubscriptionGap { gap, .. } => {
                self.notice(
                    NoticeLevel::Error,
                    format!(
                        "event history gap: requested {}, retained {}..={}",
                        gap.requested().get(),
                        gap.earliest().get(),
                        gap.latest().get()
                    ),
                );
                Vec::new()
            }
            AppEventPayload::Backpressure(backpressure) => {
                self.notice(
                    NoticeLevel::Warning,
                    format!(
                        "event stream backpressured at {} (acknowledged {})",
                        backpressure.last_delivered().get(),
                        backpressure.last_acknowledged().get()
                    ),
                );
                Vec::new()
            }
            AppEventPayload::ShutdownProgress(progress) => {
                self.notice(
                    NoticeLevel::Warning,
                    format!(
                        "daemon shutdown {}/{}; {} work items remain",
                        progress.completed_steps(),
                        progress.total_steps(),
                        progress.remaining().len()
                    ),
                );
                Vec::new()
            }
            AppEventPayload::ShutdownComplete(complete) => {
                self.notice(
                    NoticeLevel::Warning,
                    format!(
                        "daemon shutdown completed {:?}; {} work items remain",
                        complete.disposition(),
                        complete.remaining().len()
                    ),
                );
                Vec::new()
            }
            AppEventPayload::ArtifactMetadata(metadata) => self.notice_effect(
                NoticeLevel::Info,
                format!("artifact metadata received: {} bytes", metadata.byte_size()),
            ),
            AppEventPayload::ArtifactChunk(chunk) => self.notice_effect(
                NoticeLevel::Info,
                format!("artifact chunk {} received", chunk.ordinal()),
            ),
            AppEventPayload::ArtifactComplete(complete) => self.notice_effect(
                NoticeLevel::Info,
                format!("artifact transfer complete: {} bytes", complete.byte_size()),
            ),
            AppEventPayload::DomainEvent(_)
            | AppEventPayload::PromptRequested(_)
            | AppEventPayload::TerminalOutput(_)
            | AppEventPayload::TerminalExited(_) => Vec::new(),
        }
    }

    fn notice_effect(&mut self, level: NoticeLevel, text: impl Into<String>) -> Vec<Effect> {
        self.notice(level, text);
        Vec::new()
    }

    fn accept_delivery(&mut self, delivery: &peritus_app_protocol::Delivery) -> Vec<Effect> {
        self.last_cursor = self.last_cursor.max(delivery.cursor());
        if self.seen_events.insert(delivery.event_id()) {
            let family = delivery.frame().family();
            let family_name = FAMILIES
                .iter()
                .find(|entry| entry.tag == family)
                .map_or("unknown-event", |entry| entry.name);
            let record = EventRecord {
                event_id: delivery.event_id(),
                cursor: delivery.cursor(),
                family,
                family_name,
                schema: delivery.frame().schema_version(),
                attempt: delivery.attempt(),
                digest: delivery.frame().digest().into_bytes(),
                byte_len: delivery.frame().bytes().len(),
                preview: inert_preview(delivery.frame().bytes(), 240),
            };
            self.events.push_back(record);
            while self.events.len() > EVENT_CAPACITY {
                if let Some(removed) = self.events.pop_front() {
                    self.seen_events.remove(&removed.event_id);
                    self.selected_event = self.selected_event.map(|index| index.saturating_sub(1));
                }
            }
            if self.selected_event.is_none() {
                self.selected_event = self.events.len().checked_sub(1);
            }
        }

        let Some(context) = self.context else {
            return Vec::new();
        };
        let Some(correlation) = self.ids.correlation() else {
            return Vec::new();
        };
        vec![Effect::Send(AppMessage::Control(ControlEnvelope::new(
            context,
            correlation,
            ControlPayload::Acknowledge(Acknowledgement::new(
                delivery.subscription_id(),
                delivery.cursor(),
            )),
        )))]
    }
}
