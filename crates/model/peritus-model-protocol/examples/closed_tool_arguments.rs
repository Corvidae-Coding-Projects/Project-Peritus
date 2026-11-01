//! Reject tool argument deltas after the owning item has completed.
use peritus_model_protocol::{
    EventEnvelope, ItemId, ItemKind, ModelEvent, ProtocolLimits, ProviderName, ResponseReducer,
    StreamFragment, TerminalOutcome, ToolCallId, ToolName,
};
use peritus_types::Sha256Digest;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limits =
        ProtocolLimits::new([32, 32, 256, 256, 256, 16, 256, 128, 16, 256, 512, 256, 256])?;
    let mut r = ResponseReducer::new(ProviderName::new("fixture".into())?, limits);
    let item = ItemId::new("item".into())?;
    let call = ToolCallId::new("call".into())?;
    let events = vec![
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind: ItemKind::ToolCall },
        ModelEvent::ToolCallStarted {
            item_id: item.clone(),
            call_id: call.clone(),
            name: ToolName::new("read".into())?,
        },
        ModelEvent::ToolArgumentDelta {
            call_id: call.clone(),
            fragment: StreamFragment::new(b"{}".to_vec(), limits)?,
        },
        ModelEvent::ItemCompleted(item),
    ];
    for (n, e) in events.into_iter().enumerate() {
        r.push(EventEnvelope::new(n as u64 + 1, None, None, Sha256Digest::new([0; 32]), e)?)?;
    }
    let late = ModelEvent::ToolArgumentDelta {
        call_id: call,
        fragment: StreamFragment::new(b"incomplete".to_vec(), limits)?,
    };
    let observed = r.push(EventEnvelope::new(6, None, None, Sha256Digest::new([0; 32]), late)?);
    println!(
        "late_arguments={observed:?}; terminal={:?}; completed={}",
        r.terminal(),
        r.completed_items().len()
    );
    if observed.is_ok() {
        return Err("tool arguments after ItemCompleted were accepted".into());
    }
    if !matches!(r.terminal(), Some(TerminalOutcome::Failed(_))) {
        return Err("late argument rejection did not make stream terminal".into());
    }
    Ok(())
}
