//! Read-only reconciliation and explicit human acknowledgement of unprovable outcomes.
use crate::{
    daemon,
    error::{Result, problem},
    files,
    state::{App, Workspace, native_operation_belongs_to},
};
use serde_json::{Value, json};

pub fn pending(app: &App) -> Result<Value> {
    Ok(json!(app.snapshot()?.operations.iter().filter(|(_,r)|r.result.is_none()&&r.input["command"]!="daemon-request").map(|(id,r)|json!({"operation":id,"command":r.input["command"],"session":r.input["session"],"project":r.input["project"],"path":r.input["path"]})).collect::<Vec<_>>()))
}
pub async fn observe(app: &App, id: &str) -> Result<Value> {
    let Some(mut record) = app.snapshot()?.operations.get(id).cloned() else {
        return Ok(Value::Null);
    };
    if record.result.is_none() {
        let result = if record.input["command"] == "file-save" {
            let check = || -> Result<Option<Value>> {
                let root = app.project(record.input["project"].as_str().unwrap_or(""))?.root;
                let path = files::resolve(&root, record.input["path"].as_str().unwrap_or(""))?;
                let bytes = files::read_text(&path)?;
                let hash = files::revision(&bytes);
                Ok((record.input["hash"] == hash)
                    .then(|| json!({"revision":hash,"bytes":bytes.len(),"recovered":true})))
            };
            check().unwrap_or(None)
        } else if record.input["command"] == "git" {
            crate::git::recover(app, id, &record.input).await?
        } else if record.input["command"] == "send" {
            match daemon::recover_send(app, id).await {
                Ok(result) => result,
                Err(error) if error.1 => return Err(error),
                Err(error) => Some(json!({"error":error.0,"recovered":true})),
            }
        } else {
            daemon::receipts::observed(app, id).await?
        };
        if let Some(result) = result {
            let result =
                if record.input["command"] == "session-settings" && result.get("error").is_none() {
                    crate::sessions::save(app, &record.input, &result)?
                } else {
                    result
                };
            app.update(|state| {
                state.operations.get_mut(id).ok_or_else(|| problem("Operation missing"))?.result =
                    Some(result.clone());
                Ok(())
            })?;
            record.result = Some(result);
        }
    }
    Ok(json!(record))
}
pub async fn acknowledge(app: &App, id: &str) -> Result<Value> {
    let observed = observe(app, id).await?;
    if !observed["result"].is_null() {
        return Ok(observed);
    }
    if app.owned_operations.lock().map_err(problem)?.contains(id) {
        return Err(problem("The original operation still has a live execution owner"));
    }
    if let Some(record) = app.snapshot()?.operations.get(id)
        && record.input["command"] == "git"
        && let Ok(command) = crate::processes::ManagedCommand::get(app, &format!("git:{id}"))
        && command.observe()?.state() == peritus_product_runner::PreviewProcessState::Running
    {
        return Err(problem(
            "Git is still running; cancel it or wait for its original outcome before clearing the hold",
        ));
    }
    app.update(|state| acknowledge_review(state, id))?;
    observe(app, id).await
}

fn acknowledge_review(state: &mut Workspace, id: &str) -> Result<()> {
    let record = state.operations.get(id).ok_or_else(|| problem("Operation not found"))?;
    if record.input["command"] == "daemon-request" {
        return Err(problem("Review the parent operation, not its transport record"));
    }
    // Manual review is the terminal disposition for an unprovable native request. Retaining its
    // unresolved transport child forever would eventually fill the durable operation ledger even
    // though the user had already resolved the parent ambiguity.
    state.operations.retain(|key, _| !native_operation_belongs_to(key, id));
    state.operations.get_mut(id).ok_or_else(|| problem("Operation not found"))?.result = Some(
        json!({"reviewed":true,"error":"Outcome manually reviewed by the user. No acceptance was inferred and the operation was not repeated."}),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Operation;

    #[test]
    fn manual_review_settles_parent_and_retires_unresolved_transport_child() {
        let mut workspace = Workspace::default();
        workspace.operations.insert(
            "original".into(),
            Operation { input: json!({"command":"send"}), prepared: None, result: None },
        );
        workspace.operations.insert(
            "daemon:original".into(),
            Operation { input: json!({"command":"daemon-request"}), prepared: None, result: None },
        );
        workspace.operations.insert(
            "daemon:original:workbench:create".into(),
            Operation {
                input: json!({"command":"daemon-request"}),
                prepared: None,
                result: Some(json!({"accepted":true})),
            },
        );
        workspace.operations.insert(
            "daemon:original-other".into(),
            Operation { input: json!({"command":"daemon-request"}), prepared: None, result: None },
        );

        acknowledge_review(&mut workspace, "original").expect("manual review");

        assert!(workspace.operations["original"].result.is_some());
        assert!(!workspace.operations.contains_key("daemon:original"));
        assert!(!workspace.operations.contains_key("daemon:original:workbench:create"));
        assert!(workspace.operations.contains_key("daemon:original-other"));
    }
}
