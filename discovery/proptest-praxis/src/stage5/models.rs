//! Constructive event histories, an input-derived item table, usage ledger and frozen C5 corpus.
use peritus_model_protocol::*;
use peritus_types::{ProviderProfileId, Sha256Digest};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Digest;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Case {
    Stream {
        kinds: Vec<u8>,
        text: String,
        seed: u32,
        chunk: u8,
        defect: u8,
        finish: u8,
        reverse: bool,
        event_limit: u16,
        output_limit: u16,
    },
    ForkReplay {
        prefix: String,
        suffix: String,
        cut: u8,
        chunk: u8,
        repeats: u8,
        fresh_sequence: bool,
        tool: bool,
    },
    Usage {
        observations: Vec<(u8, [Option<u64>; 8])>,
    },
    Corpus {
        index: u8,
        drift: u8,
        cut: u16,
    },
    Dialect {
        tag: u8,
    },
}
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let text = prop::sample::select(vec!["é🦀界", "x", "\nquoted\"\\", "two\u{301}", "snow ☃"]);
    let stream = (
        prop::collection::vec(0_u8..6, 1..=4),
        text,
        any::<u32>(),
        1_u8..17,
        0_u8..17,
        0_u8..12,
        any::<bool>(),
        prop::sample::select(vec![16_u16, 128, 4096]),
        prop::sample::select(vec![16_u16, 128, 4096]),
    )
        .prop_map(
            |(kinds, text, seed, chunk, defect, finish, reverse, event_limit, output_limit)| {
                Case::Stream {
                    kinds,
                    text: text.into(),
                    seed,
                    chunk,
                    defect,
                    finish,
                    reverse,
                    event_limit,
                    output_limit,
                }
            },
        );
    let count = prop_oneof![
        Just(None),
        any::<u16>().prop_map(|x| Some(u64::from(x))),
        Just(Some(u64::MAX))
    ];
    let usage =
        prop::collection::vec((0_u8..3, prop::array::uniform8(count)), 0..=max_steps.min(64))
            .prop_map(|observations| Case::Usage { observations });
    let fork = (
        "[a-zé界🦀]{0,16}",
        "[a-zé界🦀]{0,12}",
        any::<u8>(),
        1_u8..16,
        1_u8..5,
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(prefix, suffix, cut, chunk, repeats, fresh_sequence, tool)| {
            Case::ForkReplay { prefix, suffix, cut, chunk, repeats, fresh_sequence, tool }
        });
    prop_oneof![8=>stream,5=>fork,3=>usage,2=>(0_u8..10,0_u8..7,any::<u16>()).prop_map(|(index,drift,cut)|Case::Corpus{index,drift,cut}),1=>(0_u8..11).prop_map(|tag|Case::Dialect{tag})].boxed()
}
fn error(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
}
fn bump(m: &mut crate::Metrics, k: impl Into<String>) {
    *m.entry(k.into()).or_default() += 1;
}
fn counters(v: [Option<u64>; 8]) -> UsageCounters {
    UsageCounters::new(v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7])
}
fn fields(v: UsageCounters) -> [Option<u64>; 8] {
    [
        v.input_tokens(),
        v.cached_input_tokens(),
        v.cache_creation_input_tokens(),
        v.output_tokens(),
        v.reasoning_output_tokens(),
        v.tool_tokens(),
        v.total_tokens(),
        v.provider_cost_microunits(),
    ]
}
fn usage(observations: &[(u8, [Option<u64>; 8])]) -> Result<crate::Metrics, String> {
    let mut actual = UsageTracker::new();
    let mut expected = [None; 8];
    let mut final_seen = false;
    let mut m = crate::Metrics::new();
    for (scope, next) in observations {
        let step = *scope % 3 == 0;
        let valid = step
            || (!final_seen
                && (0..8).all(|i| !matches!((expected[i],next[i]),(Some(a),Some(b)) if b<a)));
        let before = actual;
        let result = actual.observe(&UsageObservation::new(
            match *scope % 3 {
                0 => UsageScope::Step,
                1 => UsageScope::Cumulative,
                _ => UsageScope::Final,
            },
            counters(*next),
            None,
        ));
        if result.is_ok() != valid {
            return Err("usage acceptance differs from independent optional-counter ledger".into());
        }
        if valid && !step {
            for i in 0..8 {
                if next[i].is_some() {
                    expected[i] = next[i];
                }
            }
            final_seen = *scope % 3 == 2;
        }
        if !valid && actual != before {
            return Err("rejected usage changed retained state".into());
        }
        if fields(actual.high_water()) != expected || actual.saw_final() != final_seen {
            return Err("usage high water/final identity drift".into());
        }
        bump(&mut m, if valid { "accepted_usage" } else { "rejected_usage" });
        if step {
            bump(&mut m, "step_usage_not_added");
        }
        if final_seen {
            bump(&mut m, "final_usage_observations");
        }
    }
    Ok(m)
}
fn item(n: usize) -> Result<ItemId, String> {
    ItemId::new(format!("item-{n}")).map_err(error)
}
fn call(n: usize) -> Result<ToolCallId, String> {
    ToolCallId::new(format!("call-{n}")).map_err(error)
}
fn kind(n: u8) -> ItemKind {
    match n % 6 {
        0 => ItemKind::Message,
        1 => ItemKind::StructuredOutput,
        2 => ItemKind::ToolCall,
        3 => ItemKind::Reasoning,
        4 => ItemKind::Refusal,
        _ => ItemKind::ProviderNative,
    }
}
fn finish(n: u8, l: ProtocolLimits) -> Result<FinishReason, String> {
    Ok(match n % 10 {
        0 => FinishReason::Stop,
        1 => FinishReason::Length,
        2 => FinishReason::ToolCalls,
        3 => FinishReason::Safety,
        4 => FinishReason::Refusal,
        5 => FinishReason::Pause,
        6 => FinishReason::ContextLimit,
        7 => FinishReason::Cancelled,
        8 => FinishReason::Incomplete,
        _ => FinishReason::Provider(BoundedText::new("unfamiliar".into(), l).map_err(error)?),
    })
}
fn terminal(t: Option<&TerminalOutcome>) -> &'static str {
    match t {
        None => "open",
        Some(TerminalOutcome::Succeeded { .. }) => "success",
        Some(TerminalOutcome::RequiresAction { .. }) => "action",
        Some(TerminalOutcome::Refused { .. }) => "refused",
        Some(TerminalOutcome::Incomplete { .. }) => "incomplete",
        Some(TerminalOutcome::Cancelled) => "cancelled",
        Some(TerminalOutcome::Failed(_)) => "failed",
    }
}
fn view(items: &[ReducedItem]) -> Vec<Value> {
    items
        .iter()
        .map(|i| match i {
            ReducedItem::Text { item_id, index, text } => {
                json!([0, item_id.expose_for_wire(), index, text.expose_for_wire()])
            }
            ReducedItem::Structured { item_id, index, value } => json!([
                1,
                item_id.expose_for_wire(),
                index,
                serde_json::from_slice::<Value>(value.canonical_bytes()).unwrap()
            ]),
            ReducedItem::ToolCall { item_id, index, call } => json!([
                2,
                item_id.expose_for_wire(),
                index,
                call.id().expose_for_wire(),
                call.name().as_str(),
                serde_json::from_slice::<Value>(call.arguments().canonical_bytes()).unwrap()
            ]),
            ReducedItem::Reasoning { item_id, index, summary, replay } => json!([
                3,
                item_id.expose_for_wire(),
                index,
                summary.as_ref().map(|s| s.expose_for_wire()),
                replay
            ]),
            ReducedItem::Refusal { item_id, index, text } => {
                json!([4, item_id.expose_for_wire(), index, text.expose_for_wire()])
            }
            ReducedItem::ProviderNative { item_id, index, bytes } => {
                json!([5, item_id.expose_for_wire(), index, bytes])
            }
        })
        .collect()
}
struct Run {
    actual: ResponseReducer,
    limits: ProtocolLimits,
    seq: u64,
    events: usize,
    output: usize,
    expected: BTreeMap<u32, Value>,
    metrics: crate::Metrics,
}
impl Run {
    fn send(
        &mut self,
        event: ModelEvent,
        tag: u8,
        bytes: usize,
        valid: bool,
    ) -> Result<bool, String> {
        self.seq += 1;
        self.events += 1;
        self.output += bytes;
        let expected = valid
            && self.events <= self.limits.max_events()
            && self.output <= self.limits.max_output_bytes();
        let e = EventEnvelope::new(
            self.seq,
            Some(self.seq),
            Some(EventId::new(format!("e{}", self.seq)).map_err(error)?),
            Sha256Digest::new([tag; 32]),
            event,
        )
        .map_err(error)?;
        let encoded = encode_event_envelope(&e, self.limits).map_err(error)?;
        // Header and closed tag are independently specified, not a round-trip-derived expectation.
        let tag_at = 18 + 9 + 1 + 4 + format!("e{}", self.seq).len() + 32;
        if &encoded[..10] != b"P5EV\0\x01\0\x01\0\0" || encoded.get(tag_at) != Some(&tag) {
            return Err("event codec changed frozen header or semantic tag".into());
        }
        if decode_event_envelope(&encoded, self.limits).map_err(error)? != e {
            return Err("event codec lost exact synthetic input semantics".into());
        }
        let mut trailing = encoded.clone();
        trailing.push(0);
        if decode_event_envelope(&trailing, self.limits).is_ok()
            || decode_event_envelope(&encoded[..encoded.len() - 1], self.limits).is_ok()
        {
            return Err("event decoder accepted trailing/truncated bytes".into());
        }
        bump(&mut self.metrics, format!("event_tag_{tag}"));
        bump(&mut self.metrics, "event_codec_checks");
        let before = view(self.actual.completed_items());
        let result = self.actual.push(e);
        if result.is_ok() != expected {
            return Err(format!(
                "event {tag} acceptance mismatch seq{} expected={expected} result={result:?}",
                self.seq
            ));
        }
        if !expected {
            if terminal(self.actual.terminal()) != "failed"
                || view(self.actual.completed_items()) != before
            {
                return Err(
                    "malformed event did not retain completed prefix and fail closed".into()
                );
            }
            let stopped = self.actual.terminal().cloned();
            for n in 0..3 {
                let event = match n {
                    0 => ModelEvent::Heartbeat,
                    1 => ModelEvent::Finish(FinishReason::Stop),
                    _ => ModelEvent::ResponseCompleted,
                };
                if self
                    .actual
                    .push(
                        EventEnvelope::new(
                            self.seq + 1,
                            None,
                            None,
                            Sha256Digest::new([0; 32]),
                            event,
                        )
                        .map_err(error)?,
                    )
                    .is_ok()
                    || self.actual.terminal() != stopped.as_ref()
                {
                    return Err("malformed stream resumed or changed terminal".into());
                }
            }
            bump(&mut self.metrics, "malformed_then_continued_rejections");
            return Ok(false);
        }
        if self.actual.response_id().map(ResponseId::expose_for_wire) != Some("response") {
            return Err("reducer lost the independently supplied response identity".into());
        }
        bump(&mut self.metrics, "accepted_events");
        Ok(true)
    }
    fn compare(&self) -> Result<(), String> {
        if view(self.actual.completed_items())
            != self.expected.values().cloned().collect::<Vec<_>>()
        {
            return Err("completed items differ from input-derived exact semantic table".into());
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
fn stream(
    kinds: &[u8],
    text: &str,
    seed: u32,
    chunk: u8,
    defect: u8,
    end: u8,
    reverse: bool,
    event_limit: u16,
    output_limit: u16,
) -> Result<crate::Metrics, String> {
    if kinds.is_empty()
        || kinds.len() > 6
        || text.len() > 128
        || event_limit == 0
        || output_limit == 0
    {
        return Err("setup-incomplete: unsupported model fixture bounds".into());
    }
    let l = ProtocolLimits::new([
        64,
        64,
        256,
        256,
        256,
        16,
        2048,
        usize::from(event_limit),
        16,
        256,
        if defect == 14 { 8 } else { usize::from(output_limit) },
        256,
        256,
    ])
    .map_err(error)?;
    let mut r = Run {
        actual: ResponseReducer::new(ProviderName::new("fixture".into()).map_err(error)?, l),
        limits: l,
        seq: 0,
        events: 0,
        output: 0,
        expected: BTreeMap::new(),
        metrics: crate::Metrics::new(),
    };
    macro_rules! emit {
        ($e:expr,$tag:expr,$bytes:expr,$valid:expr) => {
            if !r.send($e, $tag, $bytes, $valid)? {
                return Ok(r.metrics);
            }
        };
    }
    emit!(
        ModelEvent::ResponseStarted {
            response_id: Some(ResponseId::new("response".into()).map_err(error)?),
            model: Some(ModelName::new("selected-model".into()).map_err(error)?)
        },
        1,
        0,
        true
    );
    if defect == 6 {
        emit!(
            ModelEvent::ResponseIdentity(ResponseId::new("different".into()).map_err(error)?),
            2,
            0,
            false
        );
    }
    emit!(
        ModelEvent::ResponseIdentity(ResponseId::new("response".into()).map_err(error)?),
        2,
        0,
        true
    );
    if defect == 7 || defect == 8 {
        let sequence = if reverse { r.seq + 1 } else { r.seq };
        let digest = if defect == 7 { [2; 32] } else { [255; 32] };
        let e = EventEnvelope::new(
            sequence,
            Some(r.seq),
            Some(EventId::new(format!("e{}", r.seq)).map_err(error)?),
            Sha256Digest::new(digest),
            ModelEvent::ResponseIdentity(ResponseId::new("response".into()).map_err(error)?),
        )
        .map_err(error)?;
        let result = r.actual.push(e);
        if defect == 8 {
            if result.is_ok() || terminal(r.actual.terminal()) != "failed" {
                return Err("conflicting exact event ID did not fail closed".into());
            }
            bump(&mut r.metrics, "conflicting_duplicate_rejections");
            return Ok(r.metrics);
        }
        if result != Ok(ReducerTransition::DuplicateIgnored) {
            return Err("exact event retransmission was not inert".into());
        }
        r.seq = sequence;
        bump(&mut r.metrics, "exact_duplicate_events");
    }
    if defect == 9 {
        r.seq += 1;
        emit!(ModelEvent::Heartbeat, 16, 0, false);
    }
    let mut kinds = kinds.to_vec();
    if matches!(defect, 3 | 4 | 11 | 16) {
        kinds[0] = 2;
    }
    if defect == 16 {
        if kinds.len() == 1 {
            kinds.push(2);
        }
        kinds[1] = 2;
    }
    if defect == 5 {
        kinds[0] = 0;
    }
    if defect == 13 {
        kinds[0] = 3;
    }
    for (n, k) in kinds.iter().enumerate() {
        emit!(
            ModelEvent::ItemStarted {
                item_id: item(n)?,
                index: if reverse { (kinds.len() - n - 1) as u32 } else { n as u32 },
                kind: kind(*k)
            },
            3,
            0,
            true
        );
    }
    if defect == 15 {
        emit!(
            ModelEvent::ItemStarted { item_id: item(0)?, index: 100, kind: ItemKind::Message },
            3,
            0,
            false
        );
    }
    if defect == 12 {
        emit!(
            ModelEvent::ItemStarted {
                item_id: ItemId::new("other".into()).map_err(error)?,
                index: 0,
                kind: ItemKind::Message
            },
            3,
            0,
            false
        );
    }
    let mut payloads = Vec::new();
    for (n, k) in kinds.iter().enumerate() {
        if *k == 2 && !(defect == 11 && n == 0) {
            emit!(
                ModelEvent::ToolCallStarted {
                    item_id: item(n)?,
                    call_id: if defect == 16 && n == 1 { call(0)? } else { call(n)? },
                    name: ToolName::new(format!("read-{n}")).map_err(error)?
                },
                8,
                0,
                !(defect == 16 && n == 1)
            );
        }
        let payload = match *k {
            1 | 2 => {
                format!("{{\"n\":{seed},\"text\":{}}}", serde_json::to_string(text).map_err(error)?)
                    .into_bytes()
            }
            5 => vec![0xff, (seed % 256) as u8, 0],
            _ => format!("{n}:{text}").into_bytes(),
        };
        payloads.push(if n == 0 && defect == 4 {
            match seed % 3 {
                0 => b"[]".to_vec(),
                1 => b"{\"x\":".to_vec(),
                _ => b"{\"x\":1,\"x\":2}".to_vec(),
            }
        } else if n == 0 && defect == 5 {
            vec![0xff]
        } else {
            payload
        });
    }
    let width = usize::from(chunk.clamp(1, 32));
    let maximum = payloads.iter().map(Vec::len).max().unwrap_or(0);
    for start in (0..maximum).step_by(width) {
        for (n, k) in kinds.iter().enumerate() {
            if start >= payloads[n].len() {
                continue;
            }
            let bytes = payloads[n][start..(start + width).min(payloads[n].len())].to_vec();
            let len = bytes.len();
            let fragment = StreamFragment::new(bytes, l).map_err(error)?;
            let (tag, event) = match *k {
                2 => (9, ModelEvent::ToolArgumentDelta { call_id: call(n)?, fragment }),
                3 => (5, ModelEvent::ReasoningSummaryDelta { item_id: item(n)?, fragment }),
                4 => (7, ModelEvent::RefusalDelta { item_id: item(n)?, fragment }),
                _ => (4, ModelEvent::TextDelta { item_id: item(n)?, fragment }),
            };
            emit!(event, tag, len, !(defect == 11 && n == 0));
        }
    }
    if defect == 13 {
        emit!(
            ModelEvent::TextDelta {
                item_id: item(0)?,
                fragment: StreamFragment::new(b"wrong".to_vec(), l).map_err(error)?
            },
            4,
            5,
            false
        );
    }
    for (n, k) in kinds.iter().enumerate() {
        if *k == 3 {
            emit!(
                ModelEvent::ReasoningReplayDelta {
                    item_id: item(n)?,
                    fragment: StreamFragment::new(vec![0xff, 0, 1], l).map_err(error)?
                },
                6,
                3,
                true
            );
        }
    }
    for n in (0..kinds.len()).rev() {
        if defect == 2 && n == 0 {
            continue;
        }
        let k = kinds[n];
        let p = &payloads[n];
        let parsed = super::json_ref::parse(p).ok();
        let valid = match k {
            0 | 4 => std::str::from_utf8(p).is_ok_and(|s| !s.is_empty() && !s.contains('\0')),
            1 => parsed.is_some(),
            2 => parsed.as_ref().is_some_and(Value::is_object),
            _ => true,
        };
        emit!(ModelEvent::ItemCompleted(item(n)?), 10, 0, valid);
        let index = if reverse { (kinds.len() - n - 1) as u32 } else { n as u32 };
        let id = format!("item-{n}");
        let expected = match k {
            0 | 4 => json!([k, id, index, std::str::from_utf8(p).map_err(error)?]),
            1 => json!([k, id, index, parsed]),
            2 => json!([k, id, index, format!("call-{n}"), format!("read-{n}"), parsed]),
            3 => json!([k, id, index, std::str::from_utf8(p).map_err(error)?, [255, 0, 1]]),
            _ => json!([k, id, index, p]),
        };
        r.expected.insert(index, expected);
        r.compare()?;
        bump(&mut r.metrics, format!("completed_kind_{k}"));
        if defect == 3 && n == 0 {
            emit!(
                ModelEvent::ToolArgumentDelta {
                    call_id: call(0)?,
                    fragment: StreamFragment::new(b"late".to_vec(), l).map_err(error)?
                },
                9,
                4,
                false
            );
        }
    }
    emit!(
        ModelEvent::Usage(UsageObservation::new(
            UsageScope::Final,
            counters([Some(u64::from(seed)); 8]),
            None
        )),
        11,
        0,
        true
    );
    if fields(r.actual.usage_high_water()) != [Some(u64::from(seed)); 8] {
        return Err("reducer dropped/changed normalized usage".into());
    }
    emit!(
        ModelEvent::RateLimit(
            RateLimitObservation::new(vec![
                RateLimitWindow::new(
                    RateLimitDimension::Requests,
                    Some(8),
                    Some(3),
                    Some(ResetTime::AfterMillis(11))
                )
                .map_err(error)?
            ])
            .map_err(error)?
        ),
        12,
        0,
        true
    );
    emit!(
        ModelEvent::Cache(CacheObservation::new(CacheStatus::Hit, None, Some(3), Some(9))),
        13,
        0,
        true
    );
    emit!(
        ModelEvent::ProviderEvent(ProviderExtension::new(
            ExtensionName::new("fixture".into()).map_err(error)?,
            CanonicalJson::parse("{\"x\":1}", JsonBounds::value(l)).map_err(error)?
        )),
        15,
        0,
        true
    );
    if r.actual.rate_limits().len() != 1
        || r.actual.rate_limits()[0].windows().len() != 1
        || r.actual.rate_limits()[0].windows()[0].remaining() != Some(3)
        || r.actual.rate_limits()[0].windows()[0].limit() != Some(8)
    {
        return Err("rate-limit metadata changed or vanished".into());
    }
    if r.actual.cache_observations().len() != 1
        || r.actual.cache_observations()[0].status() != CacheStatus::Hit
        || r.actual.cache_observations()[0].input_tokens() != Some(3)
        || r.actual.cache_observations()[0].ttl_seconds() != Some(9)
    {
        return Err("cache observation changed or vanished".into());
    }
    if r.actual.provider_events().len() != 1
        || r.actual.provider_events()[0].name().as_str() != "fixture"
        || r.actual.provider_events()[0].value().canonical_bytes() != b"{\"x\":1}"
    {
        return Err("ancillary provider metadata changed or vanished".into());
    }
    if fields(r.actual.usage_high_water()) != [Some(u64::from(seed)); 8] {
        return Err("ancillary observations changed cumulative usage".into());
    }
    emit!(ModelEvent::Heartbeat, 16, 0, true);
    emit!(ModelEvent::Finish(finish(end, l)?), 14, 0, true);
    if defect == 10 {
        emit!(ModelEvent::Finish(FinishReason::Stop), 14, 0, false);
    }
    if defect == 1 {
        if r.actual.finish_eof().is_ok() || terminal(r.actual.terminal()) != "failed" {
            return Err("EOF invented an explicit terminal".into());
        }
        bump(&mut r.metrics, "incomplete_eof_rejections");
        return Ok(r.metrics);
    }
    let mut expected_failure = None;
    if end == 10 {
        emit!(ModelEvent::ResponseCancelled, 19, 0, true);
    } else if end == 11 {
        let f = ModelFailure::new(
            ProviderName::new("fixture".into()).map_err(error)?,
            FailureCategory::Transport,
            TransportPhase::StreamObserved,
            OutcomeCertainty::AcceptedPartial,
            Retryability::Never,
            None,
            None,
            None,
            RedactedDiagnostic::new("fixture_failure".into(), None, None, None).map_err(error)?,
        );
        expected_failure = Some(f.clone());
        emit!(ModelEvent::ResponseFailed(f), 18, 0, true);
    } else {
        emit!(ModelEvent::ResponseCompleted, 17, 0, defect != 2);
    }
    let expected = if end == 10 {
        "cancelled"
    } else if end == 11 {
        "failed"
    } else {
        match end % 10 {
            0 => "success",
            2 | 5 => "action",
            3 | 4 => "refused",
            7 => "cancelled",
            _ => "incomplete",
        }
    };
    let reason = finish(end, l)?;
    let expected_terminal = if end == 10 {
        TerminalOutcome::Cancelled
    } else if end == 11 {
        TerminalOutcome::Failed(expected_failure.ok_or("missing explicit failure fixture")?)
    } else {
        match end % 10 {
            0 => TerminalOutcome::Succeeded { reason },
            2 | 5 => TerminalOutcome::RequiresAction { reason },
            3 | 4 => TerminalOutcome::Refused { reason },
            7 => TerminalOutcome::Cancelled,
            _ => TerminalOutcome::Incomplete { reason },
        }
    };
    if r.actual.terminal() != Some(&expected_terminal) {
        return Err("full terminal reason/failure differs from input-derived exact terminal".into());
    }
    if r.actual.finish_eof().map_err(error)? != expected_terminal {
        return Err("EOF changed the independently expected explicit terminal".into());
    }
    let t = Some(expected_terminal);
    let e = EventEnvelope::new(
        r.seq + 1,
        None,
        None,
        Sha256Digest::new([0; 32]),
        ModelEvent::Heartbeat,
    )
    .map_err(error)?;
    if r.actual.push(e).is_ok() || r.actual.terminal() != t.as_ref() {
        return Err("event accepted after explicit terminal".into());
    }
    r.compare()?;
    bump(&mut r.metrics, "exact_terminal_streams");
    bump(&mut r.metrics, format!("terminal_{expected}"));
    Ok(r.metrics)
}

fn corpus(
    index: u8,
    drift: u8,
    cut: u16,
    dialect_tag: Option<u8>,
) -> Result<crate::Metrics, String> {
    let names = [
        "minimal",
        "realistic",
        "boundary",
        "codex-runtime",
        "claude-runtime",
        "corrupt-truncated",
        "unknown-tag",
        "trailing-field",
        "invalid-option",
        "invalid-boolean",
    ];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/model/peritus-model-protocol/fixtures/v1");
    let name = names[usize::from(index % 10)];
    let hex = std::fs::read_to_string(root.join(format!("{name}.hex"))).map_err(error)?;
    let compact = hex.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let mut bytes = Vec::new();
    for n in (0..compact.len()).step_by(2) {
        bytes.push(u8::from_str_radix(&compact[n..n + 2], 16).map_err(error)?);
    }
    let sums = std::fs::read_to_string(root.join("SHA256SUMS")).map_err(error)?;
    let expected = sums
        .lines()
        .find(|line| line.ends_with(&format!("  {name}.hex")))
        .and_then(|line| line.split_whitespace().next())
        .ok_or("corpus digest missing")?;
    let actual_sha256 =
        sha2::Sha256::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    if actual_sha256 != expected {
        return Err("frozen C5 corpus checksum mismatch".into());
    }
    if dialect_tag.is_some() && (!index.is_multiple_of(10) || drift != 0) {
        return Err("dialect mutation requires pristine minimal corpus".into());
    }
    if let Some(tag) = dialect_tag {
        bytes[52] = tag;
    }
    let accepted = index % 10 < 5 && dialect_tag.is_none_or(|tag| (1..=9).contains(&tag));
    let original_dialect = match index % 10 {
        3 => WireDialect::OpenAiCodexRuntime,
        4 => WireDialect::AnthropicClaudeRuntime,
        _ => WireDialect::CompatibleResponses,
    };
    let dialect = match dialect_tag {
        Some(1) => WireDialect::OpenAiResponses,
        Some(2) => WireDialect::AnthropicMessages,
        Some(3) => WireDialect::GeminiInteractionsV1,
        Some(4) => WireDialect::GeminiGenerateContentV1,
        Some(5) => WireDialect::CompatibleResponses,
        Some(6) => WireDialect::CompatibleChatCompletions,
        Some(7) => WireDialect::OpenAiCodexRuntime,
        Some(8) => WireDialect::AnthropicClaudeRuntime,
        Some(9) => WireDialect::OpenCodeRuntime,
        _ => original_dialect,
    };
    let caps = CapabilityMatrix::new(
        &[
            Capability::Streaming,
            Capability::ToolCalls,
            Capability::ParallelToolCalls,
            Capability::StrictStructuredOutput,
            Capability::PromptCaching,
            Capability::ImageInput,
            Capability::AudioInput,
            Capability::DocumentInput,
            Capability::ReasoningControls,
            Capability::ReasoningSummaries,
            Capability::ResumableResponse,
            Capability::ConfirmedCancellation,
            Capability::UsageDetail,
            Capability::RateLimitDetail,
            Capability::StoredState,
            Capability::ProviderExtensions,
            Capability::SamplingControls,
            Capability::ReasoningReplay,
        ],
        &[],
    )
    .map_err(error)?;
    let profile = ProviderProfile::new(
        ProviderProfileId::new([if drift == 1 { 18 } else { 17 }; 16]).map_err(error)?,
        if drift == 2 { 8 } else { 7 },
        ProviderName::new(if drift == 3 { "other" } else { "fixture-provider" }.into())
            .map_err(error)?,
        ModelName::new(if drift == 4 { "other" } else { "fixture-model-v1" }.into())
            .map_err(error)?,
        if drift == 5 { WireDialect::AnthropicMessages } else { dialect },
        caps,
        CapabilityProvenance::Probed,
        ModelLimits::new(u64::MAX, u64::MAX, u32::MAX, u32::MAX, u64::MAX).map_err(error)?,
        OutputLimitEnforcement::Advisory,
        StateMode::BackgroundResumable,
        ResumeKind::ExactCursor,
        CancellationKind::Confirmed,
    )
    .map_err(error)?;
    if drift == 6 && accepted {
        let end = usize::from(cut) % bytes.len();
        bytes.truncate(end);
    }
    let request = decode_request(
        &bytes,
        &profile,
        RequestId::new("caller-owned".into()).map_err(error)?,
        ProtocolLimits::PRODUCTION,
    );
    if request.is_ok() != (accepted && drift == 0) {
        return Err(format!(
            "frozen canonical request {name},drift{drift} acceptance mismatch: {request:?}"
        ));
    }
    if let Ok(r) = request {
        if r.canonical_bytes().map_err(error)? != bytes
            || r.model().as_str() != "fixture-model-v1"
            || r.profile_revision() != 7
            || r.request_id().expose_for_wire() != "caller-owned"
        {
            return Err("canonical request lost input semantics/profile binding".into());
        }
        if r.canonical_bytes_bounded(bytes.len()).map_err(error)? != bytes
            || r.canonical_bytes_bounded(bytes.len() - 1).is_ok()
        {
            return Err("canonical request capacity edge drift".into());
        }
    }
    Ok(crate::Metrics::from([
        (format!("request_corpus_{name}"), 1),
        (format!("request_dialect_tag_{}", dialect_tag.unwrap_or(0)), 1),
        (
            if accepted && drift == 0 {
                "accepted_canonical_requests"
            } else {
                "rejected_canonical_requests"
            }
            .into(),
            1,
        ),
    ]))
}
pub fn check(c: &Case) -> Result<crate::Metrics, String> {
    match c {
        Case::Stream {
            kinds,
            text,
            seed,
            chunk,
            defect,
            finish,
            reverse,
            event_limit,
            output_limit,
        } => stream(
            kinds,
            text,
            *seed,
            *chunk,
            *defect,
            *finish,
            *reverse,
            *event_limit,
            *output_limit,
        ),
        Case::ForkReplay { prefix, suffix, cut, chunk, repeats, fresh_sequence, tool } => {
            fork_replay(prefix, suffix, *cut, *chunk, *repeats, *fresh_sequence, *tool)
        }
        Case::Usage { observations } => usage(observations),
        Case::Corpus { index, drift, cut } => corpus(*index, *drift, *cut, None),
        Case::Dialect { tag } => corpus(0, 0, 0, Some(*tag)),
    }
}
// Two different continuations branch from the same partially assembled response. Expected
// bytes come from the inputs, never from either reducer branch. Retransmissions reuse provider
// identity/digest but exercise both original and fresh local envelope sequences.
#[allow(clippy::too_many_arguments)]
fn fork_replay(
    prefix: &str,
    suffix: &str,
    cut: u8,
    chunk: u8,
    repeats: u8,
    fresh_sequence: bool,
    tool: bool,
) -> Result<crate::Metrics, String> {
    let limits = ProtocolLimits::PRODUCTION;
    let payloads = [format!("{prefix}-left:{suffix}"), format!("{prefix}-right:{suffix}")];
    let bytes = payloads
        .iter()
        .map(|text| {
            if tool {
                serde_json::to_vec(&json!({"text": text})).map_err(error)
            } else {
                Ok(text.as_bytes().to_vec())
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let shared = bytes[0].iter().zip(&bytes[1]).take_while(|(a, b)| a == b).count();
    let split = usize::from(cut) % (shared + 1);
    let mut base =
        ResponseReducer::new(ProviderName::new("fixture".into()).map_err(error)?, limits);
    let mut sequence = 0_u64;
    let mut identity = 0_u64;
    let mut metrics = crate::Metrics::new();
    let mut emit = |reducer: &mut ResponseReducer,
                    sequence: &mut u64,
                    identity: &mut u64,
                    event: ModelEvent,
                    replay: bool|
     -> Result<(), String> {
        *sequence += 1;
        *identity += 1;
        let envelope = EventEnvelope::new(
            *sequence,
            Some(*identity),
            Some(EventId::new(format!("fork-{identity}")).map_err(error)?),
            Sha256Digest::new([(*identity % 251) as u8; 32]),
            event.clone(),
        )
        .map_err(error)?;
        reducer.push(envelope.clone()).map_err(error)?;
        if replay {
            for _ in 0..repeats {
                let duplicate = if fresh_sequence {
                    *sequence += 1;
                    EventEnvelope::new(
                        *sequence,
                        envelope.provider_sequence(),
                        envelope.provider_event_id().cloned(),
                        envelope.provider_digest(),
                        event.clone(),
                    )
                    .map_err(error)?
                } else {
                    envelope.clone()
                };
                if reducer.push(duplicate).map_err(error)? != ReducerTransition::DuplicateIgnored {
                    return Err("output-bearing retransmission was not ignored".into());
                }
                bump(&mut metrics, "replayed_output_fragments");
            }
        }
        Ok(())
    };
    emit(
        &mut base,
        &mut sequence,
        &mut identity,
        ModelEvent::ResponseStarted {
            response_id: Some(ResponseId::new("fork-response".into()).map_err(error)?),
            model: None,
        },
        false,
    )?;
    emit(
        &mut base,
        &mut sequence,
        &mut identity,
        ModelEvent::ItemStarted {
            item_id: item(0)?,
            index: 7,
            kind: if tool { ItemKind::ToolCall } else { ItemKind::Message },
        },
        false,
    )?;
    if tool {
        emit(
            &mut base,
            &mut sequence,
            &mut identity,
            ModelEvent::ToolCallStarted {
                item_id: item(0)?,
                call_id: call(0)?,
                name: ToolName::new("inspect".into()).map_err(error)?,
            },
            false,
        )?;
    }
    let delta = |bytes: &[u8]| -> Result<ModelEvent, String> {
        let fragment = StreamFragment::new(bytes.to_vec(), limits).map_err(error)?;
        Ok(if tool {
            ModelEvent::ToolArgumentDelta { call_id: call(0)?, fragment }
        } else {
            ModelEvent::TextDelta { item_id: item(0)?, fragment }
        })
    };
    if split > 0 {
        emit(&mut base, &mut sequence, &mut identity, delta(&bytes[0][..split])?, true)?;
    }
    let mut branches = [base.clone(), base.clone()];
    if base.finish_eof().is_ok() || terminal(base.terminal()) != "failed" {
        return Err("unfinished source branch did not fail on EOF".into());
    }
    for (index, branch) in branches.iter_mut().enumerate() {
        let mut branch_sequence = sequence;
        let mut branch_identity = identity;
        for fragment in bytes[index][split..].chunks(usize::from(chunk.max(1))) {
            emit(branch, &mut branch_sequence, &mut branch_identity, delta(fragment)?, true)?;
        }
        emit(
            branch,
            &mut branch_sequence,
            &mut branch_identity,
            ModelEvent::ItemCompleted(item(0)?),
            false,
        )?;
        emit(
            branch,
            &mut branch_sequence,
            &mut branch_identity,
            ModelEvent::Finish(if tool { FinishReason::ToolCalls } else { FinishReason::Stop }),
            false,
        )?;
        emit(
            branch,
            &mut branch_sequence,
            &mut branch_identity,
            ModelEvent::ResponseCompleted,
            false,
        )?;
        let expected = if tool {
            json!([2,"item-0",7,"call-0","inspect",{"text":payloads[index]}])
        } else {
            json!([0, "item-0", 7, payloads[index]])
        };
        if view(branch.completed_items()) != vec![expected]
            || terminal(branch.terminal()) != if tool { "action" } else { "success" }
        {
            return Err("forked response lost exact branch payload or terminal".into());
        }
    }
    if !base.completed_items().is_empty() || terminal(base.terminal()) != "failed" {
        return Err("finishing a clone changed the failed source response".into());
    }
    bump(&mut metrics, "independent_response_forks");
    bump(
        &mut metrics,
        if fresh_sequence {
            "replay_fresh_local_sequence"
        } else {
            "replay_original_local_sequence"
        },
    );
    bump(&mut metrics, if tool { "forked_tool_arguments" } else { "forked_text" });
    if split > 0 && std::str::from_utf8(&bytes[0][..split]).is_err() {
        bump(&mut metrics, "fork_inside_utf8");
    }
    Ok(metrics)
}
fn stream_case(defect: u8) -> Case {
    Case::Stream {
        kinds: vec![0, 1, 2, 3, 4, 5],
        text: "é🦀界".into(),
        seed: 17,
        chunk: 2,
        defect,
        finish: 2,
        reverse: true,
        event_limit: 4096,
        output_limit: 4096,
    }
}
pub fn examples() -> Vec<Case> {
    let mut cases = (0..17).map(stream_case).collect::<Vec<_>>();
    for tool in [false, true] {
        for fresh_sequence in [false, true] {
            for cut in [0, 1, 2, 3, 4, 5, 10, 11, 12] {
                cases.push(Case::ForkReplay {
                    prefix: "é🦀".into(),
                    suffix: "tail".into(),
                    cut,
                    chunk: 1,
                    repeats: 3,
                    fresh_sequence,
                    tool,
                });
            }
        }
    }
    for finish in 0..12 {
        let mut c = stream_case(0);
        if let Case::Stream { finish: f, .. } = &mut c {
            *f = finish;
        }
        cases.push(c);
    }
    for index in 0..10 {
        cases.push(Case::Corpus { index, drift: 0, cut: 0 });
    }
    for tag in 0..=10 {
        cases.push(Case::Dialect { tag });
    }
    for drift in 1..7 {
        cases.push(Case::Corpus { index: 1, drift, cut: 19 });
    }
    cases.push(Case::Usage {
        observations: vec![
            (1, [Some(9); 8]),
            (0, [Some(u64::MAX); 8]),
            (1, [None; 8]),
            (1, [Some(8); 8]),
            (2, [Some(10); 8]),
            (1, [Some(11); 8]),
            (0, [Some(1); 8]),
        ],
    });
    cases
}
pub fn calibration() -> Case {
    stream_case(2)
}
