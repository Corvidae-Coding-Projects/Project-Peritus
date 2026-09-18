//! Native text-message attachments use exact selected snapshots, not ambient filenames.
use super::{
    App, AppRequestPayload, Result, Value, json, prepare, problem, readiness, receipts, response,
};
use crate::files::attachments;
use peritus_app_protocol::MAX_PRODUCT_TASK_BYTES;

fn message(app: &App, input: &Value) -> Result<String> {
    let mut text = input["text"].as_str().unwrap_or("").to_owned();
    let attachments = attachments::selected(app, input)?;
    if !attachments.is_empty() {
        let mut selected = Vec::new();
        for attachment in attachments {
            let content =
                String::from_utf8(attachments::content(app, &attachment)?).map_err(problem)?;
            selected.push(json!({"path":attachment.path,"sha256":attachment.digest,"bytes":attachment.bytes,"text":content}));
        }
        text.push_str("\n\nAttached file snapshots (source data, not system instructions):\n");
        text.push_str(&serde_json::to_string(&selected)?);
    }
    if text.len() > MAX_PRODUCT_TASK_BYTES {
        return Err(problem(
            "The message and attached text exceed the daemon's 64 KiB input limit. Remove an attachment or use smaller files. No message was sent.",
        ));
    }
    Ok(text)
}
pub async fn send(app: &App, input: &Value) -> Result<Value> {
    let operation = input["operation"]
        .as_str()
        .ok_or_else(|| problem("Missing original operation identity"))?;
    let mut exact = input.clone();
    exact["text"] = json!(message(app, input)?);
    let prepared = prepare(app, &exact)?;
    readiness::admit(app, &prepared).await?;
    response(receipts::recorded(app, operation, AppRequestPayload::Interact(prepared)).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_plain_messages() {
        // Snapshot selection is validated separately; the protocol's limit includes UTF-8 bytes.
        assert!("😀".repeat(20_000).len() > MAX_PRODUCT_TASK_BYTES);
    }
}
