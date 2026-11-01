use peritus_model_protocol::*;
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, ModelStream, OwnedModelStream, ProviderCoreError,
};
use peritus_types::{ProviderProfileId, Sha256Digest};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

pub(super) fn profile(model: &str) -> Result<ProviderProfile, String> {
    ProviderProfile::new(
        ProviderProfileId::new([76; 16]).map_err(|e| format!("{e:?}"))?,
        1,
        ProviderName::new("praxis-recording".into()).map_err(|e| e.to_string())?,
        ModelName::new(model.into()).map_err(|e| e.to_string())?,
        WireDialect::OpenAiResponses,
        CapabilityMatrix::new(
            &[Capability::ToolCalls, Capability::ParallelToolCalls, Capability::ReasoningControls],
            &[],
        )
        .map_err(|e| e.to_string())?,
        CapabilityProvenance::Probed,
        ModelLimits::new(200_000, 8192, 64, 8, 1).map_err(|e| e.to_string())?,
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .map_err(|e| e.to_string())
}
fn fail(detail: impl std::fmt::Debug) -> ProviderCoreError {
    eprintln!("recording provider fixture: {detail:?}");
    ProviderCoreError::configuration("praxis_fixture", "fixture dependency failed; see child log")
}
pub(super) fn providers(root: &Path) -> Option<Vec<Arc<dyn ModelProvider>>> {
    let fixture = PathBuf::from(std::env::var_os("PRAXIS_DAEMON_ROOT")?);
    if root != fixture.join("state") {
        return None;
    }
    let count =
        (0u64..64).take_while(|turn| fixture.join(format!("request-{turn}.json")).exists()).count();
    Some(vec![Arc::new(Recording {
        profile: profile("fixture-a").expect("valid fixture profile"),
        root: fixture,
        serial: Arc::new(AtomicU64::new(count as u64)),
    })])
}
struct Recording {
    profile: ProviderProfile,
    root: PathBuf,
    serial: Arc<AtomicU64>,
}
impl ModelProvider for Recording {
    fn profile(&self) -> &ProviderProfile {
        &self.profile
    }
    fn select_model(&self, model: ModelName) -> Result<Arc<dyn ModelProvider>, ProviderCoreError> {
        if !matches!(model.as_str(), "fixture-a" | "fixture-b") {
            return Err(fail("unsupported model"));
        }
        Ok(Arc::new(Self {
            profile: profile(model.as_str()).map_err(fail)?,
            root: self.root.clone(),
            serial: self.serial.clone(),
        }))
    }
    fn start(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async move {
            peritus_provider_core::validate_request_profile(&self.profile, &request)?;
            let turn = self.serial.fetch_add(1, Ordering::AcqRel);
            let messages = request
                .messages()
                .iter()
                .flat_map(|m| {
                    m.content()
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text(t) => Some(t.expose_for_wire().to_owned()),
                            ContentBlock::ToolResult(t) => Some(
                                String::from_utf8_lossy(t.output().canonical_bytes()).into_owned(),
                            ),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let effort = match request.options().reasoning() {
                ReasoningPolicy::Disabled => "disabled",
                ReasoningPolicy::Adaptive { .. } => "adaptive",
                ReasoningPolicy::Effort { effort, .. } => effort.as_str(),
            };
            let body = json!({"turn":turn,"model":request.model().as_str(),"effort":effort,"request_id":request.request_id().expose_for_wire(),"tool_count":request.tools().len(),"messages":messages});
            super::fixture::save(&self.root.join(format!("request-{turn}.json")), &body)
                .map_err(fail)?;
            let mut f = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.root.join("requests.jsonl"))
                .map_err(|e| fail(e.to_string()))?;
            writeln!(f, "{body}").and_then(|()| f.sync_all()).map_err(|e| fail(e.to_string()))?;
            let script: Value = fs::read(self.root.join(format!("turn-{turn}.json")))
                .ok()
                .map(|b| serde_json::from_slice(&b))
                .transpose()
                .map_err(|e| fail(e.to_string()))?
                .unwrap_or_else(|| json!({"text":"done"}));
            Ok(OwnedModelStream::new(
                Stream {
                    events: events(&script, self.profile.model(), turn).map_err(fail)?,
                    root: self.root.clone(),
                    turn,
                    started: false,
                    released: false,
                    sequence: 0,
                },
                cancel,
            ))
        })
    }
}
struct Stream {
    events: VecDeque<ModelEvent>,
    root: PathBuf,
    turn: u64,
    started: bool,
    released: bool,
    sequence: u64,
}
impl ModelStream for Stream {
    fn next<'a>(
        &'a mut self,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move {
            if self.started && !self.released {
                while !self.root.join(format!("release-{}", self.turn)).exists() {
                    if cancel.is_cancelled() {
                        self.events.clear();
                        self.events.push_back(ModelEvent::ResponseCancelled);
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
                self.released = true;
            }
            self.started = true;
            let Some(event) = self.events.pop_front() else { return Ok(None) };
            self.sequence += 1;
            EventEnvelope::new(self.sequence, None, None, Sha256Digest::new([42; 32]), event)
                .map(Some)
                .map_err(fail)
        })
    }
}
fn events(
    script: &Value,
    model: &ModelName,
    turn: u64,
) -> Result<VecDeque<ModelEvent>, ProtocolError> {
    let id = ItemId::new(format!("item_{turn}"))?;
    let mut events =
        vec![ModelEvent::ResponseStarted { response_id: None, model: Some(model.clone()) }];
    if let Some(tool) = script["tool"].as_str() {
        let call = ToolCallId::new(format!("fixture_call_{turn}"))?;
        events.extend([
            ModelEvent::ItemStarted { item_id: id.clone(), index: 0, kind: ItemKind::ToolCall },
            ModelEvent::ToolCallStarted {
                item_id: id.clone(),
                call_id: call.clone(),
                name: ToolName::new(tool.into())?,
            },
            ModelEvent::ToolArgumentDelta {
                call_id: call,
                fragment: StreamFragment::new(
                    script["arguments"].to_string().into_bytes(),
                    ProtocolLimits::PRODUCTION,
                )?,
            },
            ModelEvent::ItemCompleted(id),
            ModelEvent::Finish(FinishReason::ToolCalls),
        ]);
    } else {
        events.extend([
            ModelEvent::ItemStarted { item_id: id.clone(), index: 0, kind: ItemKind::Message },
            ModelEvent::TextDelta {
                item_id: id.clone(),
                fragment: StreamFragment::new(
                    script["text"].as_str().unwrap_or("done").as_bytes().to_vec(),
                    ProtocolLimits::PRODUCTION,
                )?,
            },
            ModelEvent::ItemCompleted(id),
            ModelEvent::Finish(FinishReason::Stop),
        ]);
    }
    events.push(ModelEvent::ResponseCompleted);
    Ok(events.into())
}
