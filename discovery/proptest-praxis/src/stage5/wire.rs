//! Closed A3 dispatch, frozen compatibility bytes and independent semantic/binding checks.
#[path = "wire_labels.rs"]
mod labels;
use crate::Metrics;
use peritus_app_protocol::{
    AppErrorCode as Code, AppEventPayload as Event, AppMessage, AppProtocolError,
    AppProtocolLimits, AppRequestPayload as Request, AppResponsePayload as Response,
    ControlPayload as Control, ProtocolContext, decode_app_message, encode_app_message,
};
use peritus_codec::CodecLimits;
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const CASE_COUNT: u16 = 212;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Case {
    Corpus { index: u16, mutation: u8, operand: u16 },
    Binding { kind: u8 },
    Diagnostic { text: String, limit: u8 },
}
#[derive(Clone, Debug, Deserialize)]
struct Row {
    name: String,
    hex: String,
    accepted: bool,
    family: Option<u16>,
    error: Option<String>,
    tag: u16,
    variant: String,
    intent_tag: Option<u16>,
    intent_variant: Option<String>,
}
fn rows() -> Result<&'static [Row], String> {
    static ROWS: OnceLock<Result<Vec<Row>, String>> = OnceLock::new();
    ROWS.get_or_init(|| {
        serde_json::from_str(include_str!("wire_cases.json"))
            .map_err(|e| format!("setup-incomplete: frozen wire corpus {e}"))
    })
    .as_ref()
    .map(Vec::as_slice)
    .map_err(Clone::clone)
}
fn bump(metrics: &mut Metrics, key: &str) {
    *metrics.entry(key.into()).or_default() += 1;
}
pub fn strategy(_max_steps: usize) -> BoxedStrategy<Case> {
    prop_oneof![
        8=>(0_u16..CASE_COUNT,0_u8..14,any::<u16>()).prop_map(|(index,mutation,operand)|Case::Corpus{index,mutation,operand}),
        2=>(0_u8..5).prop_map(|kind|Case::Binding{kind}),
        3=>(prop_oneof![Just(String::new()),Just("é👩‍💻\r\n".into()),prop::collection::vec(any::<char>(),1..25).prop_map(|v|v.into_iter().collect())],1_u8..128).prop_map(|(text,limit)|Case::Diagnostic{text,limit}),
    ].boxed()
}
fn hex(text: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(2) {
        return Err("setup-incomplete: odd frozen hex length".into());
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).map_err(|e| e.to_string())?;
            u8::from_str_radix(text, 16).map_err(|e| e.to_string())
        })
        .collect()
}
fn slice(input: &[u8], start: usize, length: usize) -> Result<&[u8], String> {
    input
        .get(start..start + length)
        .ok_or_else(|| "setup-incomplete: scalar grammar exceeds source input".into())
}
fn number(input: &[u8], start: usize, width: usize) -> Result<u64, String> {
    Ok(slice(input, start, width)?.iter().fold(0_u64, |n, byte| (n << 8) | u64::from(*byte)))
}
fn blob(input: &[u8], start: usize) -> Result<&[u8], String> {
    slice(input, start + 4, usize::try_from(number(input, start, 4)?).map_err(|e| e.to_string())?)
}
fn string(input: &[u8], start: usize) -> Result<&str, String> {
    std::str::from_utf8(blob(input, start)?).map_err(|e| e.to_string())
}
fn require<T: std::fmt::Debug>(
    actual: Result<T, AppProtocolError>,
    code: Code,
) -> Result<(), String> {
    match actual {
        Err(error) if error.code() == code => Ok(()),
        other => Err(format!("expected A3 {code:?}, observed {other:?}")),
    }
}
fn expected_error(name: &str) -> Result<Code, String> {
    Ok(match name {
        "unsupported-family" => Code::UnsupportedFamily,
        "unsupported-schema" => Code::UnsupportedSchema,
        "truncated-frame" => Code::TruncatedFrame,
        "unknown-tag" => Code::UnknownTag,
        _ => return Err(format!("setup-incomplete: unrecognized corpus expectation {name}")),
    })
}
fn context(actual: ProtocolContext, input: &[u8]) -> Result<(), String> {
    if actual.protocol_id().as_bytes() != slice(input, 16, 16)?
        || u64::from(actual.version().major()) != number(input, 32, 2)?
        || u64::from(actual.version().minor()) != number(input, 34, 2)?
        || actual.session_id().as_bytes() != slice(input, 36, 16)?
    {
        return Err("A3 context changed independently supplied identity/version/session".into());
    }
    Ok(())
}
fn terminal(
    binding: peritus_app_protocol::TerminalBinding,
    input: &[u8],
    start: usize,
) -> Result<(), String> {
    if binding.attachment_id().as_bytes() != slice(input, start, 16)?
        || binding.process_id().as_bytes() != slice(input, start + 16, 16)?
        || binding.originating_request_id().as_bytes() != slice(input, start + 32, 16)?
    {
        return Err("terminal binding fields differ from scalar input".into());
    }
    Ok(())
}
fn artifact_chunk(
    actual: &peritus_app_protocol::ArtifactChunk,
    input: &[u8],
    start: usize,
) -> Result<(), String> {
    if actual.transfer_id().as_bytes() != slice(input, start, 16)?
        || actual.artifact_id().as_bytes() != slice(input, start + 16, 16)?
        || actual.ordinal() != number(input, start + 32, 8)?
        || actual.offset() != number(input, start + 40, 8)?
        || actual.bytes() != blob(input, start + 48)?
    {
        return Err("artifact chunk lost exact identities/ordinal/offset/binary content".into());
    }
    Ok(())
}
fn metadata(
    actual: &peritus_app_protocol::ArtifactMetadata,
    input: &[u8],
    start: usize,
) -> Result<(), String> {
    let media = blob(input, start + 40)?;
    let digest = start + 44 + media.len();
    if actual.transfer_id().as_bytes() != slice(input, start, 16)?
        || actual.artifact_id().as_bytes() != slice(input, start + 16, 16)?
        || actual.byte_size() != number(input, start + 32, 8)?
        || actual.media_type().as_str() != std::str::from_utf8(media).map_err(|e| e.to_string())?
        || actual.digest().as_bytes() != slice(input, digest, 32)?
        || u64::from(actual.preferred_chunk_size()) != number(input, digest + 32, 4)?
    {
        return Err("artifact metadata differs from independent complete scalar bindings".into());
    }
    Ok(())
}
fn completion(
    actual: peritus_app_protocol::ArtifactCompletion,
    input: &[u8],
    start: usize,
) -> Result<(), String> {
    if actual.transfer_id().as_bytes() != slice(input, start, 16)?
        || actual.artifact_id().as_bytes() != slice(input, start + 16, 16)?
        || actual.byte_size() != number(input, start + 32, 8)?
        || actual.digest().as_bytes() != slice(input, start + 40, 32)?
    {
        return Err("artifact completion fields changed".into());
    }
    Ok(())
}
fn request_semantics(value: &Request, input: &[u8]) -> Result<(), String> {
    let start = 86;
    match value {
        Request::OpenArtifact(value) => {
            if value.transfer_id().as_bytes() != slice(input, start, 16)?
                || value.artifact_id().as_bytes() != slice(input, start + 16, 16)?
            {
                return Err("open artifact identity mismatch".into());
            }
        }
        Request::BeginArtifactUpload(value) => metadata(value, input, start)?,
        Request::UploadArtifactChunk(value) => artifact_chunk(value, input, start)?,
        Request::CompleteArtifactUpload(value) => completion(*value, input, start)?,
        Request::AttachTerminal(value) => terminal(*value, input, start)?,
        Request::TerminalInput(value) => {
            terminal(value.binding(), input, start)?;
            if value.bytes() != blob(input, start + 48)? {
                return Err("terminal input bytes changed".into());
            }
        }
        Request::TerminalResize(value) => {
            terminal(value.binding(), input, start)?;
            if u64::from(value.columns()) != number(input, start + 48, 2)?
                || u64::from(value.rows()) != number(input, start + 50, 2)?
            {
                return Err("terminal dimensions changed".into());
            }
        }
        Request::QueryWorkbench(value)
        | Request::QueryWorkbenchExecution(value)
        | Request::QueryWorkbenchBrief(value)
        | Request::QueryWorkbenchPermissions(value)
        | Request::QueryWorkbenchGoal(value) => {
            if value.conversation().as_bytes() != slice(input, start, 16)?
                || value.workspace().as_bytes() != slice(input, start + 16, 16)?
            {
                return Err("workbench query scope changed".into());
            }
        }
        Request::WorkbenchCommand(value) | Request::QueryWorkbenchReceipt(value) => {
            if value.operation().as_bytes() != slice(input, start, 16)?
                || value.query().conversation().as_bytes() != slice(input, start + 16, 16)?
                || value.query().workspace().as_bytes() != slice(input, start + 32, 16)?
                || value.expected_revision() != number(input, start + 48, 8)?
            {
                return Err("workbench operation/query/revision changed".into());
            }
        }
        Request::UpdateModels(value) => {
            if value.run_id().as_bytes() != slice(input, start, 16)? {
                return Err("model update lost run selection".into());
            }
            let mut offset = start + 16;
            let tag = number(input, 84, 2)?;
            if value.providers().is_some() != (tag == 61) {
                return Err("provider replacement presence changed".into());
            }
            let mut efforts = tag == 27;
            if let Some(providers) = value.providers() {
                for provider in [providers.writer(), providers.reviewer(), providers.fixer()] {
                    if provider.as_bytes() != slice(input, offset, 16)? {
                        return Err("provider route changed".into());
                    }
                    offset += 16;
                }
                efforts = number(input, offset, 1)? == 1;
                offset += 1;
            }
            if value.models().has_effort() != efforts {
                return Err("effort form presence changed".into());
            }
            for choice in
                [value.models().writer(), value.models().reviewer(), value.models().fixer()]
            {
                let text = string(input, offset)?;
                offset += 4 + text.len();
                if choice.id() != text || choice.manual() != (number(input, offset, 1)? == 1) {
                    return Err("selected role model/manual flag changed".into());
                }
                offset += 1;
                if efforts {
                    if u64::from(choice.effort().tag()) != number(input, offset, 2)? {
                        return Err("selected role model effort changed".into());
                    }
                    offset += 2;
                }
            }
        }
        _ => {}
    }
    Ok(())
}
fn event_semantics(value: &Event, input: &[u8]) -> Result<(), String> {
    let start = 54;
    match value {
        Event::Diagnostic(value) => {
            if value.as_str() != string(input, start)? {
                return Err("diagnostic UTF8 bytes changed".into());
            }
        }
        Event::ArtifactMetadata(value) => metadata(value, input, start)?,
        Event::ArtifactChunk(value) => artifact_chunk(value, input, start)?,
        Event::ArtifactComplete(value) => completion(*value, input, start)?,
        Event::TerminalOutput(value) => {
            terminal(value.binding(), input, start)?;
            let stream = match value.stream() {
                peritus_app_protocol::TerminalStream::Stdout => 1,
                peritus_app_protocol::TerminalStream::Stderr => 2,
                peritus_app_protocol::TerminalStream::Terminal => 3,
            };
            if value.sequence() != number(input, start + 48, 8)?
                || value.offset() != number(input, start + 56, 8)?
                || stream != number(input, start + 64, 1)?
                || value.bytes() != blob(input, start + 65)?
            {
                return Err("terminal output sequence/offset/binary bytes changed".into());
            }
        }
        Event::TerminalUnavailable(value) => terminal(*value, input, start)?,
        Event::Backpressure(value) => {
            if value.subscription_id().as_bytes() != slice(input, start, 16)?
                || value.last_delivered().get() != number(input, start + 16, 8)?
                || value.last_acknowledged().get() != number(input, start + 24, 8)?
                || u64::from(value.maximum_in_flight()) != number(input, start + 32, 4)?
            {
                return Err("backpressure identities/window/cursors changed".into());
            }
        }
        Event::SubscriptionGap { subscription_id, gap } => {
            if subscription_id.as_bytes() != slice(input, start, 16)?
                || gap.requested().get() != number(input, start + 16, 8)?
                || gap.earliest().get() != number(input, start + 24, 8)?
                || gap.latest().get() != number(input, start + 32, 8)?
            {
                return Err("subscription retention gap changed".into());
            }
        }
        Event::DomainEvent(value)
            if value.subscription_id().as_bytes() != slice(input, start, 16)?
                || value.event_id().as_bytes() != slice(input, start + 16, 16)?
                || value.cursor().get() != number(input, start + 32, 8)?
                || value.attempt_id().as_bytes() != slice(input, start + 40, 16)?
                || u64::from(value.attempt()) != number(input, start + 56, 4)?
                || value.frame().bytes() != blob(input, start + 60)? =>
        {
            return Err(
                "at-least-once delivery identity/cursor/attempt/exact embedded frame changed"
                    .into(),
            );
        }
        _ => {}
    }
    Ok(())
}
fn semantics(actual: &AppMessage, input: &[u8], row: &Row, m: &mut Metrics) -> Result<(), String> {
    if Some(actual.family()) != row.family {
        return Err(format!("{} decoded as wrong family", row.name));
    }
    let actual_name = match actual {
        AppMessage::ClientHello(value) => {
            if value.protocol_id().as_bytes() != slice(input, 18, 16)? {
                return Err("clienthello protocol identity changed".into());
            }
            "ClientHello"
        }
        AppMessage::ServerHello(value) => {
            if value.protocol_id().as_bytes() != slice(input, 16, 16)? {
                return Err("serverhello protocol identity changed".into());
            }
            "ServerHello"
        }
        AppMessage::Request(value) => {
            context(value.context(), input)?;
            if value.request_id().as_bytes() != slice(input, 52, 16)?
                || value.correlation_id().as_bytes() != slice(input, 68, 16)?
            {
                return Err("request envelope identities changed".into());
            }
            request_semantics(value.payload(), input)?;
            if let Request::WorkbenchCommand(command) | Request::QueryWorkbenchReceipt(command) =
                value.payload()
            {
                if Some(labels::intent(command.intent())) != row.intent_variant.as_deref() {
                    return Err("workbench nested intent dispatch differs from independent stable tag table".into());
                }
                bump(
                    m,
                    &format!(
                        "workbench_intent_{}",
                        row.intent_tag.ok_or("missing frozen intent tag")?
                    ),
                );
            }
            labels::request(value.payload())
        }
        AppMessage::Response(value) => {
            context(value.context(), input)?;
            if value.request_id().as_bytes() != slice(input, 52, 16)?
                || value.correlation_id().as_bytes() != slice(input, 68, 16)?
            {
                return Err("response envelope identities changed".into());
            }
            match value.payload() {
                Response::ArtifactOpened(v) => metadata(v, input, 86)?,
                Response::TerminalAttached(v) | Response::TerminalPipeAttached(v) => {
                    terminal(*v, input, 86)?
                }
                Response::PromptAccepted(v) => {
                    if v.as_bytes() != slice(input, 86, 16)? {
                        return Err("accepted prompt identity changed".into());
                    }
                }
                Response::Acknowledged(v) if v.request_id().as_bytes() != slice(input, 86, 16)? => {
                    return Err("acknowledged request changed".into());
                }
                _ => {}
            }
            labels::response(value.payload())
        }
        AppMessage::Event(value) => {
            context(value.context(), input)?;
            event_semantics(value.payload(), input)?;
            labels::event(value.payload())
        }
        AppMessage::Control(value) => {
            context(value.context(), input)?;
            if value.correlation_id().as_bytes() != slice(input, 52, 16)? {
                return Err("control correlation changed".into());
            }
            if let Control::Acknowledge(v) = value.payload()
                && (v.subscription_id().as_bytes() != slice(input, 70, 16)?
                    || v.cursor().get() != number(input, 86, 8)?)
            {
                return Err("cumulative acknowledgement identity/cursor changed".into());
            }
            if let Control::Subscription(v) = value.payload() {
                let (tag, subscription) = match v {
                    peritus_app_protocol::SubscriptionControl::Pause {
                        subscription_id, ..
                    } => (1, subscription_id),
                    peritus_app_protocol::SubscriptionControl::Resume { subscription_id } => {
                        (2, subscription_id)
                    }
                };
                if tag != number(input, 70, 1)? || subscription.as_bytes() != slice(input, 71, 16)?
                {
                    return Err("subscription pause/resume identity changed".into());
                }
            }
            if let Control::HeartbeatReply(v) = value.payload()
                && (v.heartbeat_id().as_bytes() != slice(input, 70, 16)?
                    || v.sequence() != number(input, 86, 8)?)
            {
                return Err("heartbeat nonce/sequence changed".into());
            }
            labels::control(value.payload())
        }
    };
    if actual_name != row.variant {
        return Err(format!(
            "{} tag{} decoded as {actual_name}, expected{}",
            row.name, row.tag, row.variant
        ));
    }
    bump(m, &format!("wire_family_{}_tag_{}", actual.family(), row.tag));
    bump(m, "independent_envelope_and_variant_checks");
    Ok(())
}
fn tag_offset(input: &[u8], family: u16) -> Result<usize, String> {
    Ok(match family {
        94 => 16,
        95 => {
            let length = usize::try_from(number(input, 32, 4)?).map_err(|e| e.to_string())?;
            let presence = 36 + length;
            presence + 1 + if number(input, presence, 1)? == 1 { 16 } else { 0 }
        }
        96 | 97 => 84,
        98 => 52,
        99 => 68,
        _ => return Err("setup-incomplete: no closed A3 tag".into()),
    })
}
fn corpus(index: u16, mutation: u8, operand: u16, m: &mut Metrics) -> Result<(), String> {
    let all = rows()?;
    if all.len() != usize::from(CASE_COUNT) {
        return Err("setup-incomplete: frozen corpus count mismatch".into());
    }
    let row =
        all.get(usize::from(index)).ok_or("setup-incomplete: unknown wire corpus identity")?;
    let mut input = hex(&row.hex)?;
    let family = row.family.unwrap_or(0);
    if !row.accepted {
        require(
            decode_app_message(&input, AppProtocolLimits::PRODUCTION),
            expected_error(row.error.as_deref().ok_or("missing negative expectation")?)?,
        )?;
        bump(m, "retained_negative_compatibility_cases");
        return Ok(());
    }
    let expected = match mutation % 14 {
        0 => None,
        1 => {
            let length = usize::from(operand) % input.len();
            input.truncate(length);
            Some(Code::TruncatedFrame)
        }
        2 => {
            input.push(127);
            Some(Code::TrailingBytes)
        }
        3 => {
            input.push(127);
            let length = u32::try_from(input.len() - 16).map_err(|e| e.to_string())?;
            input[12..16].copy_from_slice(&length.to_be_bytes());
            Some(Code::TrailingBytes)
        }
        4 => {
            input[6..8].copy_from_slice(&93_u16.to_be_bytes());
            Some(Code::UnsupportedFamily)
        }
        5 => {
            input[8..10].copy_from_slice(&2_u16.to_be_bytes());
            Some(Code::UnsupportedSchema)
        }
        6 => {
            let offset = tag_offset(&input, family)?;
            input[offset..offset + 2].copy_from_slice(&u16::MAX.to_be_bytes());
            Some(Code::UnknownTag)
        }
        7 => {
            input[4..6].copy_from_slice(&2_u16.to_be_bytes());
            Some(Code::UnsupportedFormat)
        }
        8 => {
            input[10] = 1;
            Some(Code::MalformedFrame)
        }
        9 => {
            let offset = if family == 94 { 18 } else { 16 };
            input[offset..offset + 16].fill(0);
            Some(Code::MalformedFrame)
        }
        10 => {
            let offset = if family == 94 { 18 } else { 16 };
            input[offset..offset + 16]
                .fill(1 + u8::try_from(operand % 254).map_err(|e| e.to_string())?);
            None
        }
        11 => {
            input[12..16].copy_from_slice(&u32::MAX.to_be_bytes());
            Some(Code::LimitExceeded)
        }
        12 => {
            let offset = if family >= 96 {
                36
            } else if family == 94 {
                18
            } else {
                16
            };
            input[offset..offset + 16].fill(0);
            Some(Code::MalformedFrame)
        }
        _ => {
            let offset = if family == 96 || family == 97 || family == 99 {
                52
            } else if family == 98 {
                36
            } else if family == 94 {
                18
            } else {
                16
            };
            input[offset..offset + 16].fill(0);
            Some(Code::MalformedFrame)
        }
    };
    let result = decode_app_message(&input, AppProtocolLimits::PRODUCTION);
    if let Some(code) = expected {
        require(result, code)?;
        bump(m, &format!("malformed_application_partition_{}", mutation % 14));
    } else {
        let actual = result.map_err(|e| format!("valid {} rejected: {e:?}", row.name))?;
        semantics(&actual, &input, row, m)?;
        if encode_app_message(&actual, AppProtocolLimits::PRODUCTION).map_err(|e| e.to_string())?
            != input
        {
            return Err(format!("{} canonical bytes changed after semantic decode", row.name));
        }
        bump(
            m,
            if row.name.starts_with("constructed") {
                "constructed_variant_forms"
            } else {
                "retained_positive_compatibility_cases"
            },
        );
    }
    Ok(())
}
fn binding(kind: u8, m: &mut Metrics) -> Result<(), String> {
    let (name, offset) = match kind % 5 {
        0 => ("constructed-96-7", 52),
        1 => ("constructed-96-4", 68),
        2 => ("constructed-96-5", 36),
        3 => ("constructed-97-6", 52),
        _ => ("constructed-99-5", 52),
    };
    let row =
        rows()?.iter().find(|r| r.name == name).ok_or("setup-incomplete: binding seed absent")?;
    let mut input = hex(&row.hex)?;
    let original = decode_app_message(&input, AppProtocolLimits::PRODUCTION)
        .map_err(|e| format!("binding positive control {name}: {e:?}"))?;
    semantics(&original, &input, row, m)?;
    input[offset..offset + 16].fill(177);
    require(decode_app_message(&input, AppProtocolLimits::PRODUCTION), Code::MalformedFrame)?;
    bump(m, &format!("duplicated_binding_mismatch_{}", kind % 5));
    Ok(())
}
fn diagnostic(text: &str, limit: u8, m: &mut Metrics) -> Result<(), String> {
    if text.len() > 1024 || limit == 0 {
        return Err("setup-incomplete: diagnostic input bound".into());
    }
    let policy = AppProtocolLimits::new(
        CodecLimits::PRODUCTION,
        16,
        64,
        4096,
        64,
        256,
        256 * 1024,
        64,
        64 * 1024,
        usize::from(limit),
        256,
    )
    .map_err(|e| e.to_string())?;
    let mut payload = vec![1; 16];
    payload.extend([0, 1, 0, 0]);
    payload.extend([2; 16]);
    payload.extend(11_u16.to_be_bytes());
    payload.extend(u32::try_from(text.len()).map_err(|e| e.to_string())?.to_be_bytes());
    payload.extend(text.as_bytes());
    let mut input = b"PRTS".to_vec();
    for value in [1_u16, 98, 1, 0] {
        input.extend(value.to_be_bytes());
    }
    input.extend(u32::try_from(payload.len()).map_err(|e| e.to_string())?.to_be_bytes());
    input.extend(payload);
    let actual = decode_app_message(&input, policy);
    if text.is_empty() {
        require(actual, Code::MalformedFrame)?;
        bump(m, "empty_diagnostic_rejections");
    } else if text.len() > usize::from(limit) {
        require(actual, Code::LimitExceeded)?;
        bump(m, "negotiated_diagnostic_byte_rejections");
    } else {
        let actual = actual.map_err(|e| e.to_string())?;
        if !matches!(&actual,AppMessage::Event(event) if matches!(event.payload(),Event::Diagnostic(value) if value.as_str()==text))
        {
            return Err("negotiated diagnostic changed Unicode prose".into());
        }
        if encode_app_message(&actual, policy).map_err(|e| e.to_string())? != input {
            return Err("bounded diagnostic encoding changed".into());
        }
        bump(m, "negotiated_diagnostic_successes");
    }
    Ok(())
}
pub fn check(case: &Case) -> Result<Metrics, String> {
    let mut metrics = Metrics::new();
    match case {
        Case::Corpus { index, mutation, operand } => {
            corpus(*index, *mutation, *operand, &mut metrics)?
        }
        Case::Binding { kind } => binding(*kind, &mut metrics)?,
        Case::Diagnostic { text, limit } => diagnostic(text, *limit, &mut metrics)?,
    }
    Ok(metrics)
}
pub fn calibration() -> Case {
    Case::Binding { kind: 0 }
}
pub fn examples() -> Vec<Case> {
    let mut cases = (0..CASE_COUNT)
        .map(|index| Case::Corpus { index, mutation: 0, operand: 0 })
        .collect::<Vec<_>>();
    for kind in 0..5 {
        cases.push(Case::Binding { kind });
    }
    for (text, limit) in [("", 1), ("a", 1), ("é", 1), ("é", 2), ("é", 3), ("👩‍💻", 10), ("👩‍💻", 11)]
    {
        cases.push(Case::Diagnostic { text: text.into(), limit });
    }
    cases
}
